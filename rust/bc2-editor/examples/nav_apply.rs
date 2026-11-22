//! The AI workspace's Build with these edits, in process, from the command line (round NK6's proof): runs
//! [`bc2_editor::nav::start_apply`] - the job the Build button starts, on its worker thread - and prints its lines as
//! they come, so a script can compare them and the files with `bfbc2.exe navkit apply`'s.
//!
//! ```text
//! cargo run --release -p bc2-editor --example nav_apply -- <opfor> <level> <side file> <out> [--cancel-after <s>]
//! ```
//!
//! Exit code 0 when the build ended well, 1 when it failed (its last line says why), 2 for a usage error.

#![allow(clippy::print_stdout, clippy::print_stderr, reason = "a proof's command line prints its lines")]

use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, lv, side, out, rest @ ..] = args.as_slice() else {
        eprintln!("usage: nav_apply <opfor> <level> <side file> <out> [--cancel-after <s>]");
        std::process::exit(2);
    };
    let cancel_after = match rest {
        [flag, s] if flag == "--cancel-after" => s.parse::<f64>().ok().map(Duration::from_secs_f64),
        _ => None,
    };
    let mut job = bc2_editor::nav::start_apply(
        &PathBuf::from(root),
        lv,
        &PathBuf::from(side),
        &PathBuf::from(out),
    );
    let t0 = Instant::now();
    let mut shown = 0;
    let mut asked = false;
    let ok = loop {
        let done = job.poll();
        let lines = job.output();
        for l in lines.get(shown..).unwrap_or_default() {
            println!("{l}");
        }
        shown = lines.len();
        if let Some(ok) = done {
            break ok;
        }
        if !asked && cancel_after.is_some_and(|c| t0.elapsed() >= c) {
            job.cancel();
            asked = true;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    for l in job.output().get(shown..).unwrap_or_default() {
        println!("{l}");
    }
    std::process::exit(i32::from(!ok));
}
