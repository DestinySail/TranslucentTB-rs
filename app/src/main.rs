#![windows_subsystem = "windows"]
mod backend;
mod startup;
mod startup_registry;
mod runtime;
mod tray;
use std::time::{Duration, Instant};
use clearbar_common::*;
use windows::{core::{Error, PCWSTR, Result}, Win32::{Foundation::*, System::Threading::*}};

fn main() {
    std::panic::set_hook(Box::new(|info| log("app", format!("Panic: {info}"))));
    if let Err(error) = run() {
        log("app", format!("Fatal: {error}"));
        eprintln!("Clearbar: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--enable-startup") => { startup::configure(true)?; log("app", "Login startup enabled"); return Ok(()); }
        Some("--disable-startup") => { startup::configure(false)?; log("app", "Login startup disabled"); return Ok(()); }
        Some("--stop") => {
            signal("stop", true);
            tray::request_close();
            for _ in 0..20 {
                if !controller_alive() { break; }
                std::thread::sleep(Duration::from_millis(250));
            }
            if controller_alive() { return Err(Error::new(E_FAIL, "Controller did not stop within 5 seconds")); }
            return Ok(());
        }
        Some("--status") => {
            let text = format!("Clearbar {}\nrunning={}\ntransparent={}\nstartup={}\nshell={:?}\n",
                env!("CARGO_PKG_VERSION"), controller_alive(), is_signaled("applied"), startup::is_enabled()?, backend::shell().map(|(_, p, _)| p));
            print!("{text}");
            let dir = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir).join("Clearbar");
            std::fs::create_dir_all(&dir).map_err(io_error)?;
            std::fs::write(dir.join("status.txt"), text).map_err(io_error)?;
            return Ok(());
        }
        None | Some("--startup") => {}
        _ => return Err(Error::new(E_INVALIDARG, "Options: --startup --enable-startup --disable-startup --stop --status")),
    }
    if backend::another_translucenttb_running() {
        return Err(Error::new(E_FAIL, "Exit the original TranslucentTB before starting Clearbar"));
    }
    unsafe {
        let name = object_name("controller");
        let mutex = CreateMutexW(None, false, PCWSTR(name.as_ptr()))?;
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        let _instance = Handle(mutex);
        if exists { log("app", "Already running; second launch ignored"); return Ok(()); }
        if let Err(error) = startup::refresh_registration() { log("app", format!("Refresh startup path: {error}")); }
        let stop = event("stop")?;
        let applied = event("applied")?;
        let restored = event("restored")?;
        ResetEvent(stop.0)?; ResetEvent(applied.0)?; ResetEvent(restored.0)?;
        let dll_path = runtime::extract()?;
        let mut tray = tray::Tray::new()?;
        log("app", "Started; waiting for Explorer/XAML readiness");
        let mut explorer_pid = 0;
        let mut injection = None;
        let mut attempt = 0usize;
        let mut next_attempt = Instant::now();
        let mut was_applied = false;
        loop {
            if tray.wait(stop.0)? { break; }
            // Check again after login in case Windows starts the original tool slightly later.
            if backend::another_translucenttb_running() {
                log("app", "Original TranslucentTB detected; stopping to avoid conflicts"); break;
            }
            let shell = backend::shell();
            let current_pid = shell.map(|(_, p, _)| p).unwrap_or(0);
            if current_pid != explorer_pid {
                injection = None; explorer_pid = current_pid; attempt = 0;
                next_attempt = Instant::now(); was_applied = false;
                ResetEvent(applied.0)?;
                log("app", format!("Taskbar process changed: pid={explorer_pid}"));
            }
            if WaitForSingleObject(applied.0, 0) == WAIT_OBJECT_0 {
                if !was_applied { log("app", "Transparency applied by Explorer UI thread"); }
                was_applied = true;
                injection = None;
                continue;
            }
            was_applied = false;
            if Instant::now() < next_attempt { continue; }
            injection = None;
            if let Some((window, _, tid)) = shell {
                match backend::Injection::attach(window, tid, &dll_path) {
                    Ok(value) => injection = Some(value),
                    Err(e) => log("app", format!("Bootstrap attempt {} failed: {e}", attempt + 1)),
                }
            } else if attempt == 0 { log("app", "Taskbar not ready; retrying in background"); }
            let delay = RETRY_SECONDS[attempt.min(RETRY_SECONDS.len() - 1)];
            next_attempt = Instant::now() + Duration::from_secs(delay);
            attempt = attempt.saturating_add(1);
        }
        // Tell all Explorer UI-thread timers to restore before releasing controller ownership.
        SetEvent(stop.0)?;
        let result = WaitForSingleObject(restored.0, 3000);
        log("app", format!("Stopped; restoration acknowledged={}", result == WAIT_OBJECT_0));
        drop(injection);
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> Error { Error::new(E_FAIL, error.to_string()) }
