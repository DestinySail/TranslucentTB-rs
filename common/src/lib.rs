use std::{fs::OpenOptions, io::Write, time::{SystemTime, UNIX_EPOCH}};
use windows::{core::{PCWSTR, Result}, Win32::{Foundation::*, System::Threading::*}};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;

pub const TAP_CLSID: windows::core::GUID = windows::core::GUID::from_u128(0xfca1da54_693f_4dbd_891a_680472b07b76);
pub const RETRY_SECONDS: [u64; 6] = [1, 2, 4, 8, 15, 30];

pub fn wide(value: &str) -> Vec<u16> { value.encode_utf16().chain(Some(0)).collect() }

pub fn object_name(role: &str) -> Vec<u16> {
    let mut session = 0;
    unsafe { let _ = ProcessIdToSessionId(GetCurrentProcessId(), &mut session); }
    wide(&format!("Local\\Clearbar.0.1.0.r1.{session}.{role}"))
}

pub struct Handle(pub HANDLE);
impl Drop for Handle {
    fn drop(&mut self) { unsafe { let _ = CloseHandle(self.0); } }
}

pub fn event(role: &str) -> Result<Handle> {
    let name = object_name(role);
    unsafe { CreateEventW(None, true, false, PCWSTR(name.as_ptr())).map(Handle) }
}

pub fn is_signaled(role: &str) -> bool {
    let name = object_name(role);
    unsafe {
        OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(name.as_ptr()))
            .map(Handle).map(|h| WaitForSingleObject(h.0, 0) == WAIT_OBJECT_0).unwrap_or(false)
    }
}

pub fn signal(role: &str, enabled: bool) {
    let name = object_name(role);
    unsafe {
        if let Ok(handle) = OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())).map(Handle) {
            if enabled { let _ = SetEvent(handle.0); } else { let _ = ResetEvent(handle.0); }
        }
    }
}

pub fn controller_alive() -> bool {
    let name = object_name("controller");
    unsafe {
        // Open briefly: keeping a DLL-side handle would keep the object alive after a crash.
        OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(name.as_ptr())).map(Handle).is_ok()
    }
}

pub fn log(component: &str, message: impl AsRef<str>) {
    let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir).join("Clearbar");
    let _ = std::fs::create_dir_all(&base);
    let path = base.join(format!("{component}.log"));
    // Bound growth; logs contain diagnostics only, never document/window titles.
    if std::fs::metadata(&path).map(|m| m.len() > 1024 * 1024).unwrap_or(false) {
        let _ = std::fs::rename(&path, base.join(format!("{component}.previous.log")));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let line = format!("{timestamp} pid={} {}\n", unsafe { GetCurrentProcessId() }, message.as_ref());
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_event_roundtrip_and_cleanup() {
        let role = format!("test-event-{}", std::process::id());
        let handle = event(&role).unwrap();
        assert!(!is_signaled(&role));
        signal(&role, true);
        assert!(is_signaled(&role));
        signal(&role, false);
        assert!(!is_signaled(&role));
        drop(handle);
        assert!(!is_signaled(&role));
    }
    #[test]
    fn names_are_local_and_protocol_versioned() {
        let name = object_name("applied");
        assert_eq!(name.last(), Some(&0));
        assert!(String::from_utf16_lossy(&name).starts_with("Local\\Clearbar.0.1.0.r1."));
    }
}
