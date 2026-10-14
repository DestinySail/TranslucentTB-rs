use clearbar_common::{log, Handle};
use windows::{core::{s, w, Error, PCWSTR, Result}, Win32::{Foundation::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*}};

pub fn shell() -> Option<(HWND, u32, u32)> {
    unsafe {
        let window = FindWindowW(w!("Shell_TrayWnd"), None).ok()?;
        let mut pid = 0;
        let tid = GetWindowThreadProcessId(window, Some(&mut pid));
        (pid != 0 && tid != 0).then_some((window, pid, tid))
    }
}

pub struct Injection { hook: HHOOK, module: HMODULE }
impl Injection {
    pub fn attach(window: HWND, thread: u32, path: &std::path::Path) -> Result<Self> {
        unsafe {
            if !path.is_file() { return Err(Error::new(E_FAIL, "Embedded TAP DLL is unavailable")); }
            use std::os::windows::ffi::OsStrExt;
            let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let module = LoadLibraryExW(PCWSTR(name.as_ptr()), None, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32)?;
            let Some(proc) = GetProcAddress(module, s!("ClearbarHook")) else {
                let error = Error::from_win32(); let _ = FreeLibrary(module); return Err(error);
            };
            match SetWindowsHookExW(WH_CALLWNDPROC, Some(std::mem::transmute::<unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT>(proc)), module, thread) {
                Ok(hook) => {
                    // A posted message wakes the target thread without synchronously blocking login.
                    let _ = PostMessageW(window, WM_NULL, WPARAM(0), LPARAM(0));
                    log("app", "Explorer bootstrap hook installed");
                    Ok(Self { hook, module })
                }
                Err(error) => { let _ = FreeLibrary(module); Err(error) }
            }
        }
    }
}
impl Drop for Injection {
    fn drop(&mut self) {
        unsafe { let _ = UnhookWindowsHookEx(self.hook); let _ = FreeLibrary(self.module); }
    }
}

pub fn another_translucenttb_running() -> bool {
    // Do not compete with the original program over the same taskbar.
    let name = clearbar_common::wide("344635E9-9AE4-4E60-B128-D53E25AB70A7");
    unsafe {
        windows::Win32::System::Threading::OpenMutexW(
            windows::Win32::System::Threading::SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(name.as_ptr()))
            .map(Handle).is_ok()
    }
}
