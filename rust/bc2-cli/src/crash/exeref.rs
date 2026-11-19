//! `bfbc2 exeref str|refs|dis ...`: `crash-reader\exeref.py`'s command line over `bc2_harness::exe` (round E4f).
//! Where the core refuses (an instruction outside the capstone parity proof, an input the reference reads its own
//! way: capstone decides there) the command line goes to the Python.

use std::fmt::Write as _;

use bc2_harness::exe::{self, Exe, Syms};
use bc2_harness::pe::Image;

use super::common::{find_exe, here, hex_alt, hex_width, isfile, join, py_int, read_symbols};
use crate::pyfmt::docstring;
use crate::pyproc::{Fail, Py, PyErr, PyResult};

const SOURCE: &str = include_str!("../../../../bfbc2-automated-map-and-bot-test-harness/crash-reader/exeref.py");

/// `take`'s answer: the flag missing, present without a value, or its value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Taken {
    No,
    Yes,
    Val(String),
}

/// `exeref.take(args, flag, has_value)`.
fn take(args: &mut Vec<String>, flag: &str, has_value: bool) -> Taken {
    let Some(i) = args.iter().position(|a| a == flag) else {
        return Taken::No;
    };
    if !has_value || args.get(i + 1).is_none_or(|a| a.starts_with("--")) {
        args.remove(i);
        return Taken::Yes;
    }
    let v = args.remove(i + 1);
    args.remove(i);
    Taken::Val(v)
}

fn err(msg: impl Into<String>) -> Fail {
    PyErr::new("ExeRefError", msg).into()
}

/// `exeref.parse_int(s)`.
fn parse_int(t: &Taken, default: &str) -> PyResult<i128> {
    let s = match t {
        Taken::No => default,
        Taken::Yes => return Err(Fail::Python), // parse_int(True): an AttributeError of the Python's
        Taken::Val(v) => v.as_str(),
    };
    let lower = s.to_lowercase();
    let hex6 = (6..=8).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_hexdigit());
    let got = if lower.starts_with("0x") || hex6 {
        py_int(s, 16)
    } else {
        py_int(s, 0)
    };
    match got {
        Err(Fail::Err(_)) => Err(err(format!("\"{s}\" is not an address (hex, e.g. 0x0143375C)"))),
        other => other,
    }
}

/// `int(v)` of an option (`int(True)` is 1).
fn int_opt(t: &Taken, default: i128) -> PyResult<i128> {
    match t {
        Taken::No => Ok(default),
        Taken::Yes => Ok(1),
        Taken::Val(v) => py_int(v, 10),
    }
}

fn i64_of(v: i128) -> PyResult<i64> {
    i64::try_from(v).map_err(|_| Fail::Python)
}

fn core<T>(r: Result<T, exe::Fallback>) -> PyResult<T> {
    r.map_err(|_| Fail::Python)
}

/// `func_name(exe, va, syms)`.
fn func_name(x: &Exe, va: i64, syms: &Syms) -> PyResult<String> {
    let (n, _) = core(exe::name_of(va, syms, Some(x)))?;
    Ok(if n.is_empty() {
        "function start unknown".to_owned()
    } else {
        n
    })
}

/// The Latin-1 bytes of the text, or the `UnicodeEncodeError` Python's latin-1 codec raises.
fn latin1(text: &str) -> PyResult<Vec<u8>> {
    let chars: Vec<char> = text.chars().collect();
    if let Some(start) = chars.iter().position(|&c| u32::from(c) > 0xFF) {
        let end = chars
            .iter()
            .skip(start)
            .position(|&c| u32::from(c) <= 0xFF)
            .map_or(chars.len(), |k| start + k);
        let msg = if end == start + 1 {
            let u = chars.get(start).map_or(0, |&c| u32::from(c));
            let shown = if u <= 0xFFFF {
                format!("\\u{u:04x}")
            } else {
                format!("\\U{u:08x}")
            };
            format!("'latin-1' codec can't encode character '{shown}' in position {start}: ordinal not in range(256)")
        } else {
            format!(
                "'latin-1' codec can't encode characters in position {start}-{}: ordinal not in range(256)",
                end - 1
            )
        };
        return Err(PyErr::new("UnicodeEncodeError", msg).into());
    }
    Ok(chars
        .iter()
        .map(|&c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect())
}

/// `show_refs(value)`: the data that holds it and the instructions that use it (each with its window).
fn show_refs(py: &mut Py, x: &Exe, value: i128, mx: i128, dis_n: i128, syms: &Syms) -> PyResult<usize> {
    let (Ok(v), Ok(limit)) = (u32::try_from(value), usize::try_from(mx)) else {
        return Err(Fail::Python); // the reference's search (refs_reference)
    };
    let found = core(x.refs(v, limit))?;
    let data = x.data_refs(v, 10);
    if !data.is_empty() {
        let list: Vec<String> = data.iter().map(|d| format!("{d:08X}")).collect();
        py.print(&format!(
            "    held in data at {}{}  (a table or a vtable: exeref.py refs <that address> finds its users)",
            list.join(", "),
            if data.len() >= 10 { " ..." } else { "" }
        ))?;
    }
    for (va, ins, _kind) in &found {
        let name = func_name(x, i64::from(*va), syms)?;
        py.print(&format!("    {va:08X}  {} {:<40} in {name}", ins.mnemonic, ins.op_str))?;
        if dis_n != 0 {
            let w = i64_of(dis_n * 4)?;
            for line in core(x.window(i64::from(*va), w, w, syms))? {
                py.print(&format!("    {line}"))?;
            }
            py.print("")?;
        }
    }
    Ok(found.len())
}

/// `dis <address>`: the window around it.
fn cmd_dis(py: &mut Py, x: &Exe, target: &str, before: i128, after: i128, syms: &Syms) -> PyResult<i32> {
    let va = parse_int(&Taken::Val(target.to_owned()), "")?;
    if !(i64::try_from(va).is_ok_and(|v| x.is_code(v))) {
        return Err(err(format!("{} is not in the exe's code", hex_alt(va))));
    }
    let v = i64_of(va)?;
    py.print(&format!("{} in {}", hex_width(va, 8), func_name(x, v, syms)?))?;
    for line in core(x.window(v, i64_of(before)?, i64_of(after)?, syms))? {
        py.print(&line)?;
    }
    Ok(0)
}

/// `refs <address>`: its string and name, then its users.
fn cmd_refs(py: &mut Py, x: &Exe, target: &str, mx: i128, dis_n: i128, syms: &Syms) -> PyResult<i32> {
    let va = parse_int(&Taken::Val(target.to_owned()), "")?;
    let Ok(v) = u32::try_from(va) else {
        return Err(Fail::Python); // printed, then the reference's search
    };
    let mut line = format!("{v:08X}");
    if let Some(s) = x.cstr(i64::from(v), 120).filter(|s| !s.is_empty()) {
        let _ = write!(line, "  \"{s}\"");
    }
    if let Some((n, _)) = syms.get(&v) {
        let _ = write!(line, "  {n}");
    }
    py.print(&line)?;
    let n = show_refs(py, x, va, mx, dis_n, syms)?;
    let reached = i128::try_from(n).unwrap_or(i128::MAX) >= mx;
    py.print(&format!(
        "{n} place(s){}",
        if reached { " (--max reached)" } else { "" }
    ))?;
    Ok(0)
}

/// `str <text>`: the strings, each with its users.
fn cmd_str(py: &mut Py, x: &Exe, text: &str, exact: bool, mx: i128, dis_n: i128, syms: &Syms) -> PyResult<i32> {
    let raw = latin1(text)?;
    if raw.contains(&0) {
        return Err(Fail::Python); // the reference's regular expression
    }
    let strings = x.find_strings(&raw, exact);
    if strings.is_empty() {
        py.print(&format!(
            "no string {} \"{text}\" in the exe",
            if exact { "equal to" } else { "containing" }
        ))?;
        return Ok(1);
    }
    if mx < 0 {
        return Err(Fail::Python); // a negative --max: the reference's search
    }
    let total = i128::try_from(strings.len()).unwrap_or(i128::MAX);
    for (va, s) in strings.iter().take(usize::try_from(mx).unwrap_or(usize::MAX)) {
        py.print(&format!("{va:08X}  \"{s}\""))?;
        if show_refs(py, x, i128::from(*va), mx, dis_n, syms)? == 0 {
            py.print("    (no instruction uses its address directly)")?;
        }
    }
    if total > mx {
        py.print(&format!("... {} more strings (--max)", total - mx))?;
    }
    Ok(0)
}

pub fn run(py: &mut Py, command_line: &[String]) -> PyResult<i32> {
    let mut args: Vec<String> = command_line.to_vec();
    let known = args
        .first()
        .is_some_and(|c| matches!(c.as_str(), "str" | "refs" | "dis"));
    if !known {
        py.print(&docstring(SOURCE))?;
        return Ok(i32::from(!args.is_empty()));
    }
    let cmd = args.remove(0);
    let exe_path = match take(&mut args, "--exe", true) {
        Taken::Val(v) if !v.is_empty() => Some(v),
        Taken::Yes => None,
        _ => find_exe(),
    };
    let Some(exe_path) = exe_path else {
        return Err(err("BFBC2Game.exe not found - give it with --exe <path>"));
    };
    if !isfile(&exe_path) {
        return Err(PyErr::file_not_found(&exe_path).into());
    }
    let mut paths = vec![Some(join(&here(), "symbols_795745.txt"))];
    while args.iter().any(|a| a == "--symbols") {
        paths.push(match take(&mut args, "--symbols", true) {
            Taken::Val(v) => Some(v),
            _ => None,
        });
    }
    let syms = read_symbols(py, &paths)?;
    let exact = take(&mut args, "--exact", false) == Taken::Yes;
    let mx = int_opt(&take(&mut args, "--max", true), 40)?;
    let dis_n = match take(&mut args, "--dis", true) {
        Taken::No => 0,
        Taken::Yes => 6,
        Taken::Val(v) => py_int(&v, 10)?,
    };
    let before = parse_int(&take(&mut args, "--before", true), "0x20")?;
    let after = parse_int(&take(&mut args, "--after", true), "0x40")?;
    let Some(first) = args.first().cloned() else {
        return Err(err(format!(
            "{cmd} needs a {}",
            if cmd == "str" { "text" } else { "address" }
        )));
    };
    let data = std::fs::read(&exe_path).map_err(|e| PyErr::new("OSError", format!("{exe_path}: {e}")))?;
    let img = Image::parse(&data).map_err(|_| Fail::Python)?;
    drop(data);
    let x = Exe::new(img, true);
    match cmd.as_str() {
        "dis" => cmd_dis(py, &x, &first, before, after, &syms),
        "refs" => cmd_refs(py, &x, &first, mx, dis_n, &syms),
        _ => cmd_str(py, &x, &args.join(" "), exact, mx, dis_n, &syms),
    }
}
