//! Printing as the Python command lines print: `sys.stdout.write(out)` then `sys.stderr.write(err)` at the end of
//! `main`, with the stdio Python 3 makes on Windows.
//!
//! - **A console** (a handle `GetConsoleMode` accepts) is written in Unicode, `\n` as `\r\n`.
//! - **Anything else** (a pipe, a file) gets the text with `\n` as `\r\n`, encoded in the ANSI code page
//!   (`PYTHONUTF8=1` or `PYTHONIOENCODING=utf-8`: UTF-8). stdout is strict: a character the code page lacks makes
//!   Python raise `UnicodeEncodeError` before anything is printed (exit code 1, the error on stderr); stderr writes
//!   it as `\xNN` / `\uNNNN` (`backslashreplace`).
//! - **The order** two streams sharing one pipe see: stderr is line-buffered (each write ends a line, so it goes out
//!   at once); stdout off a console is block-buffered, so a text up to 8,192 bytes leaves only at exit, after
//!   stderr, and a longer one at once, before it (`PYTHONUNBUFFERED`: at once).
//!
//! Code pages: 1252 (Python's own `cp1252` table, which leaves five bytes undefined) and 65001; under any other ANSI
//! code page non-ASCII text is written as UTF-8 (`rust/BENCHMARKS.md` "O1": a recorded difference).

use std::io::{self, Write as _};

/// The encoding Python picks for a stream that is not a console.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Codec {
    Utf8,
    Cp1252,
    /// Another ANSI code page: written as UTF-8.
    Other,
}

/// Python's cp1252 for U+0080..U+00FF and beyond: bytes 0x80..0x9F (None: undefined), the rest Latin-1.
const CP1252_80: [Option<u16>; 32] = [
    Some(0x20AC),
    None,
    Some(0x201A),
    Some(0x0192),
    Some(0x201E),
    Some(0x2026),
    Some(0x2020),
    Some(0x2021),
    Some(0x02C6),
    Some(0x2030),
    Some(0x0160),
    Some(0x2039),
    Some(0x0152),
    None,
    Some(0x017D),
    None,
    None,
    Some(0x2018),
    Some(0x2019),
    Some(0x201C),
    Some(0x201D),
    Some(0x2022),
    Some(0x2013),
    Some(0x2014),
    Some(0x02DC),
    Some(0x2122),
    Some(0x0161),
    Some(0x203A),
    Some(0x0153),
    None,
    Some(0x017E),
    Some(0x0178),
];

pub(crate) fn cp1252(c: char) -> Option<u8> {
    let u = c as u32;
    if u < 0x80 || (0xA0..=0xFF).contains(&u) {
        return u8::try_from(u).ok();
    }
    CP1252_80
        .iter()
        .position(|m| m.is_some_and(|v| u32::from(v) == u))
        .and_then(|i| u8::try_from(0x80 + i).ok())
}

/// `text` as Python's `open(path, 'w')` writes it (the ANSI code page, strict): UTF-8 bytes on a UTF-8 page, cp1252's
/// on 1252; Err the `UnicodeEncodeError` message for a character the page lacks (another page: ASCII text only).
#[cfg(feature = "bf1943")]
pub(crate) fn encode_ansi(text: &str) -> Result<Vec<u8>, String> {
    match ansi_code_page() {
        65001 => Ok(text.as_bytes().to_vec()),
        1252 => encode(text, Codec::Cp1252, false).map_err(|u| u.message()),
        _ if text.is_ascii() => Ok(text.as_bytes().to_vec()),
        _ => Err("'charmap' codec can't encode the text: the ANSI code page is not one bfbc2.exe writes".to_owned()),
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn ansi_code_page() -> u32 {
    // SAFETY: GetACP takes no arguments and only returns a number.
    unsafe { windows_sys::Win32::Globalization::GetACP() }
}

#[cfg(not(windows))]
fn ansi_code_page() -> u32 {
    65001
}

pub(crate) fn codec() -> Codec {
    if std::env::var("PYTHONUTF8").is_ok_and(|v| v == "1") {
        return Codec::Utf8;
    }
    if let Ok(v) = std::env::var("PYTHONIOENCODING") {
        let name = v
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
            .replace('_', "-");
        match name.as_str() {
            "" => {}
            "utf-8" | "utf8" | "u8" | "cp65001" => return Codec::Utf8,
            "cp1252" | "windows-1252" => return Codec::Cp1252,
            _ => return Codec::Other,
        }
    }
    match ansi_code_page() {
        65001 => Codec::Utf8,
        1252 => Codec::Cp1252,
        _ => Codec::Other,
    }
}

/// A character run stdout's strict encoder refused: Python's `UnicodeEncodeError` (char positions, end exclusive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unencodable {
    start: usize,
    end: usize,
    first: char,
}

impl Unencodable {
    /// `str(UnicodeEncodeError)` as the cp1252 codec raises it.
    pub(crate) fn message(&self) -> String {
        let reason = "character maps to <undefined>";
        if self.end == self.start + 1 {
            let u = self.first as u32;
            let shown = if u <= 0xFF {
                format!("\\x{u:02x}")
            } else if u <= 0xFFFF {
                format!("\\u{u:04x}")
            } else {
                format!("\\U{u:08x}")
            };
            format!(
                "'charmap' codec can't encode character '{shown}' in position {}: {reason}",
                self.start
            )
        } else {
            format!(
                "'charmap' codec can't encode characters in position {}-{}: {reason}",
                self.start,
                self.end - 1
            )
        }
    }
}

fn backslash(c: char, out: &mut Vec<u8>) {
    let u = c as u32;
    let s = if u <= 0xFF {
        format!("\\x{u:02x}")
    } else if u <= 0xFFFF {
        format!("\\u{u:04x}")
    } else {
        format!("\\U{u:08x}")
    };
    out.extend_from_slice(s.as_bytes());
}

/// `text` (already with `\r\n`) in `codec`; strict (`replace` false) refuses the first run of characters cp1252
/// lacks, as Python's charmap encoder reports it.
pub(crate) fn encode(text: &str, codec: Codec, replace: bool) -> Result<Vec<u8>, Unencodable> {
    if codec != Codec::Cp1252 || text.is_ascii() {
        return Ok(text.as_bytes().to_vec());
    }
    let mut out = Vec::with_capacity(text.len());
    let mut run: Option<Unencodable> = None;
    for (i, c) in text.chars().enumerate() {
        match cp1252(c) {
            Some(b) => {
                if let Some(r) = run.take() {
                    return Err(r);
                }
                out.push(b);
            }
            None if replace => backslash(c, &mut out),
            None => match run.as_mut() {
                Some(r) => r.end = i + 1,
                None => {
                    run = Some(Unencodable {
                        start: i,
                        end: i + 1,
                        first: c,
                    });
                }
            },
        }
    }
    run.map_or(Ok(out), Err)
}

#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn is_console<T: std::os::windows::io::AsRawHandle>(h: &T) -> bool {
    let mut mode = 0u32;
    // SAFETY: GetConsoleMode reads the handle (a process's standard handle, open for the process's life) and writes
    // one u32 it is given; a handle that is not a console makes it fail and return 0.
    unsafe { windows_sys::Win32::System::Console::GetConsoleMode(h.as_raw_handle(), &raw mut mode) != 0 }
}

#[cfg(not(windows))]
pub(crate) fn is_console<T: std::io::IsTerminal>(h: &T) -> bool {
    h.is_terminal()
}

/// Python's buffer: a write longer than this goes out at once.
const CHUNK: usize = 8192;

/// Prints `out` and `err` as the Python command line's last two writes would; `None`, or `Some(1)` when Python's
/// stdout would have raised `UnicodeEncodeError` (then stdout gets nothing and stderr the error).
pub fn emit_or_fail(out: &str, err: &str) -> Option<i32> {
    let stdout = io::stdout();
    let stderr = io::stderr();
    let out_console = is_console(&stdout);
    let err_console = is_console(&stderr);
    let codec = codec();
    let out_text = out.replace('\n', "\r\n");
    let err_text = err.replace('\n', "\r\n");
    let out_bytes = if out.is_empty() || out_console {
        out_text.into_bytes()
    } else {
        match encode(&out_text, codec, false) {
            Ok(b) => b,
            Err(e) => {
                let msg = format!(
                    "Traceback (most recent call last):\nUnicodeEncodeError: {}\n",
                    e.message()
                );
                let bytes = msg.replace('\n', "\r\n").into_bytes();
                let mut h = stderr.lock();
                let _ = h.write_all(&bytes);
                let _ = h.flush();
                return Some(1);
            }
        }
    };
    let err_bytes = if err_console {
        err_text.into_bytes()
    } else {
        encode(&err_text, codec, true).unwrap_or_default()
    };
    let unbuffered = std::env::var("PYTHONUNBUFFERED").is_ok_and(|v| !v.is_empty());
    let out_first = out_console || unbuffered || out_bytes.len() > CHUNK;
    let write_out = || {
        if !out_bytes.is_empty() {
            let mut h = stdout.lock();
            let _ = h.write_all(&out_bytes);
            let _ = h.flush();
        }
    };
    let write_err = || {
        if !err_bytes.is_empty() {
            let mut h = stderr.lock();
            let _ = h.write_all(&err_bytes);
            let _ = h.flush();
        }
    };
    if out_first {
        write_out();
        write_err();
    } else {
        write_err();
        write_out();
    }
    None
}

/// [`emit_or_fail`] for bfbc2's own texts (ASCII).
pub fn emit(out: &str, err: &str) {
    let _ = emit_or_fail(out, err);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp1252_as_python() {
        assert_eq!(
            encode("a\u{20ac}\u{e9}", Codec::Cp1252, false),
            Ok(vec![b'a', 0x80, 0xE9])
        );
        assert_eq!(
            encode("ab\u{416}\u{417}c\u{418}", Codec::Cp1252, false).map_err(|e| e.message()),
            Err("'charmap' codec can't encode characters in position 2-3: character maps to <undefined>".to_owned())
        );
        assert_eq!(
            encode("\u{81}", Codec::Cp1252, false).map_err(|e| e.message()),
            Err(
                "'charmap' codec can't encode character '\\x81' in position 0: character maps to <undefined>"
                    .to_owned()
            )
        );
        assert_eq!(
            encode("\u{416}\u{1F600}", Codec::Cp1252, true),
            Ok(b"\\u0416\\U0001f600".to_vec())
        );
        assert_eq!(encode("\u{416}", Codec::Utf8, false), Ok("\u{416}".as_bytes().to_vec()));
    }
}
