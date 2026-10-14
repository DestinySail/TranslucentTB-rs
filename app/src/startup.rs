use std::{ffi::OsString, os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}};
use windows::{core::{w, Error, Interface, PCWSTR, Result}, Win32::{
    Foundation::*, System::Com::*, UI::Shell::*,
}};
use crate::startup_registry::Registry;

const APPROVAL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder";
const VALUE: &str = "Clearbar.lnk";
const LEGACY_RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

struct Apartment;
impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize(); } } }

fn io_error(error: std::io::Error) -> Error { Error::new(E_FAIL, error.to_string()) }
fn path_wide(path: &Path) -> Vec<u16> { path.as_os_str().encode_wide().chain(Some(0)).collect() }

fn shortcut_path() -> Result<PathBuf> {
    unsafe {
        // Respect Windows profiles with a redirected Startup folder.
        let raw = SHGetKnownFolderPath(&FOLDERID_Startup, KF_FLAG_DEFAULT, None)?;
        let path = PathBuf::from(OsString::from_wide(raw.as_wide()));
        CoTaskMemFree(Some(raw.0.cast()));
        Ok(path.join("Clearbar.lnk"))
    }
}

pub fn is_enabled() -> Result<bool> {
    if !shortcut_path()?.try_exists().map_err(io_error)? { return Ok(false); }
    // Windows owns this undocumented format. Missing means enabled; only known
    // enabled states are accepted so the menu reflects Task Manager changes.
    Ok(Registry::open()?.first_byte(APPROVAL_KEY, VALUE)?
        .map(|byte| matches!(byte, 0 | 2 | 6)).unwrap_or(true))
}

fn save_shortcut(path: &Path) -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _apartment = Apartment;
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let exe = std::env::current_exe().map_err(io_error)?;
        let exe_wide = path_wide(&exe);
        link.SetPath(PCWSTR(exe_wide.as_ptr()))?;
        link.SetArguments(w!("--startup"))?;
        link.SetDescription(w!("Clearbar · 任务栏透明化"))?;
        if let Some(parent) = exe.parent() {
            let parent = path_wide(parent);
            link.SetWorkingDirectory(PCWSTR(parent.as_ptr()))?;
        }
        let persist: IPersistFile = link.cast()?;
        let path = path_wide(path);
        persist.Save(PCWSTR(path.as_ptr()), true)?;
        Ok(())
    }
}

pub fn configure(enabled: bool) -> Result<()> {
    let registry = Registry::open()?;
    let path = shortcut_path()?;
    let previous = match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(io_error(error)),
    };
    let result: Result<()> = (|| {
        if enabled {
            save_shortcut(&path)?;
        } else if previous.is_some() {
            std::fs::remove_file(&path).map_err(io_error)?;
        }
        // Re-enabling clears only this shortcut's disabled state. Never manufacture
        // StartupApproved data or change other applications' startup settings.
        registry.delete(APPROVAL_KEY, VALUE)?;
        registry.delete(LEGACY_RUN, "Clearbar")?;
        if is_enabled()? != enabled { return Err(Error::new(E_FAIL, "Startup setting did not persist")); }
        Ok(())
    })();
    if result.is_err() {
        let rollback = match previous {
            Some(bytes) => std::fs::write(&path, bytes),
            None => match std::fs::remove_file(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                result => result,
            },
        };
        if let Err(error) = rollback { clearbar_common::log("app", format!("Startup rollback failed: {error}")); }
    }
    result?;
    clearbar_common::log("app", format!("Login startup enabled={enabled}; shortcut={}", path.display()));
    Ok(())
}

pub fn refresh_registration() -> Result<()> {
    // Refresh the EXE path on an enabled installation, preserving a Task Manager
    // disabled choice. First launch never silently enables startup.
    if is_enabled()? { save_shortcut(&shortcut_path()?)?; }
    Ok(())
}
