#![allow(non_snake_case)]
mod abi;
mod callback;
mod state;
mod site;
use abi::*;
use std::{ffi::c_void, panic::{catch_unwind, AssertUnwindSafe}, sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering}, time::Duration};
use clearbar_common::{controller_alive, log, TAP_CLSID, wide};
use windows::{core::{implement, Interface, IUnknown, Result, GUID, HRESULT, PCWSTR}, Win32::{Foundation::*, System::{Com::*, LibraryLoader::*}, UI::WindowsAndMessaging::*}};

static MODULE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static INITIALIZING: AtomicBool = AtomicBool::new(false);
pub(crate) static SNAPSHOT_READY: AtomicBool = AtomicBool::new(false);
pub(crate) static SCAN: AtomicU64 = AtomicU64::new(0);
pub(crate) static COMPLETED_SCAN: AtomicU64 = AtomicU64::new(0);

fn boundary(action: impl FnOnce() -> Result<()>) -> HRESULT {
    match catch_unwind(AssertUnwindSafe(action)) {
        Ok(Ok(())) => HRESULT(0),
        Ok(Err(e)) => { log("tap", format!("COM error: {e}")); e.code() }
        Err(_) => { log("tap", "Caught Rust panic at COM boundary"); E_FAIL }
    }
}

fn subscribe(site: &IUnknown) -> Result<()> {
    let service: IVisualTreeService3 = site.cast()?;
    let diagnostics: IXamlDiagnostics = site.cast()?;
    // Advise dispatches callbacks to each XAML UI thread. Subscribing off-thread
    // follows the original implementation and avoids blocking IObjectWithSite.
    let raw = service.into_raw() as usize;
    let diagnostics_raw = diagnostics.into_raw() as usize;
    std::thread::spawn(move || unsafe {
        let service = IVisualTreeService3::from_raw(raw as *mut c_void);
        let diagnostics = IXamlDiagnostics::from_raw(diagnostics_raw as *mut c_void);
        let callback = callback::create(diagnostics);
        loop {
            while !controller_alive() { std::thread::sleep(Duration::from_secs(1)); }
            let generation = SCAN.fetch_add(1, Ordering::AcqRel) + 1;
            let hr = service.AdviseVisualTreeChange(callback.as_raw());
            SNAPSHOT_READY.store(true, Ordering::Release);
            log("tap", format!("Visual tree scan={generation} HRESULT=0x{:08X}", hr.0 as u32));
            if hr.is_ok() {
                COMPLETED_SCAN.store(generation, Ordering::Release);
                // One bounded subscription lasts as long as Explorer and the pinned DLL.
                let _ = callback.into_raw();
                break;
            }
            // On this Windows build the SDK delivered the taskbar nodes but the
            // subscription returned E_UNEXPECTED. Its cause is not established.
            // Keep the verified
            // snapshot instead of terminating transparency. Unsubscribe before a
            // later rescan so new/recreated taskbars are still discovered.
            let _ = service.UnadviseVisualTreeChange(callback.as_raw());
            log("tap", "Subscription unavailable; retaining captured backgrounds and rescanning in 30s");
            std::thread::sleep(Duration::from_secs(30));
        }
    });
    Ok(())
}
#[implement(IClassFactory)]
struct Factory;
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Option<&IUnknown>, iid: *const GUID, out: *mut *mut c_void) -> Result<()> {
        boundary(|| unsafe {
            if out.is_null() || iid.is_null() { return Err(E_POINTER.into()); }
            *out = std::ptr::null_mut();
            if outer.is_some() { return Err(CLASS_E_NOAGGREGATION.into()); }
            let site = site::create();
            site.query(iid, out).ok()
        }).ok()
    }
    fn LockServer(&self, _: BOOL) -> Result<()> { Ok(()) }
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(clsid: *const GUID, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    boundary(|| {
        if clsid.is_null() || iid.is_null() || out.is_null() { return Err(E_POINTER.into()); }
        *out = std::ptr::null_mut();
        if *clsid != TAP_CLSID { return Err(CLASS_E_CLASSNOTAVAILABLE.into()); }
        let factory: IClassFactory = Factory.into();
        factory.query(iid, out).ok()
    })
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT { S_FALSE }

#[no_mangle]
pub unsafe extern "system" fn DllMain(module: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    // Loader-lock safe: no allocation, threads, COM, synchronization or logging here.
    if reason == 1 { MODULE.store(module.0, Ordering::Relaxed); }
    BOOL(1)
}

#[no_mangle]
pub unsafe extern "system" fn ClearbarHook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let _ = catch_unwind(|| {
        if code >= 0 && !INITIALIZING.swap(true, Ordering::SeqCst) {
            let mut pinned = HMODULE::default();
            // A Rust worker may outlive the temporary hook. Pin before creating it.
            if GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
                PCWSTR(ClearbarHook as *const u16), &mut pinned).is_ok() {
                std::thread::spawn(|| {
                    if let Err(e) = initialize() {
                        log("tap", format!("Initialization exhausted: {e}"));
                        INITIALIZING.store(false, Ordering::SeqCst);
                    }
                });
            } else { INITIALIZING.store(false, Ordering::SeqCst); }
        }
    });
    CallNextHookEx(None, code, wp, lp)
}

fn initialize() -> Result<()> {
    unsafe {
        let mut path = vec![0u16; 32768];
        let len = GetModuleFileNameW(HMODULE(MODULE.load(Ordering::Relaxed)), &mut path);
        if len == 0 || len as usize >= path.len() { return Err(windows::core::Error::from_win32()); }
        path.truncate(len as usize); path.push(0);
        let xaml = LoadLibraryExW(windows::core::w!("Windows.UI.Xaml.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32)?;
        let proc = GetProcAddress(xaml, windows::core::s!("InitializeXamlDiagnosticsEx"))
            .ok_or_else(windows::core::Error::from_win32)?;
        type Init = unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, GUID, PCWSTR) -> HRESULT;
        let init: Init = std::mem::transmute(proc);
        let mut last = E_FAIL;
        for attempt in 0..60 {
            if !controller_alive() { return Err(E_ABORT.into()); }
            let dll_path = path.clone();
            // Diagnostics initialization is once per thread: every retry needs a fresh thread.
            let hr = std::thread::spawn(move || {
                // These are the XAML runtime's existing endpoint names, not arbitrary client IDs.
                let connection = wide(&format!("VisualDiagConnection{}", attempt + 1));
                init(PCWSTR(connection.as_ptr()), std::process::id(), PCWSTR::null(),
                    PCWSTR(dll_path.as_ptr()), TAP_CLSID, PCWSTR::null())
            }).join().unwrap_or(E_FAIL);
            log("tap", format!("XAML initialization attempt={} HRESULT=0x{:08X}", attempt + 1, hr.0 as u32));
            if hr.is_ok() { return Ok(()); }
            last = hr;
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(last.into())
    }
}
