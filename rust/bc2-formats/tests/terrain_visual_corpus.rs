//! Round TG2's writers on DICE's data in Project OPFOR BFBC2's game copy (run by `run_tests.py game`; ignored by a
//! plain `cargo test`):
//!
//!   cargo test --release -p bc2-formats --test terrain_visual_corpus -- --ignored --nocapture
//!
//! - every `VisualTerrain` and `VisualWater` of the bundles dated before 2013 (each resource once) decodes and
//!   encodes byte for byte (`visual_terrain`, `water::VisualWaterFile`); DICE's open-water vertices are
//!   `water::open_water_vertex` of their position, byte for byte;
//! - DICE's terrain colour / occlusion maps: the level sizes `itexture` writes are DICE's, and a map decoded,
//!   encoded by `itexture` and decoded again stays within a few levels of DICE's texels;
//! - `shaderdb::terrain::clone_terrain`: `MP_005`'s patch, undergrowth, cache and water entries cloned under another
//!   terrain's name into `MP_005`'s three databases keep their programs' keys, shaders and vertex layouts, with the
//!   occlusion map renamed; DICE's own twin `SP_008_B` has `SP_008`'s program keys under its own name.
//!
//! The game copy is `OPFOR_ROOT`'s `game`, else `project-opfor-bfbc2\game` beside the suite; the tests pass with a
//! note when it is missing.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    missing_docs
)]

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use bc2_formats::shaderdb::{self, merge, open, terrain as sterrain};
use bc2_formats::visual_terrain::VisualTerrain;
use bc2_formats::{fbrb, itexture, water};

const DICE_BEFORE: u64 = 1_356_998_400;

fn game() -> Option<PathBuf> {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    let g = root.join("game");
    g.join("Dist").join("win32").join("levels").is_dir().then_some(g)
}

fn walk(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&p, name, out);
        } else if p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
            && p.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .is_some_and(|d| d.as_secs() < DICE_BEFORE)
        {
            out.push(p);
        }
    }
}

/// Every resource of `types` in DICE's bundles named `bundle`, each name once: (name, type, data).
fn corpus(g: &Path, bundle: &str, keep: impl Fn(&str, &str) -> bool) -> Vec<(String, String, Vec<u8>)> {
    let mut bundles = Vec::new();
    walk(g, bundle, &mut bundles);
    bundles.sort();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for b in &bundles {
        let h = fbrb::entries_path(b).unwrap();
        let want: Vec<(String, String)> = h
            .entries
            .iter()
            .filter(|e| !e.is_deleted() && keep(&e.name, &e.type_name))
            .filter(|e| seen.insert(e.name.to_ascii_lowercase()))
            .map(|e| (e.name.clone(), e.type_name.clone()))
            .collect();
        if want.is_empty() {
            continue;
        }
        let names: Vec<String> = want.iter().map(|w| w.0.clone()).collect();
        for ((n, t), d) in want.into_iter().zip(fbrb::read_path_uncached(b, &names).unwrap()) {
            out.push((n, t, d));
        }
    }
    out
}

// spec: TERR-VT-8, WATER-VIS-6
#[test]
#[ignore = "game tier: reads the game copy (run_tests.py game)"]
fn dice_visual_terrains_and_waters_round_trip() {
    let Some(g) = game() else {
        println!("no game copy: skipped");
        return;
    };
    let res = corpus(&g, "terrain-00.fbrb", |_, t| t == "VisualTerrain" || t == "VisualWater");
    let (mut vts, mut vws, mut open_v, mut open_same) = (0, 0, 0usize, 0usize);
    let mut bad = Vec::new();
    for (name, ty, d) in &res {
        if ty == "VisualTerrain" {
            vts += 1;
            match VisualTerrain::decode(d) {
                Ok(v) if v.encode() == *d => {}
                Ok(_) => bad.push(format!("{name}: re-encodes to other bytes")),
                Err(e) => bad.push(format!("{name}: {e}")),
            }
            continue;
        }
        vws += 1;
        let v = match water::VisualWaterFile::decode(d) {
            Ok(v) if v.encode() == *d => v,
            Ok(_) => {
                bad.push(format!("{name}: re-encodes to other bytes"));
                continue;
            }
            Err(e) => {
                bad.push(format!("{name}: {e}"));
                continue;
            }
        };
        assert_eq!(v.tail.len(), 4, "{name}: the far plane");
        if v.tail != water::VISUAL_WATER_TAIL {
            println!(
                "{name}: far plane {}",
                f32::from_le_bytes([v.tail[0], v.tail[1], v.tail[2], v.tail[3]])
            );
        }
        // the open-water vertices: normal (0, 1, 0, 0) and tangent (1, 0, 0, 0)
        for vert in v.vertices.as_chunks::<36>().0 {
            let h = |k: usize| u16::from_le_bytes([vert[2 * k], vert[2 * k + 1]]);
            let up = [h(4), h(5), h(6), h(7), h(8), h(9), h(10), h(11)] == [0, 0x3C00, 0, 0, 0x3C00, 0, 0, 0];
            if !up {
                continue;
            }
            open_v += 1;
            let f = |k: usize| half(h(k));
            open_same += usize::from(water::open_water_vertex(f(0), f(1), f(2)) == *vert);
        }
    }
    println!("{vts} visual terrains, {vws} visual waters; open-water vertices {open_same} of {open_v} as written");
    assert!(bad.is_empty(), "{bad:#?}");
    assert_eq!(vts, 30);
    assert!(vws >= 28, "{vws}");
    assert!(open_v > 1000 && open_same == open_v, "{open_same} of {open_v}");
}

/// An IEEE half's value.
fn half(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = i32::from((h >> 10) & 0x1F);
    let m = f32::from(h & 0x3FF);
    s * if e == 0 {
        m * 2f32.powi(-24)
    } else {
        (1.0 + m / 1024.0) * 2f32.powi(e - 15)
    }
}

// spec: TGEN-OCC-1
#[test]
#[ignore = "game tier: reads the game copy (run_tests.py game)"]
fn dice_occlusion_maps_encode_as_dice_lays_them_out() {
    let Some(g) = game() else {
        println!("no game copy: skipped");
        return;
    };
    let maps = corpus(&g, "level-00.fbrb", |n, t| {
        let low = n.to_ascii_lowercase();
        t == "ITexture" && low.starts_with("terrains/") && (low.contains("occlusion") || low.ends_with("_ao.res"))
    });
    let mut checked = 0;
    for (name, _, d) in &maps {
        let h = itexture::Header::read(d).unwrap();
        let Some(f) = itexture::Format::from_code(h.format) else {
            println!("{name}: format {} not written here", h.format);
            continue;
        };
        let sizes = itexture::level_sizes(f, h.width, h.height, h.levels);
        assert_eq!(
            h.sizes[..h.levels as usize]
                .iter()
                .map(|s| *s as usize)
                .collect::<Vec<_>>(),
            sizes,
            "{name}"
        );
        assert_eq!(d.len(), itexture::HEADER + sizes.iter().sum::<usize>(), "{name}");
        // DICE's level 0 encoded again: the difference against DICE's own decoded texels
        let pic = itexture::decode_level(d, 0).unwrap();
        let again = itexture::encode(&pic, f, h.flags, true);
        assert_eq!(again.len(), d.len(), "{name}");
        assert_eq!(again[..itexture::HEADER], d[..itexture::HEADER], "{name}: header");
        let back = itexture::decode_level(&again, 0).unwrap();
        let (mut sum, mut worst) = (0u64, 0u8);
        for (a, b) in back.rgb.iter().zip(&pic.rgb) {
            for c in 0..3 {
                let e = a[c].abs_diff(b[c]);
                sum += u64::from(e);
                worst = worst.max(e);
            }
        }
        let mean = sum as f64 / (3 * pic.rgb.len()) as f64;
        println!(
            "{name}: {:?} {} x {}, {} levels; re-encoded mean error {mean:.2}, worst {worst}",
            f, h.width, h.height, h.levels
        );
        assert!(mean < 2.0, "{name}: mean error {mean}");
        checked += 1;
    }
    assert!(checked >= 12, "{checked}");
}

fn level_db(g: &Path, level: &str, dx: u32) -> Vec<u8> {
    let p = g
        .join("Dist")
        .join("win32")
        .join("levels")
        .join(level)
        .join("level-00.fbrb");
    let h = fbrb::entries_path(&p).unwrap();
    let want = format!("dx{dx}_single.res");
    let name = h
        .entries
        .iter()
        .find(|e| e.name.to_ascii_lowercase().ends_with(&want) && e.name.to_ascii_lowercase().contains("/shaders/"))
        .unwrap()
        .name
        .clone();
    fbrb::read_path_uncached(&p, &[name]).unwrap().remove(0)
}

/// Each entry's programs as (key past the entry's GUID, vertex shader code, pixel shader code), sorted.
fn programs(db: &shaderdb::ShaderDb<'_>, entry: &str) -> Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let groups = merge::entry_programs(db);
    let e = db.entries.iter().rev().find(|e| *e.name == *entry.as_bytes()).unwrap();
    let mut out: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = groups
        .get(e.guid)
        .copied()
        .unwrap_or_default()
        .iter()
        .map(|&i| {
            let [v, f, _, _] = merge::program_refs(db, usize::from(i)).unwrap();
            (
                db.keys[usize::from(i)][16..].to_vec(),
                db.vs[v].code.to_vec(),
                db.ps[f].code.to_vec(),
            )
        })
        .collect();
    out.sort();
    out
}

fn textures(db: &shaderdb::ShaderDb<'_>, entry: &str) -> Vec<String> {
    let scan = shaderdb::levels::scan(db).unwrap();
    let e = scan.iter().rev().find(|e| e.name == entry.as_bytes()).unwrap();
    e.textures
        .iter()
        .map(|t| String::from_utf8_lossy(t.2).into_owned())
        .collect()
}

// spec: TGEN-VIS-4
#[test]
#[ignore = "game tier: reads the game copy (run_tests.py game)"]
fn a_donor_terrains_entries_clone_under_a_new_name() {
    let Some(g) = game() else {
        println!("no game copy: skipped");
        return;
    };
    let ao = "Terrains/MP_005/Textures/mp05_AmbientOcclusion".to_owned();
    let ao_new = "Terrains/TG_PROOF/Textures/TG_PROOF_AmbientOcclusion".to_owned();
    for dx in [9u32, 10, 11] {
        let data = level_db(&g, "mp_005", dx);
        let layout = if dx == 11 { 10 } else { dx };
        let donor = open::open(&data, Some(layout), Some(false), "mp_005").unwrap().db;
        let mut names = sterrain::mask_entries(&donor, "MP_005");
        names.extend([
            "Terrains/MP_005/Shaders/MP5_UGCache".to_owned(),
            "Terrains/MP_005/Shaders/Water".to_owned(),
        ]);
        let t = std::time::Instant::now();
        let c = sterrain::clone_terrain(
            &data,
            &data,
            dx,
            &names,
            "MP_005",
            "TG_PROOF",
            &[(ao.clone(), ao_new.clone())],
        )
        .unwrap();
        let secs = t.elapsed().as_secs_f64();
        assert!(
            c.missing.is_empty() && c.kept.is_empty(),
            "{:?} {:?}",
            c.missing,
            c.kept
        );
        assert_eq!(c.added.len(), names.len());
        let out = open::open(&c.bytes, Some(layout), Some(false), "cloned").unwrap().db;
        assert_eq!(out.entries.len(), donor.entries.len() + names.len());
        let (mut same, mut ao_hits) = (0, 0);
        for (old, new, n) in &c.added {
            let (a, b) = (programs(&donor, old), programs(&out, new));
            assert_eq!(a.len(), *n);
            // the keys past the entry GUID, the shaders: the donor's (TEDIT-SH-1: no compiler)
            same += usize::from(a == b);
            let (ta, tb) = (textures(&donor, old), textures(&out, new));
            let want: Vec<String> = ta
                .iter()
                .map(|x| if *x == ao { ao_new.clone() } else { x.clone() })
                .collect();
            let mut want_sorted = want.clone();
            want_sorted.sort();
            let mut got = tb.clone();
            got.sort();
            assert_eq!(got, want_sorted, "{new}");
            ao_hits += usize::from(tb.contains(&ao_new));
            // the donor's own entry is still there, unchanged
            assert_eq!(programs(&out, old), a);
        }
        println!(
            "Dx{dx}: {} entries cloned in {secs:.2} s, {same} with the donor's keys and shaders, the occlusion map \
             renamed in {ao_hits}; {} -> {} bytes",
            c.added.len(),
            data.len(),
            c.bytes.len()
        );
        assert_eq!(same, c.added.len());
        assert!(ao_hits >= 50, "{ao_hits}");
    }
    // DICE's twin: SP_008_B's patch entries are SP_008's combinations with the same program keys
    for dx in [9u32, 10, 11] {
        let layout = if dx == 11 { 10 } else { dx };
        let (da, db) = (level_db(&g, "sp_008", dx), level_db(&g, "sp_008_b", dx));
        let (a, b) = (
            open::open(&da, Some(layout), Some(false), "sp_008").unwrap().db,
            open::open(&db, Some(layout), Some(false), "sp_008_b").unwrap().db,
        );
        let key = |n: &str| n.rsplit_once("/PatchMaskShaders/").map(|x| x.1.to_owned());
        let bn: BTreeMap<String, String> = sterrain::mask_entries(&b, "SP_008_B")
            .into_iter()
            .filter_map(|n| key(&n).map(|k| (k, n)))
            .collect();
        let (mut shared, mut same_keys, mut ao_twin) = (0, 0, 0);
        for n in sterrain::mask_entries(&a, "SP_008") {
            let Some(k) = key(&n) else { continue };
            let Some(m) = bn.get(&k) else { continue };
            shared += 1;
            let ka: Vec<Vec<u8>> = programs(&a, &n).into_iter().map(|p| p.0).collect();
            let kb: Vec<Vec<u8>> = programs(&b, m).into_iter().map(|p| p.0).collect();
            same_keys += usize::from(ka == kb);
            ao_twin += usize::from(
                textures(&b, m)
                    .iter()
                    .any(|t| t == "Terrains/SP_008_B/Textures/sp08_ambientocclusion"),
            );
        }
        println!(
            "Dx{dx}: SP_008 / SP_008_B: {shared} shared patch combinations, {same_keys} with the same program keys, {ao_twin} sampling the twin's own occlusion map"
        );
        assert!(
            shared >= 15 && same_keys == shared && ao_twin + 1 >= shared,
            "{same_keys} of {shared}"
        );
    }
}
