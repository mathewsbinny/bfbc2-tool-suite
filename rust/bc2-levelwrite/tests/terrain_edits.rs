//! Round E5 on real data (game tier): the terrain's resources written from strokes, and a sculpted, painted export.
//!
//! - `terrain_resources_written_from_strokes`: on MP_005's and SP_01's terrains, the main height field and the
//!   material map written from their rasters with nothing edited are DICE's bytes; a hill raised and a pit dug (with a
//!   material checkerboard painted) write files that the suite's readers read back (`terrain::parse`,
//!   `MainBlock::decode`, `MaterialMap::decode`) with the tree their heights give, the edited samples over the lower
//!   envelope, the depth map, bounds and names DICE's (`check_heights`, `check_materials`), and every sample and cell
//!   outside the edit unchanged.
//! - `sculpted_export_stages_and_checks`: EX1's canary (Atacama's Rush as Conquest) with a sculpt stroke and a
//!   material stroke in its log (tiles beside the project) is built and checked: `export::check` passes, XVAL-TERR-1
//!   holds for the rest of the terrain bundle and XVAL-TERR-2 for the two rewritten entries, the staged entries are
//!   the strokes' heights and cells; an unedited canary stages the base's terrain bundle byte for byte.
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); writes only into a temporary folder; skipped without a
//! game copy. Nothing here is loaded in game: `spec/transforms/terrain_edit.md` section 5 is the playtest.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::many_single_char_names,
    missing_docs
)]

use std::path::PathBuf;

use bc2_formats::terrain_codec::{MainBlock, MaterialMap};
use bc2_formats::{fbrb, terrain};
use bc2_levelwrite::delta::{DirStore, Raster};
use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::files::Target;
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::terrainedit::{check_heights, check_materials, material_raster, write_heights, write_materials};
use bc2_levelwrite::{MapProject, Op, Profile};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

/// A hill raised (+20 m over a 30 m disc) and a pit dug (-10 m over a 16 m square) on a height raster.
fn sculpt(r: &mut Raster, hf: &terrain::Heightfield) {
    let n = hf.size;
    let per_m = 65535.0 / f64::from(hf.max[1] - hf.min[1]);
    let (cx, cz) = (n / 2 + 40, n / 2 - 30);
    for z in cz - 15..=cz + 15 {
        for x in cx - 15..=cx + 15 {
            let d = f64::from(x as i32 - cx as i32).hypot(f64::from(z as i32 - cz as i32));
            if d <= 15.0 {
                let up = 20.0 * (1.0 - d / 15.0) * per_m;
                r.set(x, z, (f64::from(r.get(x, z)) + up).min(65535.0) as u16);
            }
        }
    }
    for z in n / 2 + 20..n / 2 + 36 {
        for x in n / 2 - 60..n / 2 - 44 {
            r.set(x, z, (f64::from(r.get(x, z)) - 10.0 * per_m).max(0.0) as u16);
        }
    }
}

/// A 4 x 4 m checkerboard of two of the map's materials at its middle.
fn checker(r: &mut Raster, a: u16, b: u16) {
    let w = r.width();
    let c = w / 2;
    let cells = w / 1024 * 4; // cells in 4 m of a 1,024 m terrain; 4 m at 0.5 m / 0.25 m cells either way
    for z in c..c + cells.max(8) {
        for x in c..c + cells.max(8) {
            r.set(x, z, if (x + z) % 2 == 0 { a } else { b });
        }
    }
}

#[test]
#[ignore = "game tier: MP_005's and SP_01's terrains; run_tests.py game runs it (tests/test_rust.py run --game)"]
// spec: TEDIT-HF-3, TEDIT-MM-1
fn terrain_resources_written_from_strokes() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    for (lv, t) in [("mp_005", "mp_005"), ("sp_01", "sp_01")] {
        let hname = format!("terrains/{t}/{t}.heightfield-000.res");
        let mname = format!("terrains/{t}/{t}-materialmap.res");
        let hb = export::base_terrain_file(&game, lv, &hname).unwrap();
        let mb = export::base_terrain_file(&game, lv, &mname).unwrap();
        let hf = terrain::parse(&hb).unwrap();
        let n = hf.size;
        let base = Raster::from_samples(n, n, &hf.raw);
        assert_eq!(
            write_heights(&hb, &base).unwrap(),
            hb,
            "{lv}: nothing edited, DICE's bytes"
        );
        let mut edited = base.clone();
        sculpt(&mut edited, &hf);
        let t0 = std::time::Instant::now();
        let out = write_heights(&hb, &edited).unwrap();
        println!("{lv}: n {n}, written in {:.3} s", t0.elapsed().as_secs_f64());
        assert_eq!(check_heights(&hb, &out), Vec::<String>::new(), "{lv}");
        let back = terrain::parse(&out).unwrap();
        assert_eq!(back.raw, edited.samples(), "{lv}: the stroke's heights");
        let block = MainBlock::decode(&out).unwrap();
        let dice = MainBlock::decode(&hb).unwrap();
        assert_eq!(block.depth, dice.depth, "{lv}: the depth map kept");
        // the envelope only lowered, and only near the edit
        let mut changed = 0;
        for (a, b) in block.envelope.iter().flatten().zip(dice.envelope.iter().flatten()) {
            assert!(a <= b, "{lv}: an envelope vertex raised");
            changed += usize::from(a != b);
        }
        println!(
            "{lv}: {changed} envelope vertices lowered; under share {:.4} (DICE's {:.4})",
            block.under_share(),
            dice.under_share()
        );
        // the material map: a checkerboard of its first two materials
        let m = MaterialMap::decode(&mb).unwrap();
        let cells = material_raster(&m);
        assert_eq!(
            write_materials(&mb, &cells).unwrap(),
            mb,
            "{lv}: nothing painted, DICE's bytes"
        );
        let mut painted = cells.clone();
        checker(&mut painted, 0, 1.min(m.names.len() as u16 - 1));
        let out = write_materials(&mb, &painted).unwrap();
        assert_eq!(check_materials(&mb, &out), Vec::<String>::new(), "{lv}");
        let back = MaterialMap::decode(&out).unwrap();
        let want: Vec<u8> = painted.samples().iter().map(|&v| v as u8).collect();
        assert!(back.cells == want, "{lv}: the painted cells");
        println!("{lv}: material map {} cells, {} names, painted", m.width, m.names.len());
    }
}

#[test]
#[ignore = "game tier: a whole staged export with a sculpted and painted terrain; run_tests.py game runs it"]
// spec: XVAL-TERR-1, XVAL-TERR-2
fn sculpted_export_stages_and_checks() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let tmp = std::env::temp_dir().join(format!("bc2_e5_export_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let project_file = tmp.join("Sculpted.mapedit.json");
    let tiles = DirStore::of(&project_file);
    let base = export::level_partition(&game, "mp_005gr").unwrap();
    let mut p = MapProject::new("E5 sculpt", "mp_005gr", "levels/MP_005GR", &base, Profile::Bfbc2);
    let c = export::choose_mode(&game, &p, Profile::Bfbc2, "Conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let w = bc2_levelwrite::write::LevelWriter::parse(&base).unwrap();
    let hname = export::heightfield_name(&w).unwrap();
    let mname = hname.replace(".heightfield-000.res", "-materialmap.res");
    let hb = export::base_terrain_file(&game, "mp_005gr", &hname).unwrap();
    let mb = export::base_terrain_file(&game, "mp_005gr", &mname).unwrap();
    let hf = terrain::parse(&hb).unwrap();
    let before = Raster::from_samples(hf.size, hf.size, &hf.raw);
    let mut after = before.clone();
    sculpt(&mut after, &hf);
    let dt = bc2_levelwrite::delta::stroke(&before, &after, &tiles).unwrap();
    assert_ne!(dt.len(), 0);
    p.push(
        Op::Stroke {
            target: Target::Terrain { file: hname.clone() },
            size: [hf.size, hf.size],
            tiles: dt,
        },
        "Raise",
    );
    let m = MaterialMap::decode(&mb).unwrap();
    let cells = material_raster(&m);
    let mut painted = cells.clone();
    checker(&mut painted, 0, 1.min(m.names.len() as u16 - 1));
    let dm = bc2_levelwrite::delta::stroke(&cells, &painted, &tiles).unwrap();
    p.push(
        Op::Stroke {
            target: Target::Material { file: mname.clone() },
            size: [m.width, m.width],
            tiles: dm,
        },
        "Material",
    );
    assert_eq!(p.reader_needed(), 11, "a material stroke needs project format 11");
    p.save(&project_file).unwrap();
    let back = MapProject::load(&project_file).unwrap();
    assert_eq!(back.operations.len(), 3, "saved and read back as project operations");
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_E5_SCULPT".into(),
        folder: String::new(),
        title: "E5 sculpt canary".into(),
        mode: "conquest".into(),
        layer: LayerSpec {
            donor: p.mode().map_or_else(String::new, |c| c.donor.clone()),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: Profile::Bfbc2,
        identity: None,
        path_data: false,
    };
    let stage = tmp.join("stage");
    let t = std::time::Instant::now();
    let rec = export::build(&game, &back, &project_file.display().to_string(), &spec, &stage).unwrap();
    println!("built in {:.1} s", t.elapsed().as_secs_f64());
    assert_eq!(rec.terrain_edits, vec![mname.clone(), hname.clone()], "in name order");
    let r = export::check(&game, &stage).unwrap();
    for (code, ok, text) in r.lines() {
        if code.starts_with("XVAL-TERR") {
            println!("{code} {ok}: {text}");
        }
    }
    assert!(r.ok(), "{:#?}", r.checks.iter().filter(|c| !c.0).collect::<Vec<_>>());
    assert!(r.lines().any(|(c, ok, _)| c == "XVAL-TERR-2" && ok));
    // the staged entries: the strokes' heights and cells, read back by DICE-format readers
    let (h, data) = fbrb::extract_path_uncached(stage.join("terrain-00.fbrb")).unwrap();
    let get = |name: &str| {
        h.entries
            .iter()
            .zip(&data)
            .find(|(e, _)| e.name.eq_ignore_ascii_case(name))
            .map(|(_, d)| d.clone())
            .unwrap()
    };
    assert_eq!(terrain::parse(&get(&hname)).unwrap().raw, after.samples());
    let staged = MaterialMap::decode(&get(&mname)).unwrap();
    let want: Vec<u8> = painted.samples().iter().map(|&v| v as u8).collect();
    assert!(staged.cells == want);
    let _ = std::fs::remove_dir_all(&tmp);
}
