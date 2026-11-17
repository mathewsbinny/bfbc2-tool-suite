//! The bundle cache against the uncached reader: the same bytes and the same errors for every read, in any order,
//! through memory, continued sessions and the disk cache, on whole and damaged bundles.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rayon::prelude::*;

use super::*;
use crate::fbrb::{build, read_header, read_path_uncached};

/// A folder of its own for each test (tests run side by side).
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("bc2_fbrb_cache_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Bytes that compress poorly (`noise`) or well (`text`), from a seed.
fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

fn text(n: usize, seed: u64) -> Vec<u8> {
    let words: [&[u8]; 6] = [b"Levels/", b"Mesh ", b"guid=", b"<instance ", b"0.5 ", b"\n"];
    let r = noise(n, seed);
    let mut out = Vec::with_capacity(n + 16);
    let mut i = 0;
    while out.len() < n {
        out.extend_from_slice(words[r[i % r.len()] as usize % words.len()]);
        out.push(b'a' + r[(i * 7) % r.len()] % 26);
        i += 1;
    }
    out.truncate(n);
    out
}

/// A bundle of ~4.5 MB of entries (about 3 MB compressed: several 1 MiB chunks), with a deleted marker, an empty
/// entry, and a name listed twice.
fn items() -> Vec<(String, &'static str, Vec<u8>)> {
    let mut v = Vec::new();
    for i in 0..24u64 {
        let data = if i % 3 == 0 {
            noise(150_000 + (i as usize) * 1_000, i)
        } else {
            text(120_000, i)
        };
        v.push((format!("Levels/Part{i:02}.dbx"), "<non-resource>", data));
    }
    v.insert(5, ("Gone.dbx".into(), crate::fbrb::DELETED, vec![]));
    v.insert(9, ("Empty.txt".into(), "Text", vec![]));
    v.push(("Levels/Part03.dbx".into(), "<non-resource>", b"the later one".to_vec()));
    v
}

fn bundle(zipped: bool) -> Vec<u8> {
    let it = items();
    let refs: Vec<(&str, &str, &[u8])> = it.iter().map(|(n, t, d)| (n.as_str(), *t, d.as_slice())).collect();
    build(&refs, zipped)
}

/// Every read pattern the tests try: single entries early to late and back, pairs, the empty entry, everything.
fn patterns() -> Vec<Vec<String>> {
    let names: Vec<String> = (0..24).map(|i| format!("levels/part{i:02}.dbx")).collect();
    let mut p: Vec<Vec<String>> = vec![
        vec![names[0].clone()],
        vec![names[12].clone()],
        vec![names[2].clone()],
        vec![names[23].clone(), names[1].clone()],
        vec!["empty.txt".into()],
        vec![names[7].clone(), "EMPTY.TXT".into(), names[7].clone()],
        vec![names[3].clone()],
        vec![],
        vec!["gone.dbx".into()],
        vec![names[18].clone(), "missing".into()],
    ];
    p.push(names.clone());
    p.push(names.iter().rev().cloned().collect());
    p
}

fn same(a: &Result<Vec<Vec<u8>>>, b: &Result<Vec<Vec<u8>>>) -> bool {
    match (a, b) {
        (Ok(x), Ok(y)) => x == y,
        (Err(x), Err(y)) => x.to_string() == y.to_string(),
        _ => false,
    }
}

fn config(memory: u64, folder: Option<&Path>) -> CacheConfig {
    CacheConfig {
        memory,
        folder: folder.map(Path::to_path_buf),
        disk_cap: u64::MAX,
        disk_min: 0,
    }
}

/// Every pattern read through `cache` (twice over, so later reads meet kept state) equals the uncached reader.
fn check_all(cache: &Cache, path: &Path) {
    for round in 0..2 {
        for names in patterns() {
            let want = read_path_uncached(path, &names);
            let got = cache.read_path(path, &names);
            assert!(same(&want, &got), "round {round}, {names:?}: {want:?} != {got:?}");
        }
    }
}

#[test]
fn memory_sessions_match() {
    let dir = TempDir::new("memory");
    for zipped in [true, false] {
        let p = dir.0.join(format!("b{zipped}.fbrb"));
        fs::write(&p, bundle(zipped)).unwrap();
        for memory in [0, 1 << 20, u64::MAX] {
            let cache = Cache::new(config(memory, None));
            check_all(&cache, &p);
            assert!(cache.memory_used() <= memory);
            let s = cache.stats();
            if zipped && memory == u64::MAX {
                assert!(s.memory_hits > 0 && s.header_hits > 0, "{s:?}");
                // the payload decompressed once, across every read
                let payload = items().iter().map(|i| i.2.len() as u64).sum::<u64>();
                assert!(s.decompressed_bytes <= payload + (1 << 21), "{s:?}");
            }
        }
    }
}

#[test]
fn disk_cache_matches_and_is_shared() {
    let dir = TempDir::new("disk");
    let folder = dir.0.join("cache");
    let p = dir.0.join("b.fbrb");
    fs::write(&p, bundle(true)).unwrap();
    for memory in [0, u64::MAX] {
        let _ = fs::remove_dir_all(&folder);
        let first = Cache::new(config(memory, Some(&folder)));
        check_all(&first, &p);
        let s = first.stats();
        assert_eq!(s.written, 1, "{s:?}");
        assert!(s.disk_hits > 0, "{s:?}");
        assert_eq!(disk_usage(&folder).1, 1);
        // another process: every read from disk, nothing decompressed
        let second = Cache::new(config(memory, Some(&folder)));
        check_all(&second, &p);
        let s = second.stats();
        assert_eq!((s.decompressed_bytes, s.written), (0, 0), "{s:?}");
        assert_eq!(s.disk_hits, s.reads);
    }
}

#[test]
fn small_bundles_stay_off_disk() {
    let dir = TempDir::new("small");
    let folder = dir.0.join("cache");
    let p = dir.0.join("b.fbrb");
    fs::write(&p, bundle(true)).unwrap();
    let mut c = config(u64::MAX, Some(&folder));
    c.disk_min = 1 << 30;
    let cache = Cache::new(c);
    check_all(&cache, &p);
    assert_eq!(cache.stats().written, 0);
    assert!(!folder.exists());
}

#[test]
fn a_changed_bundle_is_never_answered_from_the_cache() {
    let dir = TempDir::new("stale");
    let folder = dir.0.join("cache");
    let p = dir.0.join("b.fbrb");
    let one = bundle(true);
    fs::write(&p, &one).unwrap();
    let cache = Cache::new(config(u64::MAX, Some(&folder)));
    let names = ["levels/part20.dbx"];
    for _ in 0..3 {
        cache.read_path(&p, &names).unwrap();
    }
    assert_eq!(cache.stats().written, 1);
    // the same length, other bytes in the entry, another modification time
    let it = items();
    let mut other = it.clone();
    for b in &mut other.iter_mut().find(|i| i.0 == "Levels/Part20.dbx").unwrap().2 {
        *b = b.wrapping_add(1);
    }
    let refs: Vec<(&str, &str, &[u8])> = other.iter().map(|(n, t, d)| (n.as_str(), *t, d.as_slice())).collect();
    let two = build(&refs, true);
    fs::write(&p, &two).unwrap();
    File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_secs(5))
        .unwrap();
    let want = read_path_uncached(&p, &names).unwrap();
    assert_ne!(
        want,
        vec![it.iter().find(|i| i.0 == "Levels/Part20.dbx").unwrap().2.clone()]
    );
    for _ in 0..3 {
        assert_eq!(cache.read_path(&p, &names).unwrap(), want);
    }
    assert_eq!(
        Cache::new(config(0, Some(&folder))).read_path(&p, &names).unwrap(),
        want
    );
}

#[test]
fn damaged_cache_files_are_ignored() {
    let dir = TempDir::new("badfile");
    let folder = dir.0.join("cache");
    let p = dir.0.join("b.fbrb");
    fs::write(&p, bundle(true)).unwrap();
    let names = ["levels/part22.dbx", "levels/part01.dbx"];
    let want = read_path_uncached(&p, &names).unwrap();
    let cache = Cache::new(config(0, Some(&folder)));
    cache.read_path(&p, &names).unwrap();
    cache.read_path(&p, &names).unwrap();
    let file = fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "payload"))
        .unwrap();
    let whole = fs::read(&file).unwrap();
    for cut in [whole.len() - 1, 30, 4] {
        fs::write(&file, &whole[..cut]).unwrap();
        let fresh = Cache::new(config(0, Some(&folder)));
        assert_eq!(fresh.read_path(&p, &names).unwrap(), want);
        assert_eq!(fresh.stats().disk_hits, 0);
    }
    let mut wrong = whole.clone();
    wrong[25] ^= 1; // the key
    fs::write(&file, &wrong).unwrap();
    let fresh = Cache::new(config(0, Some(&folder)));
    assert_eq!(fresh.read_path(&p, &names).unwrap(), want);
    assert_eq!(fresh.stats().disk_hits, 0);
}

/// Damaged payloads: deflate data changed at several places, the trailer's checksum and length, the stream cut
/// short at several places. Every read pattern, in the order given, through every kind of cache, equals the
/// uncached reader (the same bytes, or the same error).
#[test]
fn damaged_bundles_give_the_same_errors() {
    let dir = TempDir::new("damaged");
    let good = bundle(true);
    let start = read_header(&mut good.as_slice()).unwrap().payload_start as usize;
    let n = good.len();
    let mut cases: Vec<Vec<u8>> = Vec::new();
    for at in [
        start + 3,
        start + 20,
        start + (n - start) / 3,
        start + (n - start) / 2,
        n - 300,
        n - 8,
        n - 3,
    ] {
        for x in [0x01u8, 0x80, 0xFF] {
            let mut b = good.clone();
            b[at] ^= x;
            cases.push(b);
        }
    }
    for cut in [start + 5, start + (1 << 20), start + (1 << 20) + 17, n - 5, n - 1] {
        cases.push(good[..cut].to_vec());
    }
    let mut extra = good.clone();
    extra.extend(b"after the member");
    cases.push(extra);
    let folder = dir.0.join("cache");
    let mut failures = 0;
    for (i, case) in cases.iter().enumerate() {
        let p = dir.0.join(format!("d{i}.fbrb"));
        fs::write(&p, case).unwrap();
        let wants: Vec<_> = patterns().iter().map(|names| read_path_uncached(&p, names)).collect();
        failures += wants.iter().filter(|w| w.is_err()).count();
        for (memory, disk) in [
            (u64::MAX, None),
            (0, Some(&folder)),
            (u64::MAX, Some(&folder)),
            (1 << 20, None),
        ] {
            let cache = Cache::new(config(memory, disk.map(PathBuf::as_path)));
            for (names, want) in patterns().iter().zip(&wants) {
                let got = cache.read_path(&p, names);
                assert!(same(want, &got), "case {i}, {names:?}: {want:?} != {got:?}");
            }
        }
    }
    assert!(failures > 50, "the cases should fail often ({failures})");
}

#[test]
fn reads_side_by_side() {
    let dir = TempDir::new("parallel");
    let folder = dir.0.join("cache");
    let p = dir.0.join("b.fbrb");
    fs::write(&p, bundle(true)).unwrap();
    for c in [
        config(u64::MAX, None),
        config(0, Some(&folder)),
        config(u64::MAX, Some(&folder)),
    ] {
        let cache = Cache::new(c);
        let all = patterns();
        let jobs: Vec<&Vec<String>> = all.iter().cycle().take(48).collect();
        jobs.par_iter().for_each(|names| {
            let (want, got) = (read_path_uncached(&p, names), cache.read_path(&p, names));
            assert!(same(&want, &got), "{names:?}: {want:?} != {got:?}");
        });
    }
}

#[test]
fn extract_through_the_cache() {
    let dir = TempDir::new("extract");
    let folder = dir.0.join("cache");
    for zipped in [true, false] {
        let p = dir.0.join(format!("b{zipped}.fbrb"));
        fs::write(&p, bundle(zipped)).unwrap();
        let want = crate::fbrb::extract_path_uncached(&p).unwrap();
        let cache = Cache::new(config(u64::MAX, Some(&folder)));
        for _ in 0..3 {
            assert_eq!(cache.extract_path(&p).unwrap(), want);
        }
    }
}

#[test]
fn the_disk_cache_keeps_to_its_size() {
    let dir = TempDir::new("trim");
    let folder = dir.0.join("cache");
    let mut c = config(0, Some(&folder));
    c.disk_cap = 1;
    let cache = Cache::new(c);
    let mut paths = Vec::new();
    for i in 0..3 {
        let p = dir.0.join(format!("b{i}.fbrb"));
        fs::write(&p, bundle(true)).unwrap();
        cache.read_path(&p, &["levels/part01.dbx"]).unwrap();
        cache.read_path(&p, &["levels/part01.dbx"]).unwrap();
        paths.push(p);
    }
    assert_eq!(cache.stats().written, 3);
    // only the newest stays (it is never deleted for its own room)
    assert_eq!(disk_usage(&folder).1, 1);
    // stale temporary files go
    let tmp = folder.join("x.1.2.tmp");
    fs::write(&tmp, b"x").unwrap();
    File::options()
        .write(true)
        .open(&tmp)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(7200))
        .unwrap();
    trim(&folder, u64::MAX, Path::new(""));
    assert!(!tmp.exists());
}

#[test]
fn config_from_the_environment() {
    // only the parsing: the variables are read when the process's cache is first used
    let c = CacheConfig::from_env();
    assert!(c.disk_min > 0 || std::env::var_os(DISK_MIN_ENV).is_some());
    let off = CacheConfig::off();
    assert_eq!((off.memory, off.folder), (0, None));
}

#[test]
fn identity_names_a_file() {
    let dir = TempDir::new("identity");
    let p = dir.0.join("Level-00.fbrb");
    fs::write(&p, b"x").unwrap();
    let f = File::open(&p).unwrap();
    let a = Identity::of(&p, &f).unwrap();
    assert_eq!(a.len, 1);
    let stem = a.stem();
    assert!(stem.contains("level-00_fbrb") && stem.len() > 17, "{stem}");
    let b = Identity { len: 2, ..a.clone() };
    assert_ne!(a.stem(), b.stem());
}
