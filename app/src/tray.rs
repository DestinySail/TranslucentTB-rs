use std::{panic::catch_unwind, sync::atomic::{AtomicU32, Ordering}, time::{Duration, Instant}};
use clearbar_common::{log, signal, wide};
use windows::{core::{w, Error, GUID, PCWSTR, Result}, Win32::{Foundation::*, System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*}}};

pub const WINDOW_CLASS: windows::core::PCWSTR = w!("Clearbar.Tray.0.1");
const CALLBACK: u32 = WM_APP + 1;
const EXIT: usize = 1;
const STARTUP: usize = 2;
const ICON_GUID: GUID = GUID::from_u128(0x62f739ae_96e9_4d13_bbb5_aee6aca83ba3);
const APP_ICON: usize = 1;
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub struct Tray { hwnd: HWND, icon: HICON, last_refresh: Instant }

impl Tray {
    pub fn new() -> Result<Self> {
        unsafe {
            let module = GetModuleHandleW(None)?;
            let icon = load_icon(module)?;
            let class = WNDCLASSW { hInstance: module.into(), lpszClassName: WINDOW_CLASS,
                lpfnWndProc: Some(window_proc), ..Default::default() };
            if RegisterClassW(&class) == 0 { let _ = DestroyIcon(icon); return Err(Error::from_win32()); }
            // A hidden top-level window receives TaskbarCreated broadcasts; a message-only window does not.
            let hwnd = match CreateWindowExW(WS_EX_TOOLWINDOW, WINDOW_CLASS, w!("Clearbar"),
                WS_POPUP, 0, 0, 0, 0, None, None, module, None) {
                Ok(value) => value,
                Err(error) => { let _ = DestroyIcon(icon); return Err(error); }
            };
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, icon.0 as isize);
            TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
            add_icon(hwnd, icon);
            Ok(Self { hwnd, icon, last_refresh: Instant::now() })
        }
    }

    pub fn wait(&mut self, stop: HANDLE) -> Result<bool> {
        unsafe {
            let result = MsgWaitForMultipleObjectsEx(Some(&[stop]), 500, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            if result == WAIT_FAILED { return Err(Error::from_win32()); }
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT { signal("stop", true); continue; }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            if self.last_refresh.elapsed() >= Duration::from_secs(2) {
                // Retry if Explorer was not ready when login startup created the window.
                if !Shell_NotifyIconW(NIM_MODIFY, &icon_data(self.hwnd, self.icon)).as_bool() {
                    add_icon(self.hwnd, self.icon);
                }
                self.last_refresh = Instant::now();
            }
            Ok(windows::Win32::System::Threading::WaitForSingleObject(stop, 0) == WAIT_OBJECT_0)
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(self.hwnd, self.icon));
            let _ = DestroyWindow(self.hwnd);
            let _ = DestroyIcon(self.icon);
        }
        log("app", "Tray icon removed");
    }
}

fn icon_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd, uID: 1, uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_GUID | NIF_SHOWTIP,
        uCallbackMessage: CALLBACK, hIcon: icon, guidItem: ICON_GUID, ..Default::default() };
    let tip = wide("Clearbar · 任务栏透明化（右键退出）");
    data.szTip[..tip.len()].copy_from_slice(&tip);
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    data
}

unsafe fn add_icon(hwnd: HWND, icon: HICON) {
    let data = icon_data(hwnd, icon);
    if Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
        log("app", "Tray icon added");
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // No Rust panic may unwind through the Windows callback boundary.
    let handled = catch_unwind(|| {
        if message != 0 && message == TASKBAR_CREATED.load(Ordering::Relaxed) {
            add_icon(hwnd, HICON(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut _));
            return true;
        }
        // Version 4 delivers WM_CONTEXTMENU. Also handling WM_RBUTTONUP can
        // open the menu twice for a single right click on newer Windows builds.
        if message == CALLBACK && ((lp.0 as u32) & 0xffff) == WM_CONTEXTMENU {
            if let Err(error) = show_menu(hwnd) { log("app", format!("Tray menu: {error}")); }
            return true;
        }
        if message == WM_COMMAND && wp.0 & 0xffff == STARTUP {
            let _ = EndMenu();
            toggle_startup(hwnd);
            return true;
        }
        if message == WM_CLOSE || (message == WM_COMMAND && wp.0 & 0xffff == EXIT) {
            let _ = EndMenu();
            log("app", "Exit requested from tray");
            signal("stop", true);
            return true;
        }
        false
    }).unwrap_or_else(|_| { log("app", "Caught panic in tray window"); false });
    if handled { LRESULT(0) } else { DefWindowProcW(hwnd, message, wp, lp) }
}

pub fn request_close() {
    unsafe {
        if let Ok(hwnd) = FindWindowW(WINDOW_CLASS, None) {
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn show_menu(hwnd: HWND) -> Result<()> {
    let menu = CreatePopupMenu()?;
    let result = (|| {
        // Re-read each time: Task Manager can disable startup while this app is running.
        let startup_state = crate::startup::is_enabled();
        let flags = match &startup_state {
            Ok(true) => MF_STRING | MF_CHECKED,
            Ok(false) => MF_STRING | MF_UNCHECKED,
            Err(error) => { log("app", format!("Read startup state: {error}")); MF_STRING | MF_GRAYED }
        };
        AppendMenuW(menu, flags, STARTUP, w!("开机启动"))?;
        AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
        AppendMenuW(menu, MF_STRING, EXIT, w!("退出"))?;
        let mut position = POINT::default();
        GetCursorPos(&mut position)?;
        let _ = SetForegroundWindow(hwnd);
        log("app", "Tray context menu opened");
        let selected = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            position.x, position.y, 0, hwnd, None);
        if selected.0 as usize == STARTUP {
            toggle_startup(hwnd);
        } else if selected.0 as usize == EXIT {
            log("app", "Exit selected from tray menu");
            signal("stop", true);
        }
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        Ok(())
    })();
    let _ = DestroyMenu(menu);
    result
}

unsafe fn toggle_startup(hwnd: HWND) {
    let result = crate::startup::is_enabled().and_then(|enabled| crate::startup::configure(!enabled));
    if let Err(error) = result {
        log("app", format!("Change startup: {error}"));
        let message = wide(&format!("无法更改开机启动设置，请稍后重试。\n\n{error}"));
        MessageBoxW(hwnd, windows::core::PCWSTR(message.as_ptr()), w!("Clearbar"), MB_OK | MB_ICONERROR);
    }
}

unsafe fn load_icon(instance: HMODULE) -> Result<HICON> {
    // Use the same multi-size ICO embedded in the executable, including at scaled tray sizes.
    let width = GetSystemMetrics(SM_CXSMICON);
    let height = GetSystemMetrics(SM_CYSMICON);
    let handle = LoadImageW(instance, PCWSTR(APP_ICON as *const u16), IMAGE_ICON,
        width, height, LR_DEFAULTCOLOR)?;
    Ok(HICON(handle.0))
}
