//! Counts the allocations and the memory of parsed DBX trees held at once (rust/BENCHMARKS.md), the files read into
//! memory first:
//!
//! ```text
//! cargo run --release -p bc2-formats --example dbx_mem -- [--distinct] [--threads N] [--only-owned | --only-tree]
//!     <folder> [<folder> ...]
//! ```
//!
//! Every file is parsed (on N threads, default every core) and every tree kept, as owned trees (`parse_all`) and
//! as compact trees (`parse_tree_all`); printed for each are the time, the number of allocations, the bytes
//! allocated, and the bytes still held once all trees are built (the trees' memory), then the time to drop them.
//! The global allocator is `stats_alloc`'s counting wrapper of the system allocator.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    missing_docs
)]

use std::alloc::System;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bc2_formats::dbx;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dbx")) {
            out.push(p);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let distinct = args.iter().any(|a| a == "--distinct");
    let threads = args
        .iter()
        .position(|a| a == "--threads")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let mut files = Vec::new();
    for (i, a) in args.iter().enumerate() {
        if !a.starts_with("--") && (i == 0 || args.get(i - 1).is_none_or(|p| p != "--threads")) {
            walk(Path::new(a), &mut files);
        }
    }
    let mut seen = HashSet::new();
    let data: Vec<Vec<u8>> = files
        .iter()
        .map(|f| std::fs::read(f).expect("a file"))
        .filter(|d| dbx::is_binary(d) && (!distinct || seen.insert(d.clone())))
        .collect();
    drop(seen);
    let bytes: usize = data.iter().map(Vec::len).sum();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("a pool");
    println!(
        "{} binary files, {:.1} MB, {} threads",
        data.len(),
        bytes as f64 / 1e6,
        pool.current_num_threads()
    );
    let mode = |m: &str| !args.iter().any(|a| a.starts_with("--only-")) || args.iter().any(|a| a == m);
    if mode("--only-owned") {
        measure("owned trees (parse_all: DbxFile)", bytes, || {
            pool.install(|| dbx::parse_all(&data))
        });
    }
    if mode("--only-tree") {
        measure("compact trees (parse_tree_all: DbxTree)", bytes, || {
            pool.install(|| dbx::parse_tree_all(&data))
        });
    }
}

/// Builds every tree with `parse`, keeps them all, and prints the time, allocations and memory held.
fn measure<T, E>(label: &str, bytes: usize, parse: impl FnOnce() -> Vec<Result<T, E>>) {
    let region = Region::new(GLOBAL);
    let t = Instant::now();
    let trees = parse();
    let built = t.elapsed().as_secs_f64();
    let s = region.change();
    let held = s.bytes_allocated as i128 - s.bytes_deallocated as i128 + s.bytes_reallocated as i128;
    let ok = trees.iter().filter(|t| t.is_ok()).count();
    let t = Instant::now();
    drop(trees);
    let dropped = t.elapsed().as_secs_f64();
    println!(
        "{label}: {ok} parsed in {built:.3} s; {} allocations ({} reallocations), {:.1} MB allocated, \
         {:.1} MB held ({:.2}x the files); dropped in {dropped:.3} s",
        s.allocations,
        s.reallocations,
        s.bytes_allocated as f64 / 1e6,
        held as f64 / 1e6,
        held as f64 / bytes as f64
    );
}
