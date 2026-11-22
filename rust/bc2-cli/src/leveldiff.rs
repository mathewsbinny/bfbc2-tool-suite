//! `bfbc2 leveldiff`: LevelDiff's command line (round EX1; `LevelDiff.exe`, a .NET Framework program, retired), run
//! in process by [`bc2_tools::leveldiff`]. The same arguments, text, JSON file and exit codes (0 no
//! differences, 1 differences, 2 bad input, 3 an unexpected error), proven against LevelDiff.exe's recorded answers
//! on real level pairs (`rust/BENCHMARKS.md`, "EX1").
//!
//! Printing is .NET's `Console`, not Python's: each line ends `\r\n`; off a console the text is in the console's
//! output code page (the OEM code page; the ANSI one without a console), with Windows' best-fit mapping for
//! characters it lacks, as `Console.Out` encodes; on a console it is written as Unicode. The log is LevelDiff's
//! `ToolLog` file, `%LOCALAPPDATA%\bfbc2-tool-suite\logs\leveldiff-<date>.log`.

use std::process::ExitCode;

use bc2_tools::leveldiff;

use crate::{pylog, pystdio};

/// The console's output code page as .NET's `Console.OutputEncoding` reads it (`GetConsoleOutputCP`, the ANSI code
/// page when the process has no console).
#[cfg(windows)]
#[allow(unsafe_code)]
fn code_page() -> u32 {
    // SAFETY: both take no arguments and only return a number.
    let cp = unsafe { windows_sys::Win32::System::Console::GetConsoleOutputCP() };
    if cp == 0 {
        // SAFETY: as above.
        unsafe { windows_sys::Win32::Globalization::GetACP() }
    } else {
        cp
    }
}

/// `text` in code page `cp` as Windows converts it (best fit, `?` for what has none).
#[cfg(windows)]
#[allow(unsafe_code)]
fn encode(text: &str, cp: u32) -> Vec<u8> {
    if text.is_ascii() || cp == 65001 {
        return text.as_bytes().to_vec();
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    let Ok(len) = i32::try_from(wide.len()) else {
        return text.as_bytes().to_vec();
    };
    // SAFETY: the input is `wide` with its length; a null output asks for the size needed.
    let need = unsafe {
        windows_sys::Win32::Globalization::WideCharToMultiByte(
            cp,
            0,
            wide.as_ptr(),
            len,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
        )
    };
    let Ok(size) = usize::try_from(need) else {
        return text.as_bytes().to_vec();
    };
    let mut out = vec![0u8; size];
    // SAFETY: `out` holds `need` bytes, as the call above asked for.
    let wrote = unsafe {
        windows_sys::Win32::Globalization::WideCharToMultiByte(
            cp,
            0,
            wide.as_ptr(),
            len,
            out.as_mut_ptr(),
            need,
            std::ptr::null(),
            std::ptr::null_mut(),
        )
    };
    out.truncate(usize::try_from(wrote).unwrap_or(0));
    out
}

#[cfg(not(windows))]
fn code_page() -> u32 {
    65001
}

#[cfg(not(windows))]
fn encode(text: &str, _cp: u32) -> Vec<u8> {
    text.as_bytes().to_vec()
}

fn write(console: bool, text: &str, h: &mut dyn std::io::Write) {
    if text.is_empty() {
        return;
    }
    let crlf = text.replace('\n', "\r\n");
    let bytes = if console {
        crlf.into_bytes()
    } else {
        encode(&crlf, code_page())
    };
    let _ = h.write_all(&bytes);
    let _ = h.flush();
}

/// Runs `bfbc2 leveldiff <args>`.
pub fn main(args: &[String]) -> ExitCode {
    let log = pylog::log_path("leveldiff");
    let r = leveldiff::run(args, log.as_deref());
    pylog::append(log.as_deref(), "leveldiff", args, &r.log);
    let (out, err) = (std::io::stdout(), std::io::stderr());
    let (oc, ec) = (pystdio::is_console(&out), pystdio::is_console(&err));
    write(oc, &r.stdout, &mut out.lock());
    write(ec, &r.stderr, &mut err.lock());
    ExitCode::from(u8::try_from(r.code & 0xFF).unwrap_or(1))
}
