use std::{fs::{self, OpenOptions}, io::Write, path::PathBuf};
use windows::{core::{Error, Result}, Win32::Foundation::E_FAIL};

const TAP: &[u8] = include_bytes!(env!("CLEARBAR_EMBEDDED_TAP"));

pub fn extract() -> Result<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| Error::new(E_FAIL, "LOCALAPPDATA is unavailable"))?;
    // A content-specific directory avoids replacing a DLL pinned in Explorer.
    // The tag is only a cache key; compare all bytes before trusting a cached DLL.
    let tag = TAP.iter().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ *byte as u64).wrapping_mul(0x100000001b3));
    let directory = PathBuf::from(root).join("Clearbar/runtime").join(format!("{tag:016x}"));
    fs::create_dir_all(&directory).map_err(error)?;
    let destination = directory.join("clearbar_tap.dll");
    if fs::read(&destination).map(|data| data == TAP).unwrap_or(false) { return Ok(destination); }
    let temporary = directory.join(format!("clearbar_tap.{}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new().create_new(true).write(true).open(&temporary)?;
        file.write_all(TAP)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &destination)
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result.map_err(error)?;
    clearbar_common::log("app", format!("Embedded TAP extracted: {}", destination.display()));
    Ok(destination)
}

fn error(error: std::io::Error) -> Error { Error::new(E_FAIL, format!("Cannot prepare embedded TAP: {error}")) }
