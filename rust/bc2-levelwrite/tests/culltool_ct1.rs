//! Round CT1's proof that the streamed readers answer as the whole-document ones: `scan_level` against
//! `Level::read` and `CullTree::read_only` against `CullTree::read`, on every level partition and cull tree of
//! Project OPFOR's `work\levels` (found beside the suite, or `OPFOR_ROOT`), and on damaged copies of some of them
//! (cut short, bytes changed) - the same values, or the same error (kind and text). Writes only a temporary
//! folder; skipped where there is no `work\levels`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stderr,
    clippy::cast_possible_truncation
)]

use std::path::{Path, PathBuf};

use bc2_levelwrite::culltool::{CullTree, Level, level_partitions, scan_level};

fn levels_dir() -> Option<PathBuf> {
    if let Some(r) = std::env::var_os("OPFOR_ROOT") {
        let d = PathBuf::from(r).join("work").join("levels");
        return d.is_dir().then_some(d);
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("work").join("levels"))
        .find(|d| d.is_dir())
}

/// `Level::read`'s answer as `scan_level` gives it (objects, tree references), or the error.
fn level_whole(p: &str) -> String {
    match Level::read(p) {
        Ok(l) => {
            let mut refs: Vec<_> = l.tree_refs.into_iter().collect();
            refs.sort();
            format!("{:?} {refs:?}", l.objects)
        }
        Err(e) => format!("{e:?}"),
    }
}

fn level_streamed(p: &str) -> String {
    match scan_level(p) {
        Ok(l) => {
            let mut refs: Vec<_> = l.tree_refs.into_iter().collect();
            refs.sort();
            format!("{:?} {refs:?}", l.objects)
        }
        Err(e) => format!("{e:?}"),
    }
}

fn tree_text(r: Result<CullTree, bc2_levelwrite::culltool::CullError>) -> String {
    match r {
        Ok(t) => {
            let mut leaves: Vec<_> = t.leaf_node.iter().map(|(k, v)| (*k, *v)).collect();
            leaves.sort_unstable();
            format!(
                "{} {} {} {:?} {:?} {}",
                t.enum_offset,
                t.used,
                t.spare,
                t.nodes,
                leaves,
                t.arena.nodes.len()
            )
        }
        Err(e) => format!("{e:?}"),
    }
}

fn same_answers(p: &str) {
    assert_eq!(level_whole(p), level_streamed(p), "level {p}");
    assert_eq!(
        tree_text(CullTree::read(p)),
        tree_text(CullTree::read_only(p)),
        "tree {p}"
    );
}

/// Damaged copies of the file: cut at 12 places, then 12 single bytes changed.
fn damaged(src: &Path, tmp: &Path) {
    let data = std::fs::read(src).unwrap();
    let n = data.len();
    let p = tmp.join(src.file_name().unwrap());
    let ps = p.to_str().unwrap();
    for k in 1..=12 {
        let cut = n * k / 13;
        std::fs::write(&p, &data[..cut]).unwrap();
        same_answers(ps);
    }
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15 ^ n as u64;
    for _ in 0..12 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let at = 24 + (seed >> 33) as usize % (n - 24);
        let mut d = data.clone();
        d[at] ^= (seed >> 8) as u8 | 1;
        std::fs::write(&p, &d).unwrap();
        same_answers(ps);
    }
}

#[test]
#[ignore = "game tier: whole cull trees; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn streamed_readers_answer_as_the_whole_document() {
    let _gate = crate::gate();
    let Some(dir) = levels_dir() else {
        eprintln!("skipped: no work\\levels");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("culltool-ct1-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut levels = 0;
    let mut trees = 0;
    let mut folders: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    folders.sort();
    for (i, f) in folders.iter().filter(|f| f.is_dir()).enumerate() {
        let lv = level_partitions(f.to_str().unwrap()).unwrap();
        for p in &lv {
            same_answers(p);
            levels += 1;
        }
        // the trees beside them, and the descriptions (not levels, not trees)
        let mut others: Vec<String> = Vec::new();
        for p in &lv {
            let folder = Path::new(p).with_extension("");
            for name in ["staticculltree_exclusive.dbx", "staticculltree_client.dbx"] {
                for cand in [folder.join(name), Path::new(p).parent().unwrap().join(name)] {
                    if cand.is_file() {
                        others.push(cand.to_str().unwrap().to_owned());
                    }
                }
            }
            let desc = format!("{}_description.dbx", p.trim_end_matches(".dbx"));
            if Path::new(&desc).is_file() {
                others.push(desc);
            }
        }
        others.sort();
        others.dedup();
        for p in &others {
            same_answers(p);
            trees += 1;
        }
        // damaged copies of the first level and first other file of every 6th folder
        if i % 6 != 0 {
            continue;
        }
        if let Some(p) = lv.first() {
            damaged(Path::new(p), &tmp);
        }
        if let Some(p) = others.first() {
            damaged(Path::new(p), &tmp);
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    eprintln!("{levels} level partitions, {trees} trees and descriptions: the same answers");
    assert!(levels > 0);
}
