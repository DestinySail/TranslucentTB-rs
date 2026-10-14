use std::{cell::RefCell, collections::HashMap};
use windows::{core::{w, IInspectable, Interface, PCWSTR, Result}, Win32::{Foundation::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*}};
use clearbar_common::{controller_alive, is_signaled, log, signal};
use crate::abi::*;

struct Node { parent: u64, kind: String, generation: u64 }
struct SavedOpacity { element: IUIElement, original: f64, generation: u64, background: bool }
struct State {
    diagnostics: IXamlDiagnostics,
    nodes: HashMap<u64, Node>,
    targets: HashMap<u64, SavedOpacity>,
    active: bool,
}
thread_local! {
    // XAML handles and interface calls stay on the UI thread that delivered the callback.
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_TIMER {
        STATE.with(|slot| {
            // COM calls may pump messages. Never panic or hold a RefCell borrow across reentry.
            if let Ok(mut guard) = slot.try_borrow_mut() {
                if let Some(state) = guard.as_mut() { state.tick(); }
            }
        });
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn initialize(diagnostics: &IXamlDiagnostics) -> Result<State> {
    unsafe {
        let mut instance = HMODULE::default();
        // Scope the class to this DLL, not explorer.exe. Otherwise an updated DLL
        // would reuse an older module's window procedure and its unrelated TLS.
        GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(window_proc as *const u16), &mut instance)?;
        let class = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance.into(),
            lpszClassName: w!("Clearbar.XAML.UIThread.0.1"), ..Default::default() };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class.lpszClassName, w!("Clearbar"),
            WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, instance, None)?;
        if SetTimer(hwnd, 1, 500, None) == 0 { return Err(windows::core::Error::from_win32()); }
        Ok(State { diagnostics: diagnostics.clone(), nodes: HashMap::new(), targets: HashMap::new(), active: false })
    }
}

pub fn change(diagnostics: &IXamlDiagnostics, relation: ParentChildRelation, element: &VisualElement, mutation: i32) {
    STATE.with(|slot| {
        let Ok(mut guard) = slot.try_borrow_mut() else { return };
        if guard.is_none() {
            match initialize(diagnostics) {
                Ok(state) => *guard = Some(state),
                Err(e) => { log("tap", format!("UI timer initialization: {e}")); return; }
            }
        }
        let state = guard.as_mut().unwrap();
        if mutation == 1 {
            state.targets.remove(&element.Handle);
            state.nodes.remove(&element.Handle);
            return;
        }
        let kind = element.Type.to_string();
        let name = element.Name.to_string();
        let generation = crate::SCAN.load(std::sync::atomic::Ordering::Acquire);
        state.nodes.insert(element.Handle, Node { parent: relation.Parent, kind: kind.clone(), generation });
        if kind == "Windows.UI.Xaml.Shapes.Rectangle" && matches!(name.as_str(), "BackgroundFill" | "BackgroundStroke") {
            // Restrict the change to a proven TaskbarFrame ancestor, never similarly named app controls.
            let mut parent = relation.Parent;
            let mut taskbar = false;
            for _ in 0..64 {
                let Some(node) = state.nodes.get(&parent) else { break };
                if node.kind == "Taskbar.TaskbarFrame" { taskbar = true; break; }
                parent = node.parent;
            }
            if let Some(value) = state.targets.get_mut(&element.Handle) { value.generation = generation; }
            if taskbar && !state.targets.contains_key(&element.Handle) {
                match unsafe { state.capture(element.Handle, name == "BackgroundFill") } {
                    Ok(value) => { state.targets.insert(element.Handle, value); log("tap", format!("Captured {name} handle={}", element.Handle)); }
                    Err(e) => log("tap", format!("Capture {name}: {e}")),
                }
            }
        }
        // Defer writes until the initial Diagnostics enumeration has finished.
    });
}

impl State {
    unsafe fn capture(&self, handle: u64, background: bool) -> Result<SavedOpacity> {
        let mut raw = std::ptr::null_mut();
        self.diagnostics.GetIInspectableFromHandle(handle, &mut raw).ok()?;
        let object = IInspectable::from_raw(raw);
        let element: IUIElement = object.cast()?;
        let mut original = 0.0;
        element.GetOpacity(&mut original).ok()?;
        log("tap", format!("Original opacity handle={handle} value={original}"));
        Ok(SavedOpacity { element, original, generation: crate::SCAN.load(std::sync::atomic::Ordering::Acquire), background })
    }

    fn tick(&mut self) {
        if !crate::SNAPSHOT_READY.load(std::sync::atomic::Ordering::Acquire) { return; }
        let generation = crate::COMPLETED_SCAN.load(std::sync::atomic::Ordering::Acquire);
        self.targets.retain(|_, saved| {
            if saved.generation >= generation { return true; }
            unsafe { let _ = saved.element.SetOpacity(saved.original); }
            false
        });
        self.nodes.retain(|_, node| node.generation >= generation);
        if self.targets.is_empty() { return; }
        let enabled = controller_alive() && !is_signaled("stop");
        if !enabled && !self.active { return; }
        let mut successful = 0;
        let mut backgrounds = 0;
        for saved in self.targets.values() {
            let desired = if enabled { 0.0 } else { saved.original };
            let result: Result<()> = unsafe {
                (|| {
                    let mut actual = -1.0;
                    saved.element.GetOpacity(&mut actual).ok()?;
                    if (actual - desired).abs() > f64::EPSILON { saved.element.SetOpacity(desired).ok()?; }
                    saved.element.GetOpacity(&mut actual).ok()?;
                    if (actual - desired).abs() > f64::EPSILON { return Err(E_FAIL.into()); }
                    Ok(())
                })()
            };
            if result.is_ok() { successful += 1; if saved.background { backgrounds += 1; } }
        }
        if enabled {
            signal("restored", false); signal("applied", backgrounds > 0);
        } else if !enabled {
            signal("applied", false); signal("restored", successful == self.targets.len());
        }
        if enabled != self.active {
            log("tap", format!("{} {successful}/{} backgrounds", if enabled { "Transparent" } else { "Restored" }, self.targets.len()));
            // Keep retrying restoration if one UI object temporarily rejected the call.
            self.active = enabled || successful != self.targets.len();
        }
    }
}
