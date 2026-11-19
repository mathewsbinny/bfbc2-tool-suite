//! `bfbc2`: the suite's command lines in Rust (round O1, `rust/BENCHMARKS.md` "O1"), one binary with the tools as
//! subcommands. Each subcommand is its Python command line's port: the same arguments, files, printed bytes (stdout
//! and stderr, in the order and code page Python writes them), exit codes and log lines.
//!
//! ```text
//! bfbc2 fbrbtool unpack|pack|list|big ...     python bfbc2-fbrb-tool\fbrbtool.py ...
//! bfbc2 dbxedit xml|edit|import|check|roundtrip|fixslots ...
//!                                             python bfbc2-dbx-tools\dbxedit.py ...
//! bfbc2 aptcheck|aptrefs|bc2loc|exeref|crashreader|hpdtool|hkpack|convert|aimanim ...
//!                                             round O2: python <the tool's .py> ... (src/pyproc.rs)
//! bfbc2 caches|stockmp|aptgraft|audio|dbxcatalogue|dbxparity|portparity|meshview|culltree ...
//!                                             round O3, the same way
//! bfbc2 --source-hash                        the Rust sources' fingerprint it was built from (bfbc2core.native)
//! ```
//!
//! Without a subcommand (or with an unknown one) it prints its usage to stderr and exits 2.

#![deny(unsafe_code)]

#[cfg(feature = "audio")]
mod audio;
#[cfg(feature = "bf1943")]
mod bf1943;
// round O3: the caches' overall budget after a bundle read (fbrb._budget), for the ports that read bundles
#[cfg(feature = "graft")]
mod budget;
#[cfg(feature = "caches")]
mod caches;
mod clock;
#[cfg(feature = "crash")]
mod crash;
#[cfg(feature = "cull")]
mod cull;
// round O3: os / atomicio as the Python sees them, and the log folder's pruning (every guarded tool prunes it)
#[cfg_attr(
    not(all(
        feature = "menu",
        feature = "crash",
        feature = "nav",
        feature = "caches",
        feature = "stockmp",
        feature = "graft",
        feature = "audio",
        feature = "bf1943",
        feature = "meshview",
        feature = "cull"
    )),
    allow(dead_code)
)]
mod fsx;
#[cfg(feature = "graft")]
mod graft;
#[cfg_attr(
    not(all(
        feature = "menu",
        feature = "crash",
        feature = "nav",
        feature = "caches",
        feature = "stockmp",
        feature = "graft",
        feature = "audio",
        feature = "bf1943",
        feature = "meshview",
        feature = "cull"
    )),
    allow(dead_code)
)]
mod logs;
#[cfg(feature = "menu")]
mod menu;
#[cfg(feature = "meshview")]
mod meshview;
#[cfg(feature = "nav")]
mod nav;
mod pylog;
// the helpers the groups share: some go unused when a group is built alone (--no-default-features --features ..)
#[cfg_attr(
    not(all(
        feature = "menu",
        feature = "crash",
        feature = "nav",
        feature = "caches",
        feature = "stockmp",
        feature = "graft",
        feature = "audio",
        feature = "bf1943",
        feature = "meshview",
        feature = "cull"
    )),
    allow(dead_code)
)]
mod pyproc;
mod pystdio;
#[cfg(feature = "stockmp")]
mod stockmp;

// round O3: Python's exceptions, value texts and ntpath live in the library crate bc2-pyrt (shared with the tools'
// libraries: bc2-menu ...)
#[cfg_attr(
    not(all(
        feature = "menu",
        feature = "crash",
        feature = "nav",
        feature = "caches",
        feature = "stockmp",
        feature = "graft",
        feature = "audio",
        feature = "bf1943",
        feature = "meshview",
        feature = "cull"
    )),
    allow(unused_imports)
)]
use bc2_pyrt::fmt as pyfmt;

use std::process::ExitCode;

use bc2_formats::{dbxedit, fbrb};

/// The fingerprint of the Rust sources this binary was built from (`build_native.py` passes it as it does to the
/// module; empty when built by cargo alone).
const SOURCE_HASH: &str = match option_env!("BC2_SOURCE_HASH") {
    Some(h) => h,
    None => "",
};

const USAGE: &str = "usage: bfbc2 <tool> <arguments>\n  \
     fbrbtool unpack|pack|list|big ...   FbrbTool's commands (bfbc2-fbrb-tool\\fbrbtool.py)\n  \
     dbxedit xml|edit|import|check|roundtrip|fixslots ...   DbxEdit's commands (bfbc2-dbx-tools\\dbxedit.py)\n  \
     aptcheck, aptrefs, bc2loc ...   the menu tools (bfbc2-menu-tools\\<tool>.py)\n  \
     exeref, crashreader ...   the crash reader's tools (bfbc2-automated-map-and-bot-test-harness\\crash-reader)\n  \
     hpdtool, hkpack, convert, aimanim ...   NavKit's hpdtool / hkpack, the console converter, aim poses\n  \
     caches, stockmp, aptgraft, audio, dbxcatalogue, dbxparity, portparity, meshview, culltree ...   round O3\n  \
     --source-hash   the Rust sources' fingerprint this binary was built from\n";

/// What one tool's run gave: exit code, stdout, stderr (lines ended by `\n`) and its log lines.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
    log: Vec<(String, String)>,
}

/// A tool's command line run in process: the arguments after the tool's name, and its log file.
type Runner = fn(&[String], Option<&str>) -> Run;

/// A tool of the `bfbc2-*` folders whose command line is ported: its log name and its runner.
fn tool(name: &str) -> Option<(&'static str, Runner)> {
    match name {
        "fbrbtool" => Some(("fbrbtool", |a, l| {
            let o = fbrb::tool::run(a, l);
            Run {
                code: o.code,
                stdout: o.stdout,
                stderr: o.stderr,
                log: o.log,
            }
        })),
        "dbxedit" => Some(("dbxedit", |a, l| {
            let o = dbxedit::run(a, l);
            Run {
                code: o.code,
                stdout: o.stdout,
                stderr: o.stderr,
                log: o.log,
            }
        })),
        _ => None,
    }
}

/// Round O2's command lines (`pyproc`: each runs as its Python process would, and hands off to it where the Rust
/// core refuses).
fn py_tool(name: &str) -> Option<&'static pyproc::Tool> {
    let groups: &[&[pyproc::Tool]] = &[
        #[cfg(feature = "menu")]
        menu::TOOLS,
        #[cfg(feature = "crash")]
        crash::TOOLS,
        #[cfg(feature = "nav")]
        nav::TOOLS,
        // round O3
        #[cfg(feature = "caches")]
        caches::TOOLS,
        #[cfg(feature = "stockmp")]
        stockmp::TOOLS,
        #[cfg(feature = "graft")]
        graft::TOOLS,
        #[cfg(feature = "audio")]
        audio::TOOLS,
        #[cfg(feature = "bf1943")]
        bf1943::TOOLS,
        #[cfg(feature = "meshview")]
        meshview::TOOLS,
        #[cfg(feature = "cull")]
        cull::TOOLS,
    ];
    groups.iter().flat_map(|g| g.iter()).find(|t| t.name == name)
}

fn exit(code: i32) -> ExitCode {
    // the tools' codes are 0..=3; anything else is clamped as a process exit code would truncate it
    ExitCode::from(u8::try_from(code & 0xFF).unwrap_or(1))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let Some((first, rest)) = args.split_first() else {
        pystdio::emit("", USAGE);
        return exit(2);
    };
    if first == "--source-hash" && rest.is_empty() {
        pystdio::emit(&format!("{SOURCE_HASH}\n"), "");
        return exit(0);
    }
    if let Some(t) = py_tool(first) {
        return pyproc::run(t, rest);
    }
    let Some((log_name, runner)) = tool(first) else {
        pystdio::emit("", &format!("bfbc2: unknown tool {first:?}\n{USAGE}"));
        return exit(2);
    };
    // as bfbc2core.dbxedit / fbrbtool.run: the day's log file first, the command, then its lines appended
    let log = pylog::log_path(log_name);
    let r = runner(rest, log.as_deref());
    pylog::append(log.as_deref(), log_name, rest, &r.log);
    exit(pystdio::emit_or_fail(&r.stdout, &r.stderr).unwrap_or(r.code))
}
