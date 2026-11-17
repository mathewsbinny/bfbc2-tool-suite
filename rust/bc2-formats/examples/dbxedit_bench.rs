//! Where DbxEdit's commands spend their time in process (rust/BENCHMARKS.md, E3b): each step of `xml`, `import --like`
//! and `edit` on one file, the best of N runs.
//!
//! ```text
//! cargo run --release -p bc2-formats --example dbxedit_bench -- <file.dbx> [runs]
//! ```
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::time::Instant;

use bc2_formats::dbxedit::{EditFile, convert, editor, import};

fn best<T>(runs: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut best = f64::MAX;
    let mut out = None;
    for _ in 0..runs {
        let t = Instant::now();
        let r = f();
        best = best.min(t.elapsed().as_secs_f64());
        out = Some(r);
    }
    (best * 1e3, out.expect("at least one run"))
}

/// Every value's text, its length added to `n`.
fn walk(t: &bc2_formats::dbxedit::Table, e: &bc2_formats::dbxedit::Element, n: &mut usize) {
    match &e.children {
        Some(k) => k.iter().for_each(|c| walk(t, c, n)),
        None => *n += bc2_formats::dbxedit::value_text(t, e).unwrap().len(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: dbxedit_bench <file.dbx> [runs]");
    let runs: usize = args.get(2).and_then(|r| r.parse().ok()).unwrap_or(10);
    let data = std::fs::read(path).expect("readable");
    println!("{path}: {} KB, best of {runs}", data.len() / 1000);
    let (t, file) = best(runs, || EditFile::parse(data.clone()).unwrap());
    println!("  parse            {t:8.2} ms");
    let (t, bytes) = best(runs, || file.serialize().unwrap());
    println!("  serialize        {t:8.2} ms  (identical: {})", bytes == data);
    let (t, xml) = best(runs, || convert(&data).unwrap());
    println!("  xml (convert)    {t:8.2} ms  ({} KB of text)", xml.len() / 1000);
    let (t, n) = best(runs, || {
        let mut f = file.clone();
        editor::drop_field(&mut f, "Enumeration").unwrap()
    });
    println!("  edit dropfield   {t:8.2} ms  ({n} removed)");
    let (t, (back, report)) = best(runs, || import(|| Ok(xml.clone()), "x.xml", Some(&file), &[]).unwrap());
    println!(
        "  import --like    {t:8.2} ms  ({} kept, {} changed; identical: {})",
        report.kept,
        report.changed,
        back.serialize().unwrap() == data
    );
    let (t, _) = best(runs, || bc2_formats::dbxedit::parse_xml(&xml, "x.xml").unwrap());
    println!("    of which the XML read {t:8.2} ms");
    let (t, _) = best(runs, || {
        let mut b = bc2_formats::dbxedit::TypeBook::default();
        b.learn(&file).unwrap();
        b
    });
    println!("    the types learned     {t:8.2} ms");
    let (t, _) = best(runs, || file.clone());
    println!("    the reference copied  {t:8.2} ms");
    let (t, _) = best(runs, || {
        let mut n = 0;
        file.top_level.iter().for_each(|e| walk(&file.table, e, &mut n));
        n
    });
    println!("    every value's text    {t:8.2} ms");
}
