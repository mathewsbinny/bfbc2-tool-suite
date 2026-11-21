//! Round OP1 part B: the journalled autosave survives its writer being killed mid-write. A child process (this test
//! binary again, `BC2_JOURNAL_KILL_CHILD=<folder>`) appends operations with large labels to a project's journal as
//! fast as it can - three times through the editor's writer, three times each line in flushed 1 KB pieces so the kill
//! lands inside a line - and the parent kills it at six different sizes. Each time the journal must read back to the
//! operations 1 ... k in order with nothing guessed (a cut-short last line left out), resume after its last good line,
//! and read back with the operations appended after the kill.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bc2_levelwrite::journal::{self, Entry, Journal};
use bc2_levelwrite::project::autosave_path;
use bc2_levelwrite::{MapProject, Op, Profile};

const CHILD: &str = "BC2_JOURNAL_KILL_CHILD";

fn project() -> MapProject {
    MapProject::new("Killed", "test", "levels/TEST", b"base", Profile::Bfbc2)
}

/// The next operation pushed, as the editor pushes one (its number the project's next), with a label of `size` bytes.
fn push(p: &mut MapProject, size: usize) -> Entry {
    let label = format!("{}{}", p.next, "x".repeat(size));
    p.push(
        Op::Delete {
            guids: vec![format!("G{}", p.next)],
        },
        &label,
    );
    Entry::Push {
        op: p.operations.last().unwrap().clone(),
    }
}

/// The child: appends until it is killed (never returns while the parent waits).
#[test]
fn journal_writer_child() {
    let _gate = crate::gate();
    let Some(dir) = std::env::var_os(CHILD) else {
        println!("skipped: the child of journal_survives_its_writer_killed (runs only under {CHILD})");
        return;
    };
    let dir = PathBuf::from(dir);
    let path = dir.join("Killed.mapedit.json");
    let mut p = project();
    let mut j = Journal::start(&autosave_path(&path), &p).unwrap();
    if !dir.to_string_lossy().ends_with("_chunked") {
        // the editor's own writer: a line per call
        loop {
            let e = push(&mut p, 20_000);
            j.append(&e).unwrap();
        }
    }
    // each line written in 1 KB pieces, each flushed: a kill lands inside a line (as a power cut can leave one)
    drop(j);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(autosave_path(&path))
        .unwrap();
    loop {
        let l = journal::line(&push(&mut p, 20_000)).unwrap();
        for piece in l.chunks(1024) {
            std::io::Write::write_all(&mut f, piece).unwrap();
            std::io::Write::flush(&mut f).unwrap();
        }
    }
}

fn check(auto: &Path, at_least: u64) -> usize {
    let r = journal::read(auto).unwrap();
    let p = r
        .project
        .expect("the first line is always whole (written before the child appends)");
    // the operations 1 ... k, each the child's, nothing else
    for (k, o) in p.operations.iter().enumerate() {
        assert_eq!(o.n, k as u64 + 1);
        assert!(o.label.starts_with(&o.n.to_string()));
        assert_eq!(o.label.len(), o.n.to_string().len() + 20_000);
    }
    let size = std::fs::metadata(auto).unwrap().len();
    assert!(size >= at_least);
    println!(
        "killed at {size} bytes: {} operations recovered, {} bytes of a cut-short line left out",
        p.operations.len(),
        size - r.good
    );
    p.operations.len()
}

#[test]
fn journal_survives_its_writer_killed() {
    let _gate = crate::gate();
    if std::env::var_os(CHILD).is_some() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let mut torn = 0;
    for (round, kill_at) in [200_000u64, 1_500_000, 4_000_000, 300_000, 2_000_000, 5_000_000]
        .into_iter()
        .enumerate()
    {
        let how = if round < 3 { "chunked" } else { "lines" };
        let dir = std::env::temp_dir().join(format!("bc2_levelwrite_op1_kill_{}_{round}_{how}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let auto = autosave_path(&dir.join("Killed.mapedit.json"));
        let mut child = std::process::Command::new(&exe)
            .args([
                "op1_journal::journal_writer_child",
                "--exact",
                "--nocapture",
                "--test-threads",
                "1",
            ])
            .env(CHILD, &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let t = Instant::now();
        while std::fs::metadata(&auto).map_or(0, |m| m.len()) < kill_at {
            assert!(t.elapsed() < Duration::from_secs(60), "the child wrote too slowly");
            assert!(child.try_wait().unwrap().is_none(), "the child stopped by itself");
            std::thread::sleep(Duration::from_millis(2));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let k = check(&auto, kill_at);
        assert!(k > 0);
        // resumed: the torn end cut off, three more operations after the last good line
        torn += usize::from(journal::read(&auto).unwrap().torn);
        let (mut j, mut p, r) = Journal::resume(&auto).unwrap().unwrap();
        assert_eq!(p.operations.len(), k);
        for _ in 0..3 {
            let e = push(&mut p, 20_000);
            j.append(&e).unwrap();
        }
        drop(j);
        let again = journal::read(&auto).unwrap();
        assert!(!again.torn, "resumed after the last good line");
        assert_eq!(again.lines, r.lines + 3);
        assert_eq!(check(&auto, 0), k + 3);
        // and the editor's recovery offer reads it (no saved project: it differs)
        assert_eq!(
            MapProject::read_autosave(&dir.join("Killed.mapedit.json"))
                .unwrap()
                .operations
                .len(),
            k + 3
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    println!("{torn} of 6 kills left a cut-short line");
}
