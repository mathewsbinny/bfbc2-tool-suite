//! Round O3's `bf1943` group: `bfbc2-bf1943-import`'s `dbxcatalogue.py`, `dbxparity.py` and `portparity.py` as
//! command lines.
//!
//! - `bfbc2 dbxcatalogue`: the catalogue's summary line, from its cache (`work\tools_out\dbx_catalogue.bin`,
//!   `bc2_convert::dbxcatalogue::cache`, round PR1b-1e: the Python's pickle retired) over the index's cache
//!   ([`crate::gameindex`]), or built here and cached when the game copy changed (only a build the Rust refuses is
//!   handed to the Python).
//! - `bfbc2 dbxparity|portparity`: their `argparse` command lines in full ([`argp`]: help, usage errors at the
//!   console's width or `COLUMNS` [`crate::argfmt`]'s texts, round RT2) and their runs (round PR1b-1d, nothing
//!   handed to the Python): [`dbxparity`] (the field-by-field comparison and `--predict`) and [`portparity`]
//!   (meshport's conversions against DICE's own), over BF1943's source (`crate::bf1943src`) and the game copy's
//!   index (`bc2_convert::meshview::Index`), their reports byte for byte the Python's.

mod argp;
mod dbxparity;
mod portparity;

use std::fmt::Write as _;
use std::path::Path;

use bc2_convert::dbxcatalogue::{Job, cache};
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

/// `dbxcatalogue._names(ix)`: the index's partitions (`.dbx` keys) by bundle, in the index's order, build overlays
/// left out.
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the index's keys are lower case; Python's endswith('.dbx')"
)]
fn catalogue_jobs<'a>(rows: impl Iterator<Item = (&'a str, &'a str, &'a str)>) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    let mut at: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (k, b, n) in rows {
        if !k.ends_with(".dbx") || b.to_lowercase().contains("build_overlay") {
            continue;
        }
        let i = *at.entry(b).or_insert_with(|| {
            jobs.push((b.to_owned(), Vec::new()));
            jobs.len() - 1
        });
        if let Some(j) = jobs.get_mut(i) {
            j.1.push(n.to_owned());
        }
    }
    jobs
}

/// `dbxcatalogue.load(ix)` over `jobs`: from its cache (`work\tools_out\dbx_catalogue.bin`, round PR1b-1e) when it
/// is this game copy's, else built and cached ([`cache::load`]; its lines on stderr, as the Python prints them).
/// `None`: the build refused (the Python's reference would run).
fn catalogue(py: &mut Py, jobs: &[Job], root: &str) -> Option<cache::Loaded> {
    let exe = join(root, &["game", "BFBC2Game.exe"]);
    let path = join(root, &["work", "tools_out", cache::FILE]);
    cache::load(jobs, Some(&exe), root, Some(Path::new(&path)), &mut |_, line| {
        py.eprint(line);
    })
}

/// `python dbxcatalogue.py`: `load(meshview.Index(shaderdb.find_opfor(None)))` and its counts (any arguments are
/// ignored, as the Python ignores them). A build the Rust refuses is handed to the Python (its reference).
fn dbxcatalogue(py: &mut Py, _argv: &[String]) -> PyResult<i32> {
    let t = fsx::now();
    let root = gameindex::find_opfor(None)?;
    let ix = gameindex::load(py, &root)?;
    let jobs = catalogue_jobs(ix.rows());
    let c = catalogue(py, &jobs, &root).ok_or(Fail::Python)?.catalogue;
    let line = format!(
        "{} partitions, {} classes, {} guids, {} paths, {} executable names ({:.0} s)",
        c.built.partitions,
        c.built.classes.len(),
        c.built.guids.len(),
        c.built.shapes.len(),
        c.exe_names.len(),
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
