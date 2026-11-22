//! Whether a process runs, and this machine's name: what a lock file's holder is checked by (the install ledger's
//! lock, the map project's lock, the installer's). The crate's only calls into the system (Win32), kept here.

/// This machine's DNS host name (`None` when the system does not give it).
#[must_use]
pub fn host_name() -> Option<String> {
    #[cfg(windows)]
    {
        win::hostname()
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOSTNAME").ok()
    }
}

/// Whether a process with this id runs on this machine (this process always does; ids outside `u32` and 0 never).
#[must_use]
pub fn pid_alive(pid: i64) -> bool {
    if pid == i64::from(std::process::id()) {
        return true;
    }
    let Ok(pid) = u32::try_from(pid) else {
        return false;
    };
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        win::pid_alive(pid)
    }
    #[cfg(not(windows))]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 calls: a buffer passed with its length; a process handle checked and closed"
)]
mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::SystemInformation::{ComputerNameDnsHostname, GetComputerNameExW};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    pub fn hostname() -> Option<String> {
        let mut buf = [0u16; 256];
        let mut len: u32 = 256;
        // SAFETY: the buffer and its length are passed together; the call writes at most `len` u16s.
        let ok = unsafe { GetComputerNameExW(ComputerNameDnsHostname, buf.as_mut_ptr(), &raw mut len) } != 0;
        ok.then(|| String::from_utf16_lossy(buf.get(..len as usize).unwrap_or_default()))
    }

    pub fn pid_alive(pid: u32) -> bool {
        // SAFETY: OpenProcess returns a handle or null; a non-null handle is used and closed once.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return GetLastError() == 5; // access denied: it exists (another user's)
            }
            let mut code: u32 = 0;
            let ok = GetExitCodeProcess(h, &raw mut code) != 0;
            CloseHandle(h);
            ok && code == 259 // STILL_ACTIVE
        }
    }
}

/// `game_running()`: `tasklist` names BFBC2Game.exe (NavKit's install and restore, the mod manager). A `tasklist`
/// that does not answer in 30 s (the service host busy, as during the game's crash dump) counts as the game running:
/// a refusal, never a hang (AU4-BUG-20).
#[must_use]
pub fn game_running() -> bool {
    let mut c = std::process::Command::new("tasklist");
    c.args(["/FI", "IMAGENAME eq BFBC2Game.exe", "/NH"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        c.creation_flags(0x0800_0000);
    }
    let Ok(mut child) = c.spawn() else {
        return false;
    };
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < std::time::Duration::from_secs(30) => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return true;
            }
        }
    }
    child
        .wait_with_output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("BFBC2Game.exe"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_runs() {
        assert!(pid_alive(i64::from(std::process::id())));
        assert!(!pid_alive(0) && !pid_alive(-5));
    }
}
