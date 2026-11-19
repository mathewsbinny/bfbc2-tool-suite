//! `bfbc2 crashreader [crashreport.xml] [--exe ...] [--symbols ...]... [--log ...] [--no-code]`:
//! `crash-reader\crashreader.py`'s command line over `bc2_harness` (round E4f: the exe, the code around the crash,
//! the calls before the return addresses, the log window). Where the core refuses (an instruction outside the
//! capstone parity proof, an exe pefile maps by rules of its own, a log line it leaves to the Python) and where the
//! report is XML only expat reads (`crash::xml`), the command line goes to the Python.

use std::fmt::Write as _;

use bc2_harness::exe::{self, Exe, Syms};
use bc2_harness::oplog;
use bc2_harness::pe::Image;
use bc2_harness::x86::{Insn, Operand, reg_name};
use regex::Regex;

use super::common::{
    dirname, fromtimestamp, getmtime, here, isfile, join, read_symbols, regex_safe, seconds_of_day, split, strip,
};
use super::xml;
use crate::pyfmt::docstring;
use crate::pyproc::{Fail, Py, PyResult, os_error};

const SOURCE: &str = super::common::CRASHREADER;

const REGISTERS: [&str; 9] = ["Eax", "Ebx", "Ecx", "Edx", "Esi", "Edi", "Ebp", "Esp", "Eip"];

/// `read_report(path)`'s dict.
struct Report {
    kind: String,
    sku: String,
    build: String,
    time: String,
    category: Option<i128>,
    stack: Vec<i128>,
    registers: Vec<(&'static str, i128)>,
}

impl Report {
    fn reg(&self, name: &str) -> Option<i128> {
        self.registers.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
    }
}

/// `str.capitalize()` of an ASCII name: the first letter upper case, the rest lower case.
fn capitalize(name: &str) -> String {
    let mut c = name.chars();
    c.next().map_or_else(String::new, |f| {
        format!("{}{}", f.to_ascii_uppercase(), c.as_str().to_ascii_lowercase())
    })
}

fn hex_value(v: &str) -> Option<i128> {
    let h = v.strip_prefix("0x")?;
    if h.is_empty() || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let h = h.trim_start_matches('0');
    if h.len() > 30 {
        return Some(i128::MAX); // marks a value past what is read here
    }
    Some(if h.is_empty() {
        0
    } else {
        i128::from_str_radix(h, 16).unwrap_or(i128::MAX)
    })
}

/// `read_report(path)` (`ET.parse`; the Python reads what `crash::xml` leaves to it).
fn read_report(path: &str) -> PyResult<Report> {
    let data = std::fs::read(path).map_err(|e| os_error(path, &e))?;
    let c = xml::read(&data).ok_or(Fail::Python)?;
    let text = |tag: &str| strip(c.get(tag).map_or("", String::as_str)).to_owned();
    let mut stack = Vec::new();
    for v in split(&text("stack")) {
        if let Some(x) = hex_value(v) {
            if x == i128::MAX {
                return Err(Fail::Python);
            }
            stack.push(x);
        }
    }
    let ctx = text("contextdata");
    if !ctx.is_ascii() || !regex_safe(&ctx) {
        return Err(Fail::Python);
    }
    let re = Regex::new(r"(\w+)\s+([0-9A-Fa-f]{8})").map_err(|_| Fail::Python)?;
    let mut registers: Vec<(&'static str, i128)> = Vec::new();
    for m in re.captures_iter(&ctx) {
        let (Some(n), Some(v)) = (m.get(1), m.get(2)) else {
            continue;
        };
        let name = n.as_str();
        let cap = capitalize(name);
        if let Some(r) = REGISTERS.iter().find(|r| **r == cap) {
            let value = i128::from_str_radix(v.as_str(), 16).unwrap_or(0);
            match registers.iter_mut().find(|(k, _)| k == r) {
                Some(slot) => slot.1 = value,
                None => registers.push((r, value)),
            }
        }
    }
    let cat = text("categoryid");
    let category = match hex_value(&cat) {
        Some(x) if x == i128::MAX => return Err(Fail::Python),
        other => other,
    };
    Ok(Report {
        kind: text("type"),
        sku: text("sku"),
        build: text("buildsignature"),
        time: text("createtime"),
        category,
        stack,
        registers,
    })
}

/// `find_reports()`: the `crashreport.xml` files of `Documents` (`OneDrive`'s too), newest first.
fn find_reports() -> PyResult<Vec<String>> {
    let home = home();
    let mut docs = vec![join(&home, "Documents"), join(&join(&home, "OneDrive"), "Documents")];
    if let Ok(rd) = std::fs::read_dir(if home.is_empty() { "." } else { home.as_str() }) {
        let mut names: Vec<String> = rd
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.to_lowercase().starts_with("onedrive") && !n.starts_with('.'))
            .collect();
        names.sort();
        for n in names {
            let d = join(&join(&home, &n), "Documents");
            if std::path::Path::new(&d).exists() {
                docs.push(d);
            }
        }
    }
    let mut found: Vec<(f64, String)> = Vec::new();
    for d in docs {
        for sub in ["Project OPFOR\\BFBC2", "BFBC2"] {
            let p = join(&join(&d, sub), "crashreport.xml");
            if isfile(&p) && !found.iter().any(|(_, q)| *q == p) {
                found.push((getmtime(&p)?, p));
            }
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(found.into_iter().map(|(_, p)| p).collect())
}

/// `os.path.expanduser('~')` on Windows.
fn home() -> String {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    if let Some(h) = env("USERPROFILE") {
        return h;
    }
    match (env("HOMEDRIVE"), env("HOMEPATH")) {
        (d, Some(p)) => format!("{}{p}", d.unwrap_or_default()),
        _ => "~".to_owned(),
    }
}

/// `null_operands(ins, regs)`: the registers a memory operand reads through that hold a null (or tiny) pointer.
fn null_operands(ins: &Insn, r: &Report) -> Vec<(String, i128, i64)> {
    let mut bad = Vec::new();
    for op in &ins.operands {
        let Operand::Mem { base, index, disp } = op else {
            continue;
        };
        for reg in [*base, *index] {
            let n = reg_name(reg);
            if n.is_empty() {
                continue;
            }
            let name = capitalize(&n);
            if let Some(v) = r.reg(&name)
                && v < 0x10000
            {
                bad.push((name, v, *disp));
            }
        }
    }
    bad
}

fn core<T>(r: Result<T, exe::Fallback>) -> PyResult<T> {
    r.map_err(|_| Fail::Python)
}

/// `name_of(a, syms, image)` for any Python int.
fn name_of(a: i128, syms: &Syms, x: Option<&Exe>) -> PyResult<(String, String)> {
    match i64::try_from(a) {
        Ok(v) => core(exe::name_of(v, syms, x)),
        Err(_) => Ok((String::new(), String::new())), // past every symbol and the image
    }
}

fn is_code(x: &Exe, a: i128) -> bool {
    i64::try_from(a).is_ok_and(|v| x.is_code(v))
}

/// `f'{v:X}'` of an int that may be negative.
fn signed_hex(v: i64) -> String {
    if v < 0 {
        format!("-{:X}", v.unsigned_abs())
    } else {
        format!("{v:X}")
    }
}

/// The command line as `main` reads it.
struct Options {
    args: Vec<String>,
    exe: Option<String>,
    log: Option<String>,
    symbols: Vec<Option<String>>,
    no_code: bool,
    help: bool,
}

/// `main`'s loop over the arguments: `--exe` / `--log` / `--symbols` with a value (without one they are plain
/// arguments), `-h` / `--help` (at once), `--no-code`.
fn options(command_line: &[String]) -> Options {
    let mut o = Options {
        args: Vec::new(),
        exe: None,
        log: None,
        symbols: Vec::new(),
        no_code: false,
        help: false,
    };
    let mut it = command_line.iter().peekable();
    while let Some(a) = it.next() {
        let a = a.as_str();
        if matches!(a, "--exe" | "--log" | "--symbols")
            && let Some(v) = it.peek().map(|v| (*v).clone())
        {
            it.next();
            match a {
                "--exe" => o.exe = Some(v),
                "--log" => o.log = Some(v),
                _ => o.symbols.push(Some(v)),
            }
            continue;
        }
        if a == "-h" || a == "--help" {
            o.help = true;
            return o;
        }
        if a == "--no-code" {
            o.no_code = true;
        } else {
            o.args.push(a.to_owned());
        }
    }
    o
}

/// The code around the crash, and a null pointer the faulting instruction reads through.
fn print_code(py: &mut Py, img: &Exe, eip: i128, r: &Report) -> PyResult<()> {
    let ev = i64::try_from(eip).map_err(|_| Fail::Python)?;
    let (start, ins, at) = core(img.around(ev, 0x10))?;
    py.print(&format!("\nCode (function starts near 0x{start:08X}):"))?;
    let window: &[Insn] = match at {
        Some(k) => ins
            .get(k.saturating_sub(10)..(k + 4).min(ins.len()))
            .unwrap_or_default(),
        None => ins.get(ins.len().saturating_sub(14)..).unwrap_or_default(),
    };
    for xi in window {
        let mark = if i64::from(xi.address) == ev { '>' } else { ' ' };
        py.print(&format!("  {mark} {:08X}  {} {}", xi.address, xi.mnemonic, xi.op_str))?;
    }
    if let Some(faulting) = at.and_then(|k| ins.get(k)) {
        for (reg, v, disp) in null_operands(faulting, r) {
            py.print(&format!(
                "\n  The faulting instruction reads memory through {reg} = 0x{v:X} (+0x{}): a null pointer.",
                signed_hex(disp)
            ))?;
            py.print("  The value it expected was never set - typically data the game looked up and did not find.")?;
        }
    }
    Ok(())
}

/// The call stack: each return address named, with the call before it.
fn print_stack(py: &mut Py, x: Option<&Exe>, r: &Report, eip: Option<i128>, syms: &Syms) -> PyResult<()> {
    py.print("\nCall stack (innermost first; return addresses):")?;
    let mut shown = 0;
    for &a in &r.stack {
        if let Some(img) = x
            && !(i128::from(img.base()) <= a && a < i128::from(img.end()))
        {
            let mut label =
                "(outside the exe: a system DLL, or a value on the stack that is not an address)".to_owned();
            if (0x2020_2020..=0x7A7A_7A7A).contains(&a) {
                let b = u32::try_from(a).unwrap_or(0).to_le_bytes();
                if b.iter().all(|c| (32..127).contains(c)) {
                    let t: String = b.iter().map(|&c| char::from(c)).collect();
                    label = format!("(text on the stack: \"{t}\" - not an address)");
                }
            }
            py.print(&format!("  0x{a:08X}  {label}"))?;
            continue;
        }
        let (n, h) = name_of(a, syms, x)?;
        let code = x.is_some_and(|img| is_code(img, a));
        let call = match x {
            Some(img) if code && Some(a) != eip => core(img.call_before(i64::try_from(a).map_err(|_| Fail::Python)?))?,
            _ => None,
        };
        let mut line = format!("  0x{a:08X}  {}", if n.is_empty() { "?" } else { n.as_str() });
        if let Some(c) = &call {
            let _ = write!(line, "    <- {} {}", c.mnemonic, c.op_str);
        } else if x.is_some() && code && Some(a) != eip {
            line.push_str("    (no call before it: probably not a return address)");
        } else if x.is_some() && !code {
            line.push_str("    (not code)");
        }
        py.print(&line)?;
        if !h.is_empty() && Some(a) != eip {
            py.print(&format!("              hint: {h}"))?;
        }
        shown += 1;
        if shown >= 40 {
            py.print("  ...")?;
            break;
        }
    }
    Ok(())
}

/// The log lines before the crash (`--log`, else the game's `logs\opfor.log` beside the exe).
fn print_log(py: &mut Py, log: Option<String>, exe_path: Option<&String>, mtime: f64) -> PyResult<()> {
    let mut log = log.filter(|l| !l.is_empty());
    if log.is_none()
        && let Some(e) = exe_path
    {
        let candidate = join(&join(&dirname(e), "logs"), "opfor.log");
        if isfile(&candidate) {
            log = Some(candidate);
        }
    }
    let Some(l) = log else { return Ok(()) };
    let want = seconds_of_day(mtime)?;
    let before = match std::fs::read(&l) {
        Err(_) => Vec::new(),
        Ok(data) => oplog::log_before_crash(&data, 18, Some(want)).map_err(|_| Fail::Python)?,
    };
    if before.is_empty() {
        py.print(&format!(
            "\n(no crash at the report's time in {l}: its log lines are not shown)"
        ))?;
    } else {
        py.print(&format!("\nLast log lines before the crash ({l}):"))?;
        for line in &before {
            let cut: String = line.chars().take(200).collect();
            py.print(&format!("  {cut}"))?;
        }
    }
    Ok(())
}

pub fn run(py: &mut Py, command_line: &[String]) -> PyResult<i32> {
    let o = options(command_line);
    if o.help {
        py.print(&docstring(SOURCE))?;
        return Ok(0);
    }
    if std::env::var("BFBC2_CRASHREADER_REFERENCE").is_ok_and(|v| !v.is_empty() && v != "0") {
        return Err(Fail::Python); // log_before_crash's reference, by request
    }
    let report_path = if let Some(p) = o.args.first() {
        p.clone()
    } else if let Some(p) = find_reports()?.into_iter().next() {
        p
    } else {
        py.print(
            "No crashreport.xml found (looked in Documents\\Project OPFOR\\BFBC2 and Documents\\BFBC2). Give its \
             path.",
        )?;
        return Ok(1);
    };
    let r = read_report(&report_path)?;
    let mut paths = vec![Some(join(&here(), "symbols_795745.txt"))];
    paths.extend(o.symbols);
    let syms = read_symbols(py, &paths)?;
    let exe_path = o.exe.filter(|e| !e.is_empty()).or_else(super::common::find_exe);
    let mut image: Option<Exe> = None;
    if let Some(e) = exe_path.as_ref().filter(|_| !o.no_code) {
        match std::fs::read(e) {
            Err(err) => py.print(&format!("(code view off: cannot read {e}: {e}: {err})"))?,
            Ok(data) => image = Some(Exe::new(Image::parse(&data).map_err(|_| Fail::Python)?, true)),
        }
    }
    let x = image.as_ref();
    let mtime = getmtime(&report_path)?;
    let when = fromtimestamp(mtime)?;
    py.print(&format!("Crash report {report_path}"))?;
    py.print(&format!(
        "  written {when} local ({} UTC); type {}; build {}; {}",
        r.time, r.kind, r.build, r.sku
    ))?;
    if !r.build.is_empty() && r.build != "795745" {
        py.print("  note: this is not build 795745 - the bundled names may not match")?;
    }
    let eip = r.reg("Eip").filter(|&v| v != 0).or(r.category).filter(|&v| v != 0);
    let (name, hint) = match eip {
        Some(e) => name_of(e, &syms, x)?,
        None => (String::new(), String::new()),
    };
    match eip {
        Some(e) => py.print(&format!("\nCrash at 0x{e:08X}  {name}"))?,
        None => py.print("\nCrash address unknown")?,
    }
    let regs: Vec<String> = REGISTERS
        .iter()
        .filter_map(|k| {
            r.reg(k)
                .map(|v| format!("{k} {v:08X}{}", if v == 0 { " (null)" } else { "" }))
        })
        .collect();
    py.print(&format!("  registers: {}", regs.join("  ")))?;
    if let (Some(img), Some(e)) = (x, eip)
        && is_code(img, e)
    {
        print_code(py, img, e, &r)?;
    }
    if !hint.is_empty() {
        py.print(&format!("\nHint: {hint}"))?;
    }
    print_stack(py, x, &r, eip, &syms)?;
    print_log(py, o.log, exe_path.as_ref(), mtime)?;
    Ok(0)
}
