//! Times the Rust shader database reader and writer (rust/BENCHMARKS.md):
//!
//! ```text
//! cargo run --release -p bc2-formats --example shaderdb_bench -- <file.res> [...]
//! ```
//!
//! Each file is read into memory first, then parsed (its layout from its name: Dx9_* as Dx9, others as Dx10 / Dx11;
//! Xbox 360 files by their first bytes) and written again; prints the best of five runs per file and whether the
//! written bytes are the file's.
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::cast_precision_loss, missing_docs)]

use std::time::Instant;

use bc2_formats::shaderdb;

fn best<T>(mut f: impl FnMut() -> T) -> (f64, T) {
    let mut t_best = f64::MAX;
    let mut out = f();
    for _ in 0..5 {
        let t = Instant::now();
        out = f();
        t_best = t_best.min(t.elapsed().as_secs_f64());
    }
    (t_best, out)
}

fn main() {
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path).unwrap();
        let name = path.to_lowercase();
        let dx = if name.contains("dx9") { 9 } else { 10 };
        let be = shaderdb::is_big_endian(&data);
        let (tp, db) = best(|| shaderdb::parse(&data, dx, be));
        let Ok(db) = db else {
            println!("{path}: not read ({:?})", db.err());
            continue;
        };
        let (tw, out) = best(|| shaderdb::write(&db));
        println!(
            "{path}: {:.1} MB, {} programs: parse {:.2} ms, write {:.2} ms, same bytes {}",
            data.len() as f64 / 1e6,
            db.programs.len(),
            tp * 1e3,
            tw * 1e3,
            out == data
        );
    }
}
