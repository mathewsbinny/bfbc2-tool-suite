//! Round O3's `bf1943` group: `bfbc2-bf1943-import`'s `dbxcatalogue.py`, `dbxparity.py` and `portparity.py` as
//! command lines.
//!
//! - `bfbc2 dbxcatalogue`: the catalogue's summary line from its cache (`work\tools_out\dbx_catalogue.pkl`, read with
//!   `bc2_pyrt::pickle`, over the index's cache: [`crate::gameindex`]). Building a missing catalogue or index (minutes,
//!   once; the Python writes the cache) is handed to the Python.
//! - `bfbc2 dbxparity|portparity`: their `argparse` command lines in full ([`argp`]: help, usage errors at the
//!   console's width or `COLUMNS` [`crate::argfmt`]'s texts, round RT2) and their runs (round PR1b-1d, nothing
//!   handed to the Python): [`dbxparity`] (the field-by-field comparison and `--predict`) and [`portparity`]
//!   (meshport's conversions against DICE's own), over BF1943's source (`crate::bf1943src`) and the game copy's
//!   index (`bc2_convert::meshview::Index`), their reports byte for byte the Python's.

mod argp;
mod dbxparity;
mod portparity;

use std::fmt::Write as _;

use bc2_pyrt::pickle::{self, Value};
use bc2_pyrt::py::join;

use crate::gameindex;
use crate::pyos::files as fsx;
use crate::pyproc::{Fail, Py, PyErr, PyResult, Tool};

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

/// A parity tool's parser: its texts.
struct Parser {
    prog: &'static str,
    usage: &'static str,
    help: &'static str,
    predict: bool,
    default_o: &'static str,
}

const DBXPARITY: Parser = Parser {
    prog: "dbxparity.py",
    usage: "usage: dbxparity.py [-h] [-o O] [--predict] [names ...]\n",
    help: "usage: dbxparity.py [-h] [-o O] [--predict] [names ...]\n\ndbxparity.py - BF1943's data partitions against \
           DICE's own versions, field by\nfield: the ground truth for dbxport.\n\npositional arguments:\n  names\n\n\
           options:\n  -h, --help  show this help message and exit\n  -o O\n  --predict   score dbxport's value passes \
           and DICE's conventions\n",
    predict: true,
    default_o: "dbxparity.txt",
};

const PORTPARITY: Parser = Parser {
    prog: "portparity.py",
    usage: "usage: portparity.py [-h] [-o O] [names ...]\n",
    help: "usage: portparity.py [-h] [-o O] [names ...]\n\nportparity.py - how close our Battlefield 1943 conversions \
           come to DICE's own.\nBFBC2 ships PC versions of many\n\npositional arguments:\n  names\n\noptions:\n  -h, \
           --help  show this help message and exit\n  -o O\n",
    predict: false,
    default_o: "portparity.txt",
};

/// A parsed command line: the name filters, the report path and `--predict`.
pub struct Run {
    pub names: Vec<String>,
    pub o: String,
    pub predict: bool,
}

fn dbxparity(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    match parse(py, &DBXPARITY, argv)? {
        Ok(run) => dbxparity::run(py, &run),
        Err(code) => Ok(code),
    }
}

fn portparity(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    match parse(py, &PORTPARITY, argv)? {
        Ok(run) => portparity::run(py, &run),
        Err(code) => Ok(code),
    }
}

/// `ap.parse_args(argv)`: the values, or the exit code of the help (0) or a usage error (2), printed here.
fn parse(py: &mut Py, p: &Parser, argv: &[String]) -> PyResult<Result<Run, i32>> {
    match argp::parse(argv, p.predict, p.default_o) {
        argp::Outcome::Run(a) => Ok(Ok(Run {
            names: a.names,
            o: a.o,
            predict: a.predict,
        })),
        argp::Outcome::Help => {
            py.write(&crate::argfmt::help_or(p.prog, p.help)?)?;
            Ok(Err(0))
        }
        argp::Outcome::Error(m) => error(py, p, &m).map(Err),
    }
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

/// `bf1943import.Source(bf1943import.default_source())` and `meshview.Index(shaderdb.find_opfor(None))`, as both
/// tools open them (the source first: its error is the run's when both are missing); the OPFOR folder too.
fn open(py: &mut Py) -> PyResult<(crate::bf1943src::Source, bc2_convert::meshview::Index, String)> {
    let suite = bc2_pyrt::py::suite_root().to_string_lossy().into_owned();
    let folder = crate::bf1943src::default_source(&suite);
    let src = crate::bf1943src::Source::open_reading(&folder, &suite).map_err(|e| match e {
        // uncaught here: the traceback names the module's class
        Fail::Err(mut e) if e.kind == "Bf1943ImportError" => {
            "bf1943import.Bf1943ImportError".clone_into(&mut e.kind);
            Fail::Err(e)
        }
        other => other,
    })?;
    let root = gameindex::find_opfor(None)?;
    let ix = gameindex::load_full(py, &root)?;
    Ok((src, ix, root))
}

/// `open(path, 'w', encoding=...)` and the text written: `\n` as `\r\n`, UTF-8 (`utf8`) or the ANSI code page
/// (Python's default `open()`; a character the page lacks is its `UnicodeEncodeError`). Written whole through
/// `atomicio` (`bc2_gamefs::atomic`).
fn write_report(path: &str, text: &str, utf8: bool) -> PyResult<()> {
    let text = text.replace('\n', "\r\n");
    let bytes = if utf8 {
        text.into_bytes()
    } else {
        crate::pystdio::encode_ansi(&text).map_err(|m| Fail::Err(PyErr::new("UnicodeEncodeError", m)))?
    };
    crate::pyos::atomic_write(path, &bytes)
}

/// `str(dict(...))` of (repr of key, str of value) pairs.
fn py_dict(items: impl Iterator<Item = (String, String)>) -> String {
    let parts: Vec<String> = items.map(|(k, v)| format!("{k}: {v}")).collect();
    format!("{{{}}}", parts.join(", "))
}

impl bc2_convert::meshport::Source for crate::bf1943src::Source {
    fn entry(&self, key: &str) -> Option<(&str, &str)> {
        crate::bf1943src::Source::entry(self, key).map(|(_, e)| (e.kind.as_str(), e.name.as_str()))
    }

    fn data(&self, key: &str) -> Option<Vec<u8>> {
        crate::bf1943src::Source::data(self, key)
    }

    fn keys(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }
}
