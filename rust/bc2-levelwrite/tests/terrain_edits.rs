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
//! - `painted_export_stages_and_checks` (round E5-rest): the same canary with a sculpt, the range widened (500 ->
//!   650 m), a material of the level's grid added, DryGrass painted with that material under it in one stroke, and the
//!   colour / occlusion map shaded: XVAL-TERR-2 and XVAL-TERR-3 pass, the staged height field keeps the sculpted ground
//!   in the wider range, the painted cells draw a DryGrass patch shader, the level bundle holds the shaded map.
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
            also: Vec::new(),
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
            also: Vec::new(),
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

#[test]
#[ignore = "game tier: a whole staged export with every terrain layer of round E5-rest; run_tests.py game runs it"]
// spec: XVAL-TERR-2, XVAL-TERR-3, TEDIT-HF-2, TEDIT-VT-1, TEDIT-OCC-1
fn painted_export_stages_and_checks() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let tmp = std::env::temp_dir().join(format!("bc2_e5r_export_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let project_file = tmp.join("Painted.mapedit.json");
    let tiles = DirStore::of(&project_file);
    let base = export::level_partition(&game, "mp_005gr").unwrap();
    let mut p = MapProject::new("E5-rest paint", "mp_005gr", "levels/MP_005GR", &base, Profile::Bfbc2);
    let c = export::choose_mode(&game, &p, Profile::Bfbc2, "Conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let w = bc2_levelwrite::write::LevelWriter::parse(&base).unwrap();
    let hname = export::heightfield_name(&w).unwrap();
    let stem = hname.replace(".heightfield-000.res", "");
    let mname = format!("{stem}-materialmap.res");
    let hb = export::base_terrain_file(&game, "mp_005gr", &hname).unwrap();
    let hf = terrain::parse(&hb).unwrap();
    let n = hf.size;
    // a sculpt, then the range widened from 500 m to 650 m (TEDIT-HF-2)
    let before = Raster::from_samples(n, n, &hf.raw);
    let mut after = before.clone();
    sculpt(&mut after, &hf);
    let dt = bc2_levelwrite::delta::stroke(&before, &after, &tiles).unwrap();
    p.push(
        Op::Stroke {
            target: Target::Terrain { file: hname.clone() },
            size: [n, n],
            tiles: dt,
            also: Vec::new(),
        },
        "Raise",
    );
    p.push(
        Op::TerrainRange {
            file: hname.clone(),
            from: 500.0,
            to: 650.0,
        },
        "Widen",
    );
    // a material of the level's grid the map lacks (MAT-GRID-2)
    let mb = export::base_terrain_file(&game, "mp_005gr", &mname).unwrap();
    let m = MaterialMap::decode(&mb).unwrap();
    let grid = ["mp_005gr", "mp_005"]
        .iter()
        .find_map(|lv| export::base_terrain_file(&game, lv, &format!("levels/{lv}/materials_win32.dbx")))
        .unwrap();
    let add = bc2_levelwrite::terrainedit::grid_terrain_materials(&grid)
        .into_iter()
        .find(|g| !m.names.iter().any(|n| n.eq_ignore_ascii_case(g)) && !g.to_ascii_lowercase().starts_with("default"))
        .unwrap();
    p.push(
        Op::MaterialAdd {
            file: mname.clone(),
            name: add.clone(),
        },
        "Add material",
    );
    // DryGrass painted on a 40 m disc (TEDIT-VT-1), with the added material under it in the same stroke
    let mut paint = Raster::new(n, n, 0);
    let mut cells = material_raster(&m);
    let per = m.width / n;
    for j in 960..1040u32 {
        for i in 1060..1140u32 {
            if f64::from(i as i32 - 1100).hypot(f64::from(j as i32 - 1000)) < 40.0 {
                paint.set(i, j, 255);
                for dz in 0..per {
                    for dx in 0..per {
                        cells.set(i * per + dx, j * per + dz, m.names.len() as u16);
                    }
                }
            }
        }
    }
    let paint_target = Target::Paint {
        layer: "DryGrass".into(),
    };
    let dp = bc2_levelwrite::delta::stroke(&Raster::new(n, n, 0), &paint, &tiles).unwrap();
    let dc = bc2_levelwrite::delta::stroke(&material_raster(&m), &cells, &tiles).unwrap();
    p.push(
        Op::Stroke {
            target: paint_target,
            size: [n, n],
            tiles: dp,
            also: vec![bc2_levelwrite::ops::StrokePart {
                target: Target::Material { file: mname.clone() },
                size: [m.width, m.width],
                tiles: dc,
            }],
        },
        "Paint DryGrass",
    );
    // the colour / occlusion map darkened over 64 x 64 texels (TEDIT-OCC-1)
    let cname = "terrains/mp_005/textures/mp05_ambientocclusion.res".to_owned();
    let cb = export::base_terrain_file(&game, "mp_005gr", &cname).unwrap();
    let cr = bc2_levelwrite::terrainpaint::colour_raster(&cb).unwrap();
    let mut shaded = cr.clone();
    for z in 1000..1064 {
        for x in 1100..1164 {
            let col = bc2_levelwrite::terrainpaint::colour_of(shaded.get(x, z), false);
            shaded.set(x, z, bc2_levelwrite::terrainpaint::sample_of(col.map(|v| v / 2), false));
        }
    }
    let dcol = bc2_levelwrite::delta::stroke(&cr, &shaded, &tiles).unwrap();
    p.push(
        Op::Stroke {
            target: Target::Colour { file: cname.clone() },
            size: [cr.width(), cr.height()],
            tiles: dcol,
            also: Vec::new(),
        },
        "Shade",
    );
    assert_eq!(
        p.reader_needed(),
        12,
        "the new terrain operations need project format 12"
    );
    p.save(&project_file).unwrap();
    let back = MapProject::load(&project_file).unwrap();
    assert_eq!(back.operations.len(), 6);
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_E5R_PAINT".into(),
        folder: String::new(),
        title: "E5-rest paint canary".into(),
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
    for l in rec
        .report
        .iter()
        .filter(|l| l.starts_with("project:") || l.contains("patch shader entr"))
    {
        println!("{l}");
    }
    let vname = format!("{stem}.visual.res");
    let aname = format!("{stem}.masktileatlas.res");
    assert_eq!(
        rec.terrain_edits,
        vec![mname.clone(), hname.clone(), aname.clone(), vname.clone()],
        "in name order"
    );
    assert_eq!(rec.colour_edits, vec![cname.clone()]);
    let r = export::check(&game, &stage).unwrap();
    for (code, ok, text) in r.lines() {
        if code.starts_with("XVAL-TERR") {
            println!("{code} {ok}: {text}");
        }
    }
    assert!(r.ok(), "{:#?}", r.checks.iter().filter(|c| !c.0).collect::<Vec<_>>());
    assert!(r.lines().any(|(c, ok, _)| c == "XVAL-TERR-3" && ok));
    // the staged entries read back
    let (h, data) = fbrb::extract_path_uncached(stage.join("terrain-00.fbrb")).unwrap();
    let get = |name: &str| {
        h.entries
            .iter()
            .zip(&data)
            .find(|(e, _)| e.name.eq_ignore_ascii_case(name))
            .map(|(_, d)| d.clone())
            .unwrap()
    };
    let staged_hf = terrain::parse(&get(&hname)).unwrap();
    assert_eq!(staged_hf.max[1], 650.0);
    let worst = staged_hf
        .raw
        .iter()
        .zip(after.samples())
        .map(|(a, b)| (f64::from(*a) * 650.0 - f64::from(b) * 500.0).abs() / 65535.0)
        .fold(0.0, f64::max);
    assert!(worst < 0.01, "the sculpted ground kept within a stored unit: {worst} m");
    let staged_m = MaterialMap::decode(&get(&mname)).unwrap();
    assert_eq!(staged_m.names.last(), Some(&add));
    assert_eq!(
        staged_m.get(1100 * per as usize, 1000 * per as usize),
        m.names.len() as u8,
        "the paired material under the paint"
    );
    let vt = bc2_formats::visual_terrain::VisualTerrain::decode(&get(&vname)).unwrap();
    let base_vt = bc2_formats::visual_terrain::VisualTerrain::decode(
        &export::base_terrain_file(&game, "mp_005gr", &vname).unwrap(),
    )
    .unwrap();
    let cell = (1000 / 16) * 128 + 1100 / 16;
    assert_ne!(
        vt.maps[0].shader[cell], base_vt.maps[0].shader[cell],
        "the painted cell's shader"
    );
    assert!(
        vt.shaders[usize::from(vt.maps[0].shader[cell])]
            .name
            .contains("DryGrass")
    );
    let (lh, ld) = fbrb::extract_path_uncached(stage.join("level-00.fbrb")).unwrap();
    let staged_c = lh
        .entries
        .iter()
        .zip(&ld)
        .find(|(e, _)| e.name.eq_ignore_ascii_case(&cname))
        .map(|(_, d)| d.clone())
        .unwrap();
    let back_c = bc2_levelwrite::terrainpaint::colour_raster(&staged_c).unwrap();
    assert!(back_c.get(1130, 1030) != cr.get(1130, 1030), "the shaded texel");
    assert_eq!(back_c.get(10, 10), cr.get(10, 10), "a far texel DICE's");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[ignore = "game tier: a sculpted export of MP_005 with its path data sampled on the stage (needs bin\\bfbc2.exe and \
            MP_005 prepared for NavKit); run_tests.py game runs it"]
// spec: TEDIT-NAV-1
fn sculpted_path_data_end_to_end() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let opfor = dir.parent().unwrap().to_path_buf();
    if !opfor
        .join("work")
        .join("navkit")
        .join("mp_005")
        .join("navkit.json")
        .is_file()
    {
        eprintln!("skipped: MP_005 is not prepared for NavKit");
        return;
    }
    let game = Game::open(&dir).unwrap();
    let tmp = std::env::temp_dir().join(format!("bc2_e5r_path_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let project_file = tmp.join("PathData.mapedit.json");
    let tiles = DirStore::of(&project_file);
    let base = export::level_partition(&game, "mp_005").unwrap();
    let mut p = MapProject::new("E5-rest path data", "mp_005", "levels/MP_005", &base, Profile::Bfbc2);
    let c = export::choose_mode(&game, &p, Profile::Bfbc2, "conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let plain = p.clone();
    let w = bc2_levelwrite::write::LevelWriter::parse(&base).unwrap();
    let hname = export::heightfield_name(&w).unwrap();
    let hb = export::base_terrain_file(&game, "mp_005", &hname).unwrap();
    let hf = terrain::parse(&hb).unwrap();
    let before = Raster::from_samples(hf.size, hf.size, &hf.raw);
    let mut after = before.clone();
    sculpt(&mut after, &hf);
    let dt = bc2_levelwrite::delta::stroke(&before, &after, &tiles).unwrap();
    p.push(
        Op::Stroke {
            target: Target::Terrain { file: hname.clone() },
            size: [hf.size, hf.size],
            tiles: dt,
            also: Vec::new(),
        },
        "Raise",
    );
    p.save(&project_file).unwrap();
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_E5R_PATH".into(),
        folder: String::new(),
        title: "E5-rest path data".into(),
        mode: "Conquest".into(),
        layer: LayerSpec {
            donor: p.mode().unwrap().donor.clone(),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: Profile::Bfbc2,
        identity: None,
        path_data: true,
    };
    let stage = tmp.join("stage");
    let t = std::time::Instant::now();
    let rec = export::build(&game, &p, &project_file.display().to_string(), &spec, &stage).unwrap();
    println!("built with its path data in {:.1} s", t.elapsed().as_secs_f64());
    let pd = rec.path_data.as_ref().unwrap();
    println!("path data: {} ({} files)", pd.source, pd.files.len());
    for f in &pd.files {
        println!("  {} {} bytes", f.path, f.size);
    }
    assert!(pd.source.contains("offline sampler on the stage"), "{}", pd.source);
    let r = export::check(&game, &stage).unwrap();
    for (ok, l) in r
        .checks
        .iter()
        .filter(|c| c.1.contains("path data") || c.1.starts_with("terrain"))
    {
        println!("{} {l}", if *ok { "ok    " } else { "FAILED" });
    }
    // the same Conquest layer without the sculpt, its path data the base's samples built: what fails there is the
    // layer's, not the sculpt's
    let plain_stage = tmp.join("plain");
    let t = std::time::Instant::now();
    let plain_spec = ExportSpec {
        level: "MAPEDIT_E5R_PLAIN".into(),
        ..spec.clone()
    };
    let prec = export::build(&game, &plain, "plain", &plain_spec, &plain_stage).unwrap();
    println!(
        "unsculpted: built in {:.1} s, path data {}",
        t.elapsed().as_secs_f64(),
        prec.path_data.as_ref().map_or("none", |d| d.source.as_str())
    );
    let pr = export::check(&game, &plain_stage).unwrap();
    let failed =
        |r: &export::CheckReport| -> Vec<String> { r.lines().filter(|l| !l.1).map(|l| l.0.to_owned()).collect() };
    for (ok, l) in pr.checks.iter().filter(|c| !c.0) {
        println!("unsculpted: {} {l}", if *ok { "ok    " } else { "FAILED" });
    }
    let (sculpted, unsculpted) = (failed(&r), failed(&pr));
    println!("failed rules: sculpted {sculpted:?}, unsculpted {unsculpted:?}");
    assert!(
        sculpted.iter().all(|c| unsculpted.contains(c)),
        "the sculpt fails a rule the layer passes without it: {:#?}",
        r.checks.iter().filter(|c| !c.0).collect::<Vec<_>>()
    );
    assert!(
        r.checks
            .iter()
            .filter(|c| c.1.contains("pathdata.hpd: the path data recorded"))
            .all(|c| c.0),
        "the staged path data reads"
    );
    let _ = std::fs::remove_dir_all(&tmp);
    // the stage sampler's tree (about 1 GB, plain folders the sampler made)
    let _ = std::fs::remove_dir_all(bc2_levelwrite::pathdata::stage_tree(&opfor, "MAPEDIT_E5R_PATH"));
}
