//! `bfbc2 xdiff [--db wake] [--ps N ...] [--vs [N ...]] [--n N] [--seed S] [-o out] ...`:
//! `bfbc2-shader-tools\xdiff.py`'s command line up to its run - the `argparse` parser and, for a BF1943 database (any
//! `--db` but `bfbc2:<level>`), no BF1943 tool suite to read BF1943's bundles with (`bf1943import.fbrb1943`'s
//! `Bf1943ImportError`), an uncaught exception as the script has no guard. (Before round XD2 `--db` took only `wake`:
//! another name was its `DBS` table's `KeyError`.)
//!
//! A run is handed to the Python, and so is a BFBC2 Xbox level's database (`bfbc2:<level>`, read from Project OPFOR's
//! unpacked Xbox copy): each pixel or vertex shader runs through `xenos_exec.py` (the Xbox 360 microcode interpreter,
//! NumPy, kept Python by round E3f's measurement) and through its translation drawn by `shaderview.exe` on WARP, the
//! inputs drawn from NumPy's seeded generator; the result is NumPy's comparison of the two.

use super::argp::{self, Kind, Opt, Parsed, Parser, Ty};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyErr, PyResult, suite_root};

const USAGE: &str = "usage: xdiff.py [-h] [--db DB] [--ps [PS ...]] [--vs [VS ...]] [--n N]
                [--seed SEED] [--limit LIMIT] [--bisect] [-o O]
                [--workers WORKERS]
";

const HELP_BODY: &str = "
options:
  -h, --help         show this help message and exit
  --db DB
  --ps [PS ...]
  --vs [VS ...]      vertex shaders (no numbers: every one)
  --n N
  --seed SEED
  --limit LIMIT
  --bisect           find the first differing instruction (one --ps)
  -o O
  --workers WORKERS  shaders at a time (default BFBC2_XDIFF_WORKERS, else up
                     to 4)
";

const fn o(strings: &'static [&'static str], kind: Kind, ty: Ty) -> Opt {
    Opt {
        strings,
        kind,
        ty,
        choices: &[],
        required: false,
    }
}

const OPTS: &[Opt] = &[
    o(&["--db"], Kind::Store, Ty::Str),
    o(&["--ps"], Kind::Star, Ty::Int),
    o(&["--vs"], Kind::Star, Ty::Int),
    o(&["--n"], Kind::Store, Ty::Int),
    o(&["--seed"], Kind::Store, Ty::Int),
    o(&["--limit"], Kind::Store, Ty::Int),
    o(&["--bisect"], Kind::Flag, Ty::Str),
    o(&["-o"], Kind::Store, Ty::Str),
    o(&["--workers"], Kind::Store, Ty::Int),
];

/// `xdiff.main(argv)` (no `errors.setup`: an exception is printed by the interpreter, exit code 1).
pub fn run(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    let help: &'static str = Box::leak(format!("{USAGE}{HELP_BODY}").into_boxed_str());
    let p = Parser {
        prog: "xdiff.py",
        usage: USAGE,
        help,
        opts: OPTS,
        pos: &[],
        order: &[Err(0), Err(1), Err(2), Err(3), Err(4), Err(5), Err(6), Err(7), Err(8)],
    };
    let a = match argp::parse(py, &p, argv, false, &mut Vec::new())? {
        Parsed::Args(a) => a,
        Parsed::Exit(c) => return Ok(c),
    };
    let a = match argp::finish(py, &p, a)? {
        Parsed::Args(a) => a,
        Parsed::Exit(c) => return Ok(c),
    };
    // load_db(a.db): a BFBC2 Xbox level's database is the Python's to read (shaderdb.find_opfor, shaderdb.load)
    let db = a.one("--db").unwrap_or("wake");
    if db.to_lowercase().starts_with("bfbc2:") {
        return Err(Fail::Python);
    }
    // bf1943_entry: bf1943import.default_source(), then fbrb1943(): the BF1943 tool suite's bundle reader beside the
    // suite, else BF1943_SUITE_FBRB
    let suite = pyos::abspath(&suite_root().to_string_lossy())?;
    let here = pyos::join(&pyos::dirname(&suite)?, &["bf1943-tool-suite", "bf1943-fbrb-tool"])?;
    let there = std::env::var("BF1943_SUITE_FBRB").unwrap_or_default();
    for c in [here.as_str(), there.as_str()] {
        if !c.is_empty() && pyos::isfile(&pyos::join2(c, "fbrb1943.py")?) {
            return Err(Fail::Python); // BF1943's bundles, then the run
        }
    }
    Err(Fail::Err(PyErr::new(
        "bf1943import.Bf1943ImportError",
        "the BF1943 tool suite (bf1943-tool-suite\\bf1943-fbrb-tool\\fbrb1943.py) was not found next to this suite: \
         its bundle reader reads the Xbox 360 bundles",
    )))
}
