//! The map editor's integration tests in one test binary, `editor` (round QW1, audit AU2 P5): each file of `tests/` is a
//! module here (`mod g2;` is `tests/g2.rs`), so the crate and its dependencies are linked into one
//! executable instead of one per file. A test's name is `<file>::<function>`:
//!
//! ```text
//! cargo test -p bc2-editor --test editor                       every test of the crate's files
//! cargo test -p bc2-editor --test editor g2::                one file's tests (what `--test g2` was)
//! cargo test -p bc2-editor --test editor g2::<function> -- --exact   one test
//! ```
//!
//! The files' tests used to run in separate processes, one file after another; here they share one process, so
//! each test first takes `gate()`: at most [`GATE`] of them run at once (the real-level tests hold whole levels,
//! and a PC with 15 GB must not hold all of them together). A new file of `tests/` is a new `mod` line below.
//!
//! **The hang guard** (round HG1): a test that holds its place longer than `BC2_TEST_HANG_S` seconds (default
//! [`HANG_S`]) ends the binary with the names of the tests still running and how long each has run, instead of the
//! runner's timeout naming nothing. With `BC2_TEST_HANG_STACKS=1` it first prints where each thread waits
//! (`rust\hang_stacks.py`: a minidump of the binary, each thread's stack named from the PDBs; at most two minutes).

use std::sync::{Condvar, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

mod asset_browser;
mod df1;
mod e2b;
mod editing;
mod every_level;
mod ex1;
mod factions;
mod g2;
mod g3;
mod g4;
mod game_view;
mod game_view_live;
mod game_view_package;
mod game_view_r5d;
mod game_view_ww2;
mod gr1;
mod gr2;
mod gr3;
mod inspect_parity;
mod inspector_parity;
mod nav_parity;
mod real_level_editing;
mod render;
mod simplified_view;
mod view_geometry;
mod view_toggles;
mod vw2;
mod xf1;

/// At most this many of the binary's tests run at the same time.
const GATE: usize = 2;

static RUNNING: Mutex<usize> = Mutex::new(0);
static FREED: Condvar = Condvar::new();

/// The default of `BC2_TEST_HANG_S`: no test of the binary holds its place this long unless it is stuck.
const HANG_S: u64 = 1200;

/// The next [`Gate`]'s number.
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The tests holding a place: (a number of their own, the test's name, since when).
static HOLDING: Mutex<Vec<(u64, String, Instant)>> = Mutex::new(Vec::new());

/// A test's place among the [`GATE`] that may run at once, given back when it is dropped (a failing test's too).
pub(crate) struct Gate(u64);

/// The hang guard's thread (see the module), started by the first `gate()`.
fn watch() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let limit = std::env::var("BC2_TEST_HANG_S")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(HANG_S);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let held: Vec<(String, u64)> = HOLDING
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .iter()
                    .map(|(_, n, t)| (n.clone(), t.elapsed().as_secs()))
                    .collect();
                if held.iter().all(|h| h.1 < limit) {
                    continue;
                }
                eprintln!("hang guard: a test ran over {limit} s (BC2_TEST_HANG_S); running now: {held:?}");
                if std::env::var("BC2_TEST_HANG_STACKS").is_ok_and(|v| v == "1") {
                    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("..")
                        .join("hang_stacks.py");
                    if let Ok(mut child) = std::process::Command::new("python")
                        .arg(script)
                        .arg(std::process::id().to_string())
                        .spawn()
                    {
                        let t = Instant::now();
                        while matches!(child.try_wait(), Ok(None)) && t.elapsed() < Duration::from_mins(2) {
                            std::thread::sleep(Duration::from_millis(200));
                        }
                        let _ = child.kill();
                    }
                }
                std::process::exit(101);
            }
        });
    });
}

/// Waits until fewer than [`GATE`] tests run, then holds a place: every test's first statement.
pub(crate) fn gate() -> Gate {
    // the binary's tests share each level's read (round TS1): MP_005 is read once, not once per test
    bc2_editor::app::share_level_reads();
    let mut running = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
    while *running >= GATE {
        running = FREED.wait(running).unwrap_or_else(PoisonError::into_inner);
    }
    *running += 1;
    drop(running);
    watch();
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = std::thread::current().name().unwrap_or("?").to_owned();
    HOLDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((id, name, Instant::now()));
    Gate(id)
}

impl Drop for Gate {
    fn drop(&mut self) {
        HOLDING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|h| h.0 != self.0);
        let mut running = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
        *running = running.saturating_sub(1);
        FREED.notify_one();
    }
}

/// The level `lv` is on this PC: the game copy is found and lists it (`game\Dist`, or an installed `game\Package`
/// level). A test skips a level (printing "skipped") only when this is false: with the data there, a level that does
/// not open, or a view that does not start, is a failure (round TS1, audit AU3 TS-7: four helpers turned such errors
/// into skips that passed).
pub(crate) fn level_here(lv: &str) -> bool {
    bc2_editor::levels::find_opfor(None).is_ok_and(|root| {
        bc2_editor::levels::list_levels(&root)
            .iter()
            .any(|l| l.lv.eq_ignore_ascii_case(lv))
    })
}
