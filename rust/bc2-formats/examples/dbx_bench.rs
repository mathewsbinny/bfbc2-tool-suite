//! Times the Rust DBX reader on folders of `.dbx` files, the files read into memory first (rust/BENCHMARKS.md):
//!
//! ```text
//! cargo run --release -p bc2-formats --example dbx_bench -- [--distinct] <folder> [<folder> ...]
//! ```
//!
//! Prints the best of three runs for `parse` and `to_xml`, on one thread and on every core. `--distinct` keeps one
//! copy of files with the same bytes (the corpus has each shared partition once per level).
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    missing_docs
)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bc2_formats::dbx;

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

fn best<F: FnMut() -> usize>(mut f: F) -> (f64, usize) {
    let mut best = f64::MAX;
    let mut n = 0;
    for _ in 0..3 {
        let t = Instant::now();
        n = f();
        best = best.min(t.elapsed().as_secs_f64());
    }
    (best, n)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let distinct = args.iter().any(|a| a == "--distinct");
    let mut files = Vec::new();
    for a in args.iter().filter(|a| !a.starts_with("--")) {
        walk(Path::new(a), &mut files);
    }
    let mut seen = HashSet::new();
    let data: Vec<Vec<u8>> = files
        .iter()
        .map(|f| std::fs::read(f).expect("a file"))
        .filter(|d| !distinct || seen.insert(d.clone()))
        .collect();
    let bytes: usize = data.iter().map(Vec::len).sum();
    println!(
        "{} files, {:.1} MB, {} threads",
        data.len(),
        bytes as f64 / 1e6,
        bc2_formats::threads()
    );
    let one = rayon::ThreadPoolBuilder::new().num_threads(1).build().expect("a pool");
    let count_ok = |r: &[dbx::Result<dbx::DbxFile>]| r.iter().filter(|x| x.is_ok()).count();
    let (t, n) = best(|| one.install(|| count_ok(&dbx::parse_all(&data))));
    println!("parse   1 thread : {t:7.3} s  ({n} parsed)");
    let (t, n) = best(|| count_ok(&dbx::parse_all(&data)));
    println!("parse   parallel : {t:7.3} s  ({n} parsed)");
    let (t, n) = best(|| one.install(|| dbx::to_xml_all(&data).iter().filter(|x| x.is_ok()).count()));
    println!("to_xml  1 thread : {t:7.3} s  ({n} written)");
    let (t, n) = best(|| dbx::to_xml_all(&data).iter().filter(|x| x.is_ok()).count());
    println!("to_xml  parallel : {t:7.3} s  ({n} written)");
}
