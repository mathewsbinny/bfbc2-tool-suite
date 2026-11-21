//! Round O3's `bf1943` group: `bfbc2-bf1943-import`'s `dbxcatalogue.py`, `dbxparity.py` and `portparity.py` as
//! command lines.
//!
//! - `bfbc2 dbxcatalogue`: the catalogue's summary line from its cache (`work\tools_out\dbx_catalogue.pkl`, read with
//!   `bc2_pyrt::pickle`, over the index's cache: [`crate::gameindex`]). Building a missing catalogue or index (minutes,
//!   once; the Python writes the cache) is handed to the Python.
//! - `bfbc2 dbxparity|portparity`: their `argparse` command lines (help, usage errors; at the console's width or
//!   `COLUMNS` [`crate::argfmt`]'s texts, round RT2); a run
//!   (BF1943's source, dbxport's / meshport's ports, NumPy) is handed to the Python, as are `--`, attached values
//!   and positionals on both sides of an option. Option abbreviations resolve as argparse resolves them, with its
//!   ambiguity error (round RT3).

use std::fmt::Write as _;

use bc2_pyrt::pickle::{self, Value};
use bc2_pyrt::py::join;

use crate::argfmt::Long;
use crate::gameindex;
use crate::pyos::files as fsx;
use crate::pyproc::{Fail, Py, PyResult, Tool};

/// The group's tools.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "dbxcatalogue",
        script: "bfbc2-bf1943-import\\dbxcatalogue.py",
        log: None,
        run: dbxcatalogue,
    },
    Tool {
        name: "dbxparity",
        script: "bfbc2-bf1943-import\\dbxparity.py",
        log: None,
        run: dbxparity,
    },
    Tool {
        name: "portparity",
        script: "bfbc2-bf1943-import\\portparity.py",
        log: None,
        run: portparity,
    },
];

/// `dbxcatalogue.CACHE_VERSION`
const CACHE_VERSION: i64 = 3;

/// `dbxcatalogue._signature(ix, exe)`: SHA-1 over each bundle of the index (sorted) and the exe, `path|size|mtime`.
fn signature(ix: &gameindex::Index, exe: &str) -> String {
    let mut bundles: Vec<&str> = ix.bundles().collect();
    bundles.sort_unstable();
    bundles.dedup();
    bundles.push(exe);
    let mut h = sha1_smol::Sha1::new();
    for b in bundles {
        let line = match fsx::stat(b) {
            Ok(st) => format!("{b}|{}|{}\n", st.size, st.mtime.trunc()),
            Err(_) => format!("{b}|missing\n"),
        };
        h.update(line.as_bytes());
    }
    h.digest().to_string()
}

/// `python dbxcatalogue.py`: `load(meshview.Index(shaderdb.find_opfor(None)))` and its counts (any arguments are
/// ignored, as the Python ignores them).
fn dbxcatalogue(py: &mut Py, _argv: &[String]) -> PyResult<i32> {
    let t = fsx::now();
    let root = gameindex::find_opfor(None)?;
    let ix = gameindex::load(py, &root)?;
    let exe = join(&root, &["game", "BFBC2Game.exe"]);
    let sig = signature(&ix, &exe);
    let path = join(&root, &["work", "tools_out", "dbx_catalogue.pkl"]);
    let data = std::fs::read(&path).map_err(|_| Fail::Python)?;
    let c = pickle::loads(&data).map_err(|_| Fail::Python)?;
    if c.get("version").and_then(|v| v.as_int()) != Some(CACHE_VERSION)
        || c.get("sig").and_then(|s| s.as_str().map(str::to_owned)).as_deref() != Some(sig.as_str())
    {
        return Err(Fail::Python);
    }
    let cat = c.get("catalogue").ok_or(Fail::Python)?;
    let len = |k: &str| cat.get(k).and_then(|v| v.len()).ok_or(Fail::Python);
    let Some(Value::Int(partitions)) = cat.get("partitions") else {
        return Err(Fail::Python);
    };
    let line = format!(
        "{partitions} partitions, {} classes, {} guids, {} paths, {} executable names ({:.0} s)",
        len("classes")?,
        len("guids")?,
        len("shapes")?,
        len("exe_names")?,
        fsx::now() - t
    );
    py.print(&line)?;
    Ok(0)
}

/// A parity tool's `argparse` parser: `names` (`*`), `-o` (default `<tool>.txt`), and `--predict` (dbxparity).
struct Parser {
    prog: &'static str,
    usage: &'static str,
    help: &'static str,
    predict: bool,
}

const DBXPARITY: Parser = Parser {
    prog: "dbxparity.py",
    usage: "usage: dbxparity.py [-h] [-o O] [--predict] [names ...]\n",
    help: "usage: dbxparity.py [-h] [-o O] [--predict] [names ...]\n\ndbxparity.py - BF1943's data partitions against \
           DICE's own versions, field by\nfield: the ground truth for dbxport.\n\npositional arguments:\n  names\n\n\
           options:\n  -h, --help  show this help message and exit\n  -o O\n  --predict   score dbxport's value passes \
           and DICE's conventions\n",
    predict: true,
};

const PORTPARITY: Parser = Parser {
    prog: "portparity.py",
    usage: "usage: portparity.py [-h] [-o O] [names ...]\n",
    help: "usage: portparity.py [-h] [-o O] [names ...]\n\nportparity.py - how close our Battlefield 1943 conversions \
           come to DICE's own.\nBFBC2 ships PC versions of many\n\npositional arguments:\n  names\n\noptions:\n  -h, \
           --help  show this help message and exit\n  -o O\n",
    predict: false,
};

fn dbxparity(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    parse(py, &DBXPARITY, argv)
}

fn portparity(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    parse(py, &PORTPARITY, argv)
}

/// An option string to `argparse` ('O') rather than a value ('A'): starts with '-', is not '-' alone, and is not a
/// negative number (the parsers have no option that looks like one).
fn is_option(a: &str) -> bool {
    let Some(rest) = a.strip_prefix('-') else { return false };
    if rest.is_empty() {
        return false;
    }
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    let neg = match rest.split_once('.') {
        None => digits(rest),
        Some((i, f)) => digits(i) && !f.is_empty() && digits(f),
    };
    !neg
}

/// `ap.parse_args(argv)` of a parity tool: the help (exit 0) or a usage error (exit 2) here; a command line that
/// parses runs in the Python (BF1943's source, the ports, NumPy).
fn parse(py: &mut Py, p: &Parser, argv: &[String]) -> PyResult<i32> {
    let mut extras: Vec<&str> = Vec::new();
    let mut names_done = false;
    let mut in_run = false;
    let mut i = 0;
    while let Some(a) = argv.get(i).map(String::as_str) {
        if !is_option(a) {
            if names_done && !in_run {
                return Err(Fail::Python); // a second run of positionals: argparse's own rules
            }
            names_done = true;
            in_run = true;
            i += 1;
            continue;
        }
        in_run = false;
        match a {
            "-h" | "--help" => {
                py.write(&crate::argfmt::help_or(p.prog, p.help)?)?;
                return Ok(0);
            }
            "--predict" if p.predict => {}
            "-o" => match argv.get(i + 1) {
                Some(v) if !is_option(v) => i += 1,
                Some(v) if v == "--" => return Err(Fail::Python),
                _ => return error(py, p, "argument -o: expected one argument"),
            },
            _ => {
                // round RT3: a long option resolved as argparse resolves it (an abbreviation, or its ambiguity
                // error); single-dash forms (-oX) and `--` are argparse's own
                if !a.starts_with("--") || a.len() == 2 {
                    return Err(Fail::Python);
                }
                let strings: &[&'static str] = if p.predict {
                    &["-h", "--help", "-o", "--predict"]
                } else {
                    &["-h", "--help", "-o"]
                };
                let inline = a.contains('=');
                match crate::argfmt::long_option(a, strings) {
                    Long::Found("--help") if !inline => {
                        py.write(&crate::argfmt::help_or(p.prog, p.help)?)?;
                        return Ok(0);
                    }
                    Long::Found("--predict") if !inline => {}
                    Long::Ambiguous(m) => return error(py, p, &m),
                    Long::Unknown => extras.push(a),
                    _ => return Err(Fail::Python), // a flag given a value, an argument with a space
                }
            }
        }
        i += 1;
    }
    if !extras.is_empty() {
        return error(py, p, &format!("unrecognized arguments: {}", extras.join(" ")));
    }
    // every command line that parses: the run is the Python's
    Err(Fail::Python)
}

/// `parser.error(message)`: the usage (at the console's width) and the message on stderr, exit code 2.
fn error(py: &mut Py, p: &Parser, message: &str) -> PyResult<i32> {
    let mut s = String::new();
    let _ = write!(
        s,
        "{}{}: error: {message}",
        crate::argfmt::usage_or(p.prog, p.usage)?,
        p.prog
    );
    py.eprint(&s);
    Ok(2)
}
