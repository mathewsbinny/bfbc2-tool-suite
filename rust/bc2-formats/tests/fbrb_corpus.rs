//! The Rust bundle reader on the game's own bundles (run by `run_tests.py game`; ignored by a plain `cargo test`):
//!
//! ```text
//! cargo test --release -p bc2-formats --test fbrb_corpus -- --ignored --nocapture
//! ```
//!
//! Every `.fbrb` under Project OPFOR BFBC2's `game` folder (read only) and `work\ww2` is opened and every entry
//! extracted without an error or a panic; each entry's data has its header's size. The folder is `OPFOR_ROOT`,
//! else `project-opfor-bfbc2` beside the suite; the test passes with a note when neither has the bundles. The full
//! parity with the Python reader (the same entries and bytes) is `core/tests/test_fbrb_native.py --game`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use bc2_formats::fbrb;
use rayon::prelude::*;

fn opfor() -> Option<PathBuf> {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let candidates = [
        std::env::var_os("OPFOR_ROOT")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from),
        Some(suite.join("..").join("project-opfor-bfbc2")),
    ];
    candidates.into_iter().flatten().find(|p| p.join("game").is_dir())
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fbrb")) {
            out.push(p);
        }
    }
}

#[test]
#[ignore = "reads every bundle of the game copy (about 11 GB); run_tests.py game runs it"]
fn every_bundle_extracts() {
    let Some(root) = opfor() else {
        println!("skipped: no Project OPFOR BFBC2 game folder (OPFOR_ROOT or beside the suite)");
        return;
    };
    let mut files = Vec::new();
    walk(&root.join("game"), &mut files);
    walk(&root.join("work").join("ww2"), &mut files);
    assert!(files.len() > 1000, "only {} bundles found", files.len());
    let (entries, bytes) = (AtomicU64::new(0), AtomicU64::new(0));
    // four at a time keeps the memory in hand (each bundle is held whole while it is checked: up to 600 MB)
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    let bad: Vec<String> = pool.install(|| {
        files
            .par_iter()
            .filter_map(|f| match fbrb::extract_path(f) {
                Ok((h, data)) => {
                    entries.fetch_add(h.entries.len() as u64, Ordering::Relaxed);
                    let mut wrong = 0;
                    for (e, d) in h.entries.iter().zip(&data) {
                        bytes.fetch_add(d.len() as u64, Ordering::Relaxed);
                        wrong += usize::from(!e.is_deleted() && d.len() as u64 != u64::from(e.size));
                    }
                    (wrong > 0).then(|| format!("{}: {wrong} entries of the wrong size", f.display()))
                }
                Err(e) => Some(format!("{}: {e}", f.display())),
            })
            .collect()
    });
    for b in bad.iter().take(20) {
        println!("  {b}");
    }
    println!(
        "{} bundles, {} entries, {:.2} GB of entry data, {} refused",
        files.len(),
        entries.load(Ordering::Relaxed),
        bytes.load(Ordering::Relaxed) as f64 / 1e9,
        bad.len()
    );
    assert_eq!(bad, Vec::<String>::new());
}
