//! Times the Rust Granny reader on the `.res` files under folders (rust/BENCHMARKS.md, "E3i"):
//!
//! ```text
//! cargo run --release -p bc2-formats --example granny_bench -- <folder> [<folder> ...]
//! ```
//!
//! Every file that reads as a Granny resource: the tables (`Granny::parse`) and the object tree (`root`), one thread,
//! best of three; the files are in memory.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use bc2_formats::granny::{Granny, Value};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("res")) {
            out.push(p);
        }
    }
}

fn count(v: &Value<'_>) -> usize {
    match v {
        Value::List(l) => 1 + l.iter().map(count).sum::<usize>(),
        Value::Ints(n) => 1 + n.len(),
        Value::Floats(n) => 1 + n.len(),
        Value::Object(o) => 1 + o.fields.iter().map(|(_, v)| count(v)).sum::<usize>(),
        _ => 1,
    }
}

fn main() {
    let mut paths = Vec::new();
    for a in std::env::args().skip(1) {
        walk(Path::new(&a), &mut paths);
    }
    let files: Vec<Vec<u8>> = paths
        .iter()
        .map(|p| std::fs::read(p).unwrap())
        .filter(|b| Granny::parse(b).is_ok())
        .collect();
    let bytes: usize = files.iter().map(Vec::len).sum();
    println!("{} Granny resources, {:.1} MB", files.len(), bytes as f64 / 1e6);
    let mut values = 0;
    for (what, tree) in [("parse", false), ("parse + root", true)] {
        let mut best = f64::MAX;
        for _ in 0..3 {
            let t = Instant::now();
            values = 0;
            for b in &files {
                let g = Granny::parse(b).unwrap();
                if tree && let Ok(v) = g.root(64) {
                    values += count(&v);
                }
            }
            best = best.min(t.elapsed().as_secs_f64());
        }
        println!("{what:14} {best:.3} s");
    }
    println!("{values} values in the trees");
}
