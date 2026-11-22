//! `bfbc2 convert havok|verify-havok|texture|verify-texture ...`: the console converter's command line
//! (`bfbc2-console-convert\convert.py`) over `bc2_convert` (`hkclasses`, `havok`, `texture`, `texcheck`), with its
//! `argparse` parser ported for the command lines it takes (usage, errors and help; at the console's width or
//! `COLUMNS` [`crate::argfmt`]'s texts, round RT2).
//!
//! Option abbreviations resolve as argparse resolves them, with its ambiguity error (round RT3). Handed to the Python:
//! `--`, single-dash options other than `-h`; an executable whose class
//! layouts the core cannot read (the reference raises its own exception) or that the converter does not take; a
//! wrapper or texture check the core leaves to the reference; a correlation within 1e-9 of the 0.9 threshold
//! (NumPy's sums decide it there).

#![allow(
    clippy::indexing_slicing,
    reason = "indexes and slices within lengths checked just before (the arguments, the manifest's fields, the pictures)"
)]
#![allow(clippy::many_single_char_names, reason = "the reference's names (a, k, o, w, c, p)")]

use std::collections::HashMap;

use bc2_convert::{havok, hkclasses, texcheck, texture};

use super::common::{bytes_repr, dirname, isdir, isfile, join, join_all};
use crate::argfmt::Long;
use crate::pyfmt::{str_repr, thousands};
use crate::pyproc::{Fail, Py, PyErr, PyResult, os_error, read_file, suite_root, write_file};

const MAIN_USAGE: &str = "usage: convert.py [-h] {havok,verify-havok,texture,verify-texture} ...\n";
const MAIN_HELP: &str = "usage: convert.py [-h] {havok,verify-havok,texture,verify-texture} ...\n\nConverts Xbox 360 \
    Bad Company 2 resources to the PC's formats (the user's own\nconsole files; nothing is shipped).\n\npositional \
    arguments:\n  {havok,verify-havok,texture,verify-texture}\n\noptions:\n  -h, --help            show this help \
    message and exit\n";
const CHOICES: [&str; 4] = ["havok", "verify-havok", "texture", "verify-texture"];

/// A subcommand's parser: its usage, help, positionals (name, one or more) and options.
struct Sub {
    name: &'static str,
    usage: &'static str,
    help: &'static str,
    positionals: &'static [(&'static str, bool)],
    options: &'static [&'static str],
}

const SUBS: [Sub; 4] = [
    Sub {
        name: "havok",
        usage: "usage: convert.py havok [-h] [--exe EXE] input output\n",
        help: "usage: convert.py havok [-h] [--exe EXE] input output\n\npositional arguments:\n  input\n  output\n\n\
               options:\n  -h, --help  show this help message and exit\n  --exe EXE\n",
        positionals: &[("input", false), ("output", false)],
        options: &["--exe"],
    },
    Sub {
        name: "verify-havok",
        usage: "usage: convert.py verify-havok [-h] [--exe EXE] [--report REPORT]\n                               \
                pc console [console ...]\n",
        help: "usage: convert.py verify-havok [-h] [--exe EXE] [--report REPORT]\n                               pc \
               console [console ...]\n\npositional arguments:\n  pc\n  console\n\noptions:\n  -h, --help       show \
               this help message and exit\n  --exe EXE\n  --report REPORT\n",
        positionals: &[("pc", false), ("console", true)],
        options: &["--exe", "--report"],
    },
    Sub {
        name: "texture",
        usage: "usage: convert.py texture [-h] input output\n",
        help: "usage: convert.py texture [-h] input output\n\npositional arguments:\n  input\n  output\n\noptions:\n  \
               -h, --help  show this help message and exit\n",
        positionals: &[("input", false), ("output", false)],
        options: &[],
    },
    Sub {
        name: "verify-texture",
        usage: "usage: convert.py verify-texture [-h] [--report REPORT]\n                                 pc console \
                [console ...]\n",
        help: "usage: convert.py verify-texture [-h] [--report REPORT]\n                                 pc console \
               [console ...]\n\npositional arguments:\n  pc\n  console\n\noptions:\n  -h, --help       show this \
               help message and exit\n  --report REPORT\n",
        positionals: &[("pc", false), ("console", true)],
        options: &["--report"],
    },
];

/// The parsed command line (`argparse.Namespace`).
#[derive(Default)]
struct Args {
    positionals: HashMap<&'static str, Vec<String>>,
    options: HashMap<&'static str, String>,
}

impl Args {
    fn one(&self, name: &str) -> &str {
        self.positionals
            .get(name)
            .and_then(|v| v.first())
            .map_or("", String::as_str)
    }

    fn many(&self, name: &str) -> &[String] {
        self.positionals.get(name).map_or(&[], Vec::as_slice)
    }

    fn opt(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }
}

/// `parser.error(message)`: the usage (at the console's width) and the message on stderr, exit code 2.
fn error(py: &mut Py, usage: &str, prog: &str, message: &str) -> PyResult<Option<(usize, Args)>> {
    py.eprint(crate::argfmt::usage_or(prog, usage)?.trim_end_matches('\n'));
    py.eprint(&format!("{prog}: error: {message}"));
    Err(Fail::Err(PyErr::new("SystemExit", "2")))
}

/// The help of the parser `prog` at the console's width (`text`, recorded, at the pipe's).
fn help(py: &mut Py, prog: &str, text: &str) -> PyResult<Option<(usize, Args)>> {
    py.write(&crate::argfmt::help_or(prog, text)?)?;
    Ok(None)
}

/// Whether an argument is an option string to `argparse` ('O') rather than a value ('A'): it starts with '-', is not
/// '-' alone and is not a negative number (the parsers have no option that looks like one).
fn is_option(a: &str) -> bool {
    let Some(rest) = a.strip_prefix('-') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    // _negative_number_matcher: ^-\d+$|^-\d*\.\d+$
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    let neg = match rest.split_once('.') {
        None => digits(rest),
        Some((i, f)) => digits(i) && !f.is_empty() && digits(f),
    };
    !neg
}

/// `ap.parse_args(argv)`: Some((subcommand, its arguments)), None after `-h` (the help printed), or the error.
#[allow(
    clippy::too_many_lines,
    reason = "argparse's loop over options and runs of values, kept in one piece"
)]
fn parse(py: &mut Py, argv: &[String]) -> PyResult<Option<(usize, Args)>> {
    let Some(cmd) = argv.first() else {
        return error(
            py,
            MAIN_USAGE,
            "convert.py",
            "the following arguments are required: cmd",
        );
    };
    if cmd == "-h" || cmd == "--help" {
        return help(py, "convert.py", MAIN_HELP);
    }
    if cmd.starts_with("--")
        && !cmd.contains('=')
        && crate::argfmt::long_option(cmd, &["-h", "--help"]) == Long::Found("--help")
    {
        return help(py, "convert.py", MAIN_HELP); // an abbreviation of --help (round RT3)
    }
    if cmd.starts_with('-') && cmd != "-" {
        return Err(Fail::Python);
    }
    let Some(k) = CHOICES.iter().position(|c| c == cmd) else {
        let choices: Vec<String> = CHOICES.iter().map(|c| str_repr(c)).collect();
        let msg = format!(
            "argument cmd: invalid choice: {} (choose from {})",
            str_repr(cmd),
            choices.join(", ")
        );
        return error(py, MAIN_USAGE, "convert.py", &msg);
    };
    let sub = &SUBS[k];
    let prog = format!("convert.py {}", sub.name);
    let rest = &argv[1..];
    let mut i = 0;
    let mut parsed = Args::default();
    let mut extras: Vec<String> = Vec::new();
    let mut left: &[(&'static str, bool)] = sub.positionals;
    while i < rest.len() {
        let a = rest[i].as_str();
        if is_option(a) {
            if a == "-h" || a == "--help" {
                return help(py, &prog, sub.help);
            }
            if a == "--" || !a.starts_with("--") {
                return Err(Fail::Python);
            }
            let inline = a.split_once('=').map(|(_, v)| v);
            // round RT3: an abbreviation resolved as argparse resolves it, or its ambiguity error
            let strings: Vec<&'static str> = ["-h", "--help"]
                .into_iter()
                .chain(sub.options.iter().copied())
                .collect();
            let name = match crate::argfmt::long_option(a, &strings) {
                Long::Found("--help") if inline.is_none() => return help(py, &prog, sub.help),
                Long::Found("--help") | Long::Spaced => return Err(Fail::Python),
                Long::Found(s) => s,
                Long::Ambiguous(m) => return error(py, sub.usage, &prog, &m),
                Long::Unknown => "",
            };
            if let Some(&opt) = sub.options.iter().find(|o| **o == name) {
                match inline {
                    Some(v) => parsed.options.insert(opt, v.to_owned()),
                    None => match rest.get(i + 1) {
                        Some(v) if !is_option(v) && v != "--" => {
                            i += 1;
                            parsed.options.insert(opt, v.clone())
                        }
                        _ => return error(py, sub.usage, &prog, &format!("argument {opt}: expected one argument")),
                    },
                };
                i += 1;
                continue;
            }
            extras.push(a.to_owned());
            i += 1;
            continue;
        }
        // a run of values: argparse's consume_positionals gives each positional as many as its pattern takes
        let mut run = 0;
        while i + run < rest.len() && !is_option(&rest[i + run]) {
            run += 1;
        }
        let mut take = left.len();
        let counts = loop {
            if take == 0 {
                break Vec::new();
            }
            let need: usize = take;
            if run >= need {
                // all but the last one-or-more positional take one; a '+' takes the rest of the run
                let mut c = vec![1usize; take];
                if left[take - 1].1 {
                    c[take - 1] = run - (take - 1);
                }
                break c;
            }
            take -= 1;
        };
        let mut at = i;
        for (k, n) in counts.iter().enumerate() {
            parsed.positionals.insert(left[k].0, rest[at..at + n].to_vec());
            at += n;
        }
        left = &left[counts.len()..];
        extras.extend(rest[at..i + run].iter().cloned());
        i += run;
    }
    if !left.is_empty() {
        let names: Vec<&str> = left.iter().map(|p| p.0).collect();
        return error(
            py,
            sub.usage,
            &prog,
            &format!("the following arguments are required: {}", names.join(", ")),
        );
    }
    if !extras.is_empty() {
        return error(
            py,
            MAIN_USAGE,
            "convert.py",
            &format!("unrecognized arguments: {}", extras.join(" ")),
        );
    }
    Ok(Some((k, parsed)))
}

/// `default_exe()`: `OPFOR_ROOT`'s `game\BFBC2Game.exe`, else that of the `project-opfor-bfbc2` folder beside the suite.
fn default_exe() -> PyResult<String> {
    let mut cands = Vec::new();
    if let Some(v) = std::env::var("OPFOR_ROOT").ok().filter(|v| !v.is_empty()) {
        cands.push(v);
    }
    let suite = suite_root().to_string_lossy().into_owned();
    cands.push(join(&dirname(&suite).ok_or(Fail::Python)?, "project-opfor-bfbc2").ok_or(Fail::Python)?);
    for c in cands {
        let exe = join_all(&c, ["game", "BFBC2Game.exe"]).ok_or(Fail::Python)?;
        if isfile(&exe) {
            return Ok(exe);
        }
    }
    Err(PyErr::new(
        "FileNotFoundError",
        "no game copy (Project OPFOR BFBC2) found for BFBC2Game.exe: pass --exe",
    )
    .into())
}

/// `havok.classes(exe)` as the converter's `HavokClasses` (the classes dict: by name, first place, last value).
fn classes(exe: &str) -> PyResult<havok::Classes> {
    let data = std::fs::read(exe).map_err(|e| os_error(exe, &e))?;
    let rows = hkclasses::classes(&data).ok_or(Fail::Python)?;
    let mut out: Vec<havok::Klass> = Vec::new();
    for (name, parent, size, members) in rows {
        let k = havok::Klass {
            name: name.clone(),
            parent,
            size,
            members: members
                .into_iter()
                .map(|(name, cls, ty, subtype, carray, _flags, offset)| havok::Member {
                    name,
                    cls,
                    ty,
                    subtype,
                    carray,
                    offset: u32::from(offset),
                })
                .collect(),
        };
        if let Some(slot) = out.iter_mut().find(|x| x.name == name) {
            *slot = k;
        } else {
            out.push(k);
        }
    }
    Ok(havok::Classes::new(&out))
}

fn convert_error(msg: impl Into<String>) -> Fail {
    Fail::Err(PyErr::new("ConvertError", msg))
}

/// `havok.wrapper_end(b)`.
fn wrapper_end(b: &[u8]) -> PyResult<usize> {
    b.windows(8)
        .position(|w| w == havok::MAGIC)
        .ok_or_else(|| convert_error("no Havok packfile in the resource"))
}

fn convert_wrapper(b: &[u8], o: usize) -> PyResult<Vec<u8>> {
    havok::convert_wrapper(b, o).map_err(|e| match e {
        havok::WrapperError::Refused(m) => convert_error(m),
        havok::WrapperError::Reference => Fail::Python,
    })
}

/// `havok._stopped(b, o)`.
fn stopped(b: &[u8], o: usize) -> String {
    let v = b.get(o + 40..(o + 56).min(b.len())).unwrap_or_default();
    if !v.starts_with(b"Havok-5.5.0") {
        return format!("Havok version {}, not 5.5.0", bytes_repr(v));
    }
    "the packfile is not converted: it reads past the end of the data, names a class or member type the game's \
     layouts lack, or nests objects too deeply"
        .to_owned()
}

fn convert_packfile(b: &[u8], o: usize, k: &havok::Classes) -> PyResult<Vec<u8>> {
    havok::convert_packfile(b, o, k).map_err(|e| match e {
        havok::HavokError::Refused(m) => convert_error(m),
        havok::HavokError::Reference => convert_error(stopped(b, o)),
    })
}

/// `havok.verify`'s answer: (exact, {reason: bytes} in order, unexplained offsets).
type Verified = (bool, Vec<(&'static str, usize)>, Vec<usize>);

/// `havok.verify(console, pc, klasses)`.
fn verify(console: &[u8], pc: &[u8], k: &havok::Classes) -> PyResult<Verified> {
    let o = wrapper_end(console)?;
    let w = convert_wrapper(console, o)?;
    let got = match havok::verify_packfile(console, o, k, pc) {
        Ok(g) => g,
        Err(havok::HavokError::Refused(m)) => return Err(convert_error(m)),
        Err(havok::HavokError::Reference) => {
            convert_packfile(console, o, k)?;
            if pc.len() > console.len() {
                return Err(convert_error(format!(
                    "the PC file is longer than the console one ({} > {} bytes)",
                    pc.len(),
                    console.len()
                )));
            }
            return Err(convert_error(
                "not verified: a cached bounding volume lies past the end of the data, or a byte lies in two \
                 bounding volumes that answer differently",
            ));
        }
    };
    let mut why: Vec<(&'static str, usize)> = Vec::new();
    let mut bad = Vec::new();
    if w.as_slice() != &pc[..o.min(pc.len())] {
        let first = (0..o)
            .find(|&i| i >= w.len() || pc.get(i).is_none_or(|&c| w[i] != c))
            .ok_or(Fail::Python)?;
        if pc.len() < o {
            return Err(Fail::Python);
        }
        bad.push(first);
    }
    for (i, r) in got {
        match r {
            Some(r) => {
                let t = r.text();
                if let Some(e) = why.iter_mut().find(|e| e.0 == t) {
                    e.1 += 1;
                } else {
                    why.push((t, 1));
                }
            }
            None => bad.push(i),
        }
    }
    Ok((why.is_empty() && bad.is_empty(), why, bad))
}

/// A `collections.Counter` of strings: counts in insertion order, `most_common()` by count, ties as inserted.
#[derive(Default)]
struct Counter(Vec<(String, usize)>);

impl Counter {
    fn add(&mut self, k: &str, n: usize) {
        if let Some(e) = self.0.iter_mut().find(|e| e.0 == k) {
            e.1 += n;
        } else {
            self.0.push((k.to_owned(), n));
        }
    }

    fn get(&self, k: &str) -> usize {
        self.0.iter().find(|e| e.0 == k).map_or(0, |e| e.1)
    }

    fn total(&self) -> usize {
        self.0.iter().map(|e| e.1).sum()
    }

    fn most_common(&self) -> Vec<(String, usize)> {
        let mut v = self.0.clone();
        v.sort_by_key(|e| std::cmp::Reverse(e.1));
        v
    }
}

/// `convert._find(root, rel)`: the entry in any case, part by part.
fn find(root: &str, rel: &str) -> PyResult<Option<String>> {
    let mut cur = root.to_owned();
    for part in rel.split('/') {
        if !isdir(&cur) {
            return Ok(None);
        }
        let rd = std::fs::read_dir(&cur).map_err(|_| Fail::Python)?;
        let low = part.to_lowercase();
        let mut hit = None;
        for e in rd {
            let e = e.map_err(|_| Fail::Python)?;
            let name = e.file_name().into_string().map_err(|_| Fail::Python)?;
            if name.to_lowercase() == low {
                hit = Some(name);
                break;
            }
        }
        let Some(name) = hit else { return Ok(None) };
        cur = join(&cur, &name).ok_or(Fail::Python)?;
    }
    Ok(Some(cur))
}

/// `convert.manifest(folder, kind)`: {lower-case relative path: full path} (sorted keys are all that is read).
fn manifest(folder: &str, kind: &str) -> PyResult<HashMap<String, String>> {
    let man = join(folder, "fbrb.manifest").ok_or(Fail::Python)?;
    let raw = std::fs::read(&man).map_err(|e| os_error(&man, &e))?;
    let text = String::from_utf8_lossy(&raw).replace("\r\n", "\n").replace('\r', "\n");
    let mut out = HashMap::new();
    for line in text.split_inclusive('\n') {
        let f: Vec<&str> = line.trim_end_matches('\n').split('|').collect();
        if f.len() >= 2 && f[1] == kind {
            let mut p = Some(join_all(folder, f[0].split('/')).ok_or(Fail::Python)?);
            if !p.as_deref().is_some_and(isfile) {
                p = find(folder, f[0])?;
            }
            if let Some(p) = p {
                out.insert(f[0].to_lowercase(), p);
            }
        }
    }
    Ok(out)
}

fn common(pc: &HashMap<String, String>, con: &HashMap<String, String>) -> Vec<String> {
    let mut keys: Vec<String> = pc.keys().filter(|k| con.contains_key(*k)).cloned().collect();
    keys.sort();
    keys
}

fn consoles(folders: &[String], kind: &str) -> PyResult<HashMap<String, String>> {
    let mut con = HashMap::new();
    for f in folders {
        #[allow(
            clippy::iter_over_hash_type,
            reason = "each manifest names a file once; the first folder holding it wins either way"
        )]
        for (rel, p) in manifest(f, kind)? {
            con.entry(rel).or_insert(p);
        }
    }
    Ok(con)
}

/// The report file (`open(report, 'w', encoding='utf-8').write(...)`: text mode, `\n` as `\r\n`).
fn write_report(path: &str, lines: &[String]) -> PyResult<()> {
    let text = format!("{}\n", lines.join("\n")).replace('\n', "\r\n");
    write_file(path, text.as_bytes())
}

fn cmd_havok(py: &mut Py, a: &Args) -> PyResult<i32> {
    let exe = match a.opt("--exe").filter(|e| !e.is_empty()) {
        Some(e) => e.to_owned(),
        None => default_exe()?,
    };
    let k = classes(&exe)?;
    let input = a.one("input");
    let data = read_file(input)?;
    let o = wrapper_end(&data)?;
    let w = convert_wrapper(&data, o)?;
    let out = convert_packfile(&data, o, &k)?;
    let mut res = w;
    res.extend_from_slice(&out[o..]);
    let output = a.one("output");
    write_file(output, &res)?;
    py.print(&format!(
        "{output}: {} bytes",
        thousands(i128::try_from(res.len()).unwrap_or(i128::MAX))
    ))?;
    Ok(0)
}

fn cmd_verify_havok(py: &mut Py, a: &Args) -> PyResult<i32> {
    let exe = match a.opt("--exe").filter(|e| !e.is_empty()) {
        Some(e) => e.to_owned(),
        None => default_exe()?,
    };
    let k = classes(&exe)?;
    let pc = manifest(a.one("pc"), "HavokPhysicsData")?;
    let con = consoles(a.many("console"), "HavokPhysicsData")?;
    let mut total = Counter::default();
    let mut reasons = Counter::default();
    let mut lines: Vec<String> = Vec::new();
    for rel in common(&pc, &con) {
        let c = read_file(&con[&rel])?;
        let p = read_file(&pc[&rel])?;
        if c.len() != p.len() {
            total.add("different asset (sizes differ)", 1);
            lines.push(format!("different asset  {rel}"));
            continue;
        }
        match verify(&c, &p, &k) {
            Err(Fail::Err(e)) if e.kind == "ConvertError" => {
                total.add("error", 1);
                lines.push(format!("ERROR  {rel}: {}", e.msg));
            }
            Err(e) => return Err(e),
            Ok((exact, why, bad)) => {
                if let Some(first) = bad.first() {
                    total.add("wrong", 1);
                    lines.push(format!("WRONG  {rel}: {} bytes, first at {first:#x}", bad.len()));
                } else if exact {
                    total.add("exact", 1);
                } else {
                    total.add("equivalent", 1);
                    for (r, n) in why {
                        reasons.add(r, n);
                    }
                }
            }
        }
    }
    let parts: Vec<String> = total.most_common().iter().map(|(k, v)| format!("{v} {k}")).collect();
    py.print(&format!("{} files: {}", total.total(), parts.join(", ")))?;
    for (r, n) in reasons.most_common() {
        py.print(&format!(
            "  differing bytes the game does not read or rebuilds: {} {r}",
            thousands(i128::try_from(n).unwrap_or(i128::MAX))
        ))?;
    }
    if let Some(r) = a.opt("--report").filter(|r| !r.is_empty()) {
        write_report(r, &lines)?;
    }
    for l in &lines {
        if !l.starts_with("different") {
            py.print(l)?;
        }
    }
    Ok(i32::from(total.get("wrong") > 0 || total.get("error") > 0))
}

fn cmd_texture(py: &mut Py, a: &Args) -> PyResult<i32> {
    let data = read_file(a.one("input"))?;
    let out = texture::convert(&data).map_err(|e| convert_error(e.to_string()))?;
    let output = a.one("output");
    write_file(output, &out)?;
    py.print(&format!(
        "{output}: {} bytes",
        thousands(i128::try_from(out.len()).unwrap_or(i128::MAX))
    ))?;
    Ok(0)
}

/// `texture.check(pc, reference)`: (worst, corr); the correlation exact in the core as texture.py's
/// (`texcheck::correlation`, round CI1: the same value as the Python's, so no margin around the 0.9 threshold).
fn check(pc: &[u8], reference: Option<&[u8]>) -> PyResult<(Option<f64>, Option<f64>)> {
    let Some(got) = texcheck::check(pc, reference) else {
        return Err(Fail::Python);
    };
    let Some(c) = got else { return Ok((None, None)) };
    Ok((Some(c.worst), c.corr))
}

fn cmd_verify_texture(py: &mut Py, a: &Args) -> PyResult<i32> {
    let pc = manifest(a.one("pc"), "ITexture")?;
    let con = consoles(a.many("console"), "XenonTexture")?;
    let mut total = Counter::default();
    let mut lines: Vec<String> = Vec::new();
    let slice = |b: &[u8], lo: usize, hi: usize| -> Vec<u8> {
        b.get(lo.min(b.len())..hi.min(b.len())).unwrap_or_default().to_vec()
    };
    for rel in common(&pc, &con) {
        let c = read_file(&con[&rel])?;
        let p = read_file(&pc[&rel])?;
        let mut c812 = slice(&c, 8, 12);
        c812.reverse();
        let mut want = slice(&p, 16, 20);
        want.reverse();
        let mut p2024 = slice(&p, 20, 24);
        p2024.reverse();
        want.extend(p2024);
        if c812 != slice(&p, 8, 12) || slice(&c, 16, 24) != want {
            total.add("not comparable (size or format differs)", 1);
            continue;
        }
        let out = match texture::convert(&c) {
            Ok(o) => o,
            Err(e) => {
                total.add("not converted", 1);
                lines.push(format!("not converted  {rel}: {e}"));
                continue;
            }
        };
        if out == p {
            total.add("byte-identical", 1);
            continue;
        }
        let (ours, corr) = check(&out, Some(&p))?;
        let (theirs, _) = check(&p, None)?;
        if let (Some(ours), Some(theirs)) = (ours, theirs) {
            if ours > (theirs + 3.0).max(theirs * 1.25) {
                total.add("broken mip chain", 1);
                lines.push(format!(
                    "BROKEN  {rel}: mip chain error {ours:.1} (the PC file {theirs:.1})"
                ));
            } else if corr.is_some_and(|c| c < 0.9) {
                total.add("different art", 1);
            } else {
                total.add("same picture, re-encoded", 1);
            }
        } else {
            total.add("not compared (format)", 1);
            let fmt = u32::from_le_bytes(p.get(8..12).and_then(|s| s.try_into().ok()).ok_or(Fail::Python)?);
            lines.push(format!("NOT COMPARED  {rel}: format {fmt} (no decoder)"));
        }
    }
    let parts: Vec<String> = total.most_common().iter().map(|(k, v)| format!("{v} {k}")).collect();
    py.print(&format!("{} textures: {}", total.total(), parts.join(", ")))?;
    for l in &lines {
        py.print(l)?;
    }
    if let Some(r) = a.opt("--report").filter(|r| !r.is_empty()) {
        write_report(r, &lines)?;
    }
    Ok(i32::from(
        total.get("broken mip chain") > 0 || total.get("not converted") > 0,
    ))
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let Some((k, a)) = (match parse(py, args) {
        Ok(v) => v,
        Err(Fail::Err(e)) if e.kind == "SystemExit" => return Ok(2),
        Err(e) => return Err(e),
    }) else {
        return Ok(0);
    };
    match SUBS[k].name {
        "havok" => cmd_havok(py, &a),
        "verify-havok" => cmd_verify_havok(py, &a),
        "texture" => cmd_texture(py, &a),
        _ => cmd_verify_texture(py, &a),
    }
}
