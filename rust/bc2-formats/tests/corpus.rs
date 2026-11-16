//! The Rust reader on the game's own files (run by `run_tests.py game`; ignored by a plain `cargo test`):
//!
//!   cargo test --release -p bc2-formats --test corpus -- --ignored --nocapture
//!
//! Every `.dbx` under Project OPFOR BFBC2's `work\levels` and `work\navkit\common` (unpacked from its game copy)
//! is read without a panic, and its XML is identical to `DbxEdit xml`'s for every file DbxEdit's fingerprints
//! (`bfbc2-dbx-tools/tests/fingerprints_795745.txt`) cover. The folder is `OPFOR_ROOT`, else
//! `project-opfor-bfbc2` beside the suite; the test passes with a note when neither has the files. The full parity
//! with the Python reader is `core/tests/test_dbx_native.py --game`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, missing_docs)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bc2_formats::dbx;
use rayon::prelude::*;

fn suite() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn opfor() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("OPFOR_ROOT")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from),
        Some(suite().join("..").join("project-opfor-bfbc2")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|p| p.join("work").join("levels").is_dir())
}

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

fn sha1(data: &[u8]) -> String {
    sha1_smol::Sha1::from(data).digest().to_string()
}

#[test]
#[ignore = "reads the game copy's unpacked files (about 1 GB); run_tests.py game runs it"]
fn corpus_xml_matches_dbxedit() {
    let Some(root) = opfor() else {
        println!("skipped: no Project OPFOR BFBC2 work folder (OPFOR_ROOT or beside the suite)");
        return;
    };
    let golden: HashMap<String, String> =
        std::fs::read_to_string(suite().join("bfbc2-dbx-tools/tests/fingerprints_795745.txt"))
            .expect("the fingerprints file")
            .lines()
            .filter_map(|l| {
                let p: Vec<&str> = l.split_whitespace().collect();
                match p.as_slice() {
                    ["xml", input, output, ..] => Some(((*input).to_owned(), (*output).to_owned())),
                    _ => None,
                }
            })
            .collect();
    let mut files = Vec::new();
    walk(&root.join("work").join("levels"), &mut files);
    walk(&root.join("work").join("navkit").join("common"), &mut files);
    let t = std::time::Instant::now();
    // (input sha1, parsed, xml sha1 or None)
    let rows: Vec<(String, bool, Option<String>)> = files
        .par_iter()
        .map(|f| {
            let data = std::fs::read(f).expect("a corpus file");
            let parsed = dbx::parse(&data).is_ok();
            let xml = dbx::to_xml(&data).ok().map(|x| sha1(x.as_bytes()));
            (sha1(&data), parsed, xml)
        })
        .collect();
    let mut seen = HashSet::new();
    let (mut checked, mut wrong, mut parsed) = (0, 0, 0);
    for (h, ok, xml) in &rows {
        if !seen.insert(h.clone()) {
            continue;
        }
        parsed += usize::from(*ok);
        if let (Some(want), Some(got)) = (golden.get(h), xml) {
            checked += 1;
            wrong += usize::from(want != got);
        }
    }
    println!(
        "{} files ({} distinct), {parsed} parsed; XML checked against DbxEdit for {checked}: {wrong} differ, in {:.1} s",
        files.len(),
        seen.len(),
        t.elapsed().as_secs_f64()
    );
    assert_eq!(wrong, 0);
    assert!(checked >= 31_000, "only {checked} fingerprinted files found");
}
