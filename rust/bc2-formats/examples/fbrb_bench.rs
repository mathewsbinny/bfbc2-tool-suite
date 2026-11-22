//! Times the Rust bundle reader on folders of `.fbrb` files (rust/BENCHMARKS.md):
//!
//! ```text
//! cargo run --release -p bc2-formats --example fbrb_bench -- <folder> [<folder> ...]
//! ```
//!
//! Prints the best of three runs of reading every bundle's header (`entries`) and of extracting every entry of every
//! bundle (`extract`: decompressed, held in memory, then dropped), on one thread and on every core. The files are
//! read from disk each time (the second and later runs from the system's file cache).
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use bc2_formats::fbrb;
use rayon::prelude::*;

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

fn best<F: FnMut() -> (usize, u64)>(mut f: F) -> (f64, usize, u64) {
    let mut best = f64::MAX;
    let mut n = (0, 0);
    for _ in 0..3 {
        let t = Instant::now();
        n = f();
        best = best.min(t.elapsed().as_secs_f64());
    }
    (best, n.0, n.1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut files = Vec::new();
    for a in args.iter().filter(|a| !a.starts_with("--")) {
        walk(Path::new(a), &mut files);
    }
    let bytes: u64 = files
        .iter()
        .filter_map(|f| std::fs::metadata(f).ok())
        .map(|m| m.len())
        .sum();
    println!(
        "{} bundles, {:.1} MB on disk, {} threads",
        files.len(),
        bytes as f64 / 1e6,
        bc2_formats::threads()
    );
    let one = rayon::ThreadPoolBuilder::new().num_threads(1).build().expect("a pool");
    let headers = || {
        let n = files
            .par_iter()
            .filter_map(|f| fbrb::entries_path(f).ok())
            .map(|h| h.entries.len())
            .sum();
        (n, 0)
    };
    let extract = || {
        files
            .par_iter()
            .filter_map(|f| fbrb::extract_path_uncached(f).ok())
            .map(|(h, d)| (h.entries.len(), d.iter().map(|x| x.len() as u64).sum::<u64>()))
            .reduce(|| (0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
    };
    let (t, n, _) = best(|| one.install(headers));
    println!("entries  1 thread : {t:7.3} s  ({n} entries)");
    let (t, n, _) = best(headers);
    println!("entries  parallel : {t:7.3} s  ({n} entries)");
    if args.iter().any(|a| a == "--headers-only") {
        return;
    }
    let (t, n, b) = best(|| one.install(extract));
    println!("extract  1 thread : {t:7.3} s  ({n} entries, {:.1} MB)", b as f64 / 1e6);
    let (t, n, b) = best(extract);
    println!("extract  parallel : {t:7.3} s  ({n} entries, {:.1} MB)", b as f64 / 1e6);
}
