//! Create Level's new terrain on real data (game tier; rounds E7 and TG3, `spec/transforms/terrain_edit.md`
//! TEDIT-NEW-*, `terrain_gen.md` TGEN-REQ-5 / TGEN-OUT-5):
//!
//! - `dice_heights_through_the_import`: every main height field of `Dist\win32`'s levels, written out as a 16-bit
//!   height map (range 0 to its own) and imported, gives DICE's stored samples, bounds and min / max tree back; one
//!   through a real PNG file too.
//! - `new_level_exports_and_checks`: a level generated from a seed (arid, 1,024, MP_005 the base) and a level from
//!   MP_005's own heights as a PNG are exported as new levels and checked: `export::check` passes but for rules the
//!   base's Conquest layer fails without a new terrain, XVAL-TERR-4 (the level's own terrain bundle, nothing of the
//!   base's terrain, registered alone) and XVAL-LVL-3 (the height field loaded) pass; the imported level's staged main
//!   block holds DICE's heights.
//! - `new_level_path_data`: the same two with the path-data step: the stage sampled offline (`bfbc2 navkit sample
//!   --offline --stage`) on the new terrain and its path data built; the build checks (XVAL-AI-1, the file parsed and
//!   written back byte for byte) and the bots' measures on the playable area (connectivity, coverage); MP_005's own
//!   heights under the new name, with DICE's water mesh, give the path data DICE's terrain under DICE's name gives
//!   (TGEN-OUT-5's canary); with the import's own water (one quad over the square) they differ by the water rule.
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); writes only into a temporary folder and the stage
//! sampler's tree (removed); skipped without a game copy. Nothing is loaded in game (terrain_edit.md section 5 step 7).
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    missing_docs
)]

use std::path::{Path, PathBuf};

use bc2_formats::terrain_codec::{MainBlock, minmax_tree};
use bc2_formats::{fbrb, terrain};
use bc2_levelwrite::delta::{DirStore, Raster};
use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::files::Target;
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::newterrain;
use bc2_levelwrite::project::{GenerateSpec, HeightFormat, KeptFile, TerrainImport};
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

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_newterrain_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A 16-bit greyscale PNG of `vals` (`n` a side).
fn write_png(path: &Path, n: u32, vals: &[u16]) {
    let f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut e = png::Encoder::new(f, n, n);
    e.set_color(png::ColorType::Grayscale);
    e.set_depth(png::BitDepth::Sixteen);
    let data: Vec<u8> = vals.iter().flat_map(|v| v.to_be_bytes()).collect();
    e.write_header().unwrap().write_image_data(&data).unwrap();
}

fn import_of(file: &str, n: u32, range: [f64; 2], water: f64, generate: Option<GenerateSpec>) -> TerrainImport {
    TerrainImport {
        content_set: "BFBC2".into(),
        heightmap: KeptFile {
            file: file.into(),
            source: String::new(),
            size: 0,
            sha1: "0123456789abcdef".into(),
        },
        format: HeightFormat::Png16,
        samples: n,
        sample_range: [0, 65535],
        map_size_m: f64::from(n),
        height_range_m: range,
        water_level_m: water,
        colour_map: None,
        biome: Some("arid".into()),
        ring: false,
        generate,
    }
}

#[test]
#[ignore = "game tier: Dist's main height fields through the height-map import; run_tests.py game runs it"]
fn dice_heights_through_the_import() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let mut seen = std::collections::BTreeSet::new();
    let (mut same, mut all) = (0, 0);
    // DICE's bundles: Dist's levels and Package's (BFBC1, Vietnam), each terrain once; OPFOR's own ports (newer
    // than 2013) left out
    let mut entries: Vec<_> = [
        dir.join("Dist").join("win32").join("levels"),
        dir.join("Package").join("levels"),
    ]
    .iter()
    .filter_map(|l| std::fs::read_dir(l).ok())
    .flat_map(|r| r.flatten())
    .filter(|e| {
        e.path()
            .join("terrain-00.fbrb")
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t < std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_356_998_400))
    })
    .collect();
    entries.sort_by_key(std::fs::DirEntry::path);
    for e in entries {
        let b = e.path().join("terrain-00.fbrb");
        if !b.is_file() {
            continue;
        }
        let h = fbrb::entries_path(&b).unwrap();
        let names: Vec<String> = h
            .entries
            .iter()
            .filter(|x| !x.is_deleted() && x.name.to_ascii_lowercase().ends_with(".heightfield-000.res"))
            .map(|x| x.name.clone())
            .filter(|n| seen.insert(n.to_ascii_lowercase()))
            .collect();
        for (name, data) in names.iter().zip(fbrb::read_path_uncached(&b, &names).unwrap()) {
            let dice = MainBlock::decode(&data).unwrap();
            let hf = terrain::parse(&data).unwrap();
            let range = f64::from(dice.max[1]) - f64::from(dice.min[1]);
            let (g, lift) = newterrain::stored(&dice.heights, [0.0, range], 0.0);
            let mine = MainBlock::new(g.bounds(), g.raw.clone(), None).unwrap();
            all += 1;
            let ok = lift == 0.0
                && mine.heights == dice.heights
                && mine.tree == dice.tree
                && mine.min == dice.min
                && mine.max == dice.max
                && mine.size == hf.size
                && mine.under_share() >= 1.0;
            if ok {
                same += 1;
            } else {
                println!("differs: {name} (bounds {:?} / {:?})", mine.min, dice.min);
            }
        }
    }
    println!("DICE main blocks through the import: {same} of {all} give DICE's samples, bounds and tree");
    assert!(all >= 23, "{all} main blocks found");
    assert_eq!(same, all);
    // one through a real PNG file, read back by the import's reader (1025 a side: the last row and column dropped)
    let game = Game::open(&dir).unwrap();
    let base = export::level_partition(&game, "mp_005").unwrap();
    let w = bc2_levelwrite::write::LevelWriter::parse(&base).unwrap();
    let hname = export::heightfield_name(&w).unwrap();
    let dice = MainBlock::decode(&export::base_terrain_file(&game, "mp_005", &hname).unwrap()).unwrap();
    let n = dice.size as usize;
    let tmp = temp("png");
    let mut big = vec![0u16; (n + 1) * (n + 1)];
    for j in 0..n {
        big[j * (n + 1)..j * (n + 1) + n].copy_from_slice(&dice.heights[j * n..j * n + n]);
    }
    write_png(&tmp.join("MP005.heightmap.png"), (n + 1) as u32, &big);
    let range = f64::from(dice.max[1]);
    let mut t = import_of("MP005.heightmap.png", n as u32, [0.0, range], -1.0, None);
    t.format = HeightFormat::Png16;
    let (terr, notes) = newterrain::from_heightmap(&t, &tmp).unwrap();
    println!("{}", notes.join("\n"));
    assert_eq!(terr.height.raw, dice.heights);
    let res = terr.resources("MP005_COPY").unwrap();
    let back = MainBlock::decode(&res[0].data).unwrap();
    assert_eq!((back.heights == dice.heights, back.tree == dice.tree), (true, true));
    assert!(terr.height.water_y.is_none());
    let _ = std::fs::remove_dir_all(&tmp);
}

fn spec_of(level: &str, p: &MapProject, path_data: bool) -> ExportSpec {
    ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: level.into(),
        folder: String::new(),
        title: level.into(),
        mode: "Conquest".into(),
        layer: LayerSpec {
            donor: p.mode().unwrap().donor.clone(),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: Profile::Bfbc2,
        identity: None,
        path_data,
    }
}

/// A Conquest project on MP_005 with a new terrain: generated (arid 1,024, seed 7) or MP_005's own heights as a PNG.
fn project(game: &Game, folder: &Path, generated: bool) -> (MapProject, PathBuf) {
    let base = export::level_partition(game, "mp_005").unwrap();
    let import = if generated {
        import_of(
            "",
            1024,
            [0.0, 0.0],
            0.0,
            Some(GenerateSpec {
                biome: "arid".into(),
                size: 1024,
                seed: 7,
                border_m: None,
                water_share: None,
                pads: vec![[0.0, 0.0, 30.0]],
                rough: None,
                erosion: None,
                ring: true,
                water_level_m: None,
            }),
        )
    } else {
        let w = bc2_levelwrite::write::LevelWriter::parse(&base).unwrap();
        let hname = export::heightfield_name(&w).unwrap();
        let dice = MainBlock::decode(&export::base_terrain_file(game, "mp_005", &hname).unwrap()).unwrap();
        write_png(&folder.join("Copy.heightmap.png"), dice.size, &dice.heights);
        // MP_005's water: its water mesh's level
        let water = export::ground_of(game, "mp_005", &w)
            .unwrap()
            .1
            .map_or(0.0, |m| f64::from(m.vertices[0][1]));
        import_of(
            "Copy.heightmap.png",
            dice.size,
            [0.0, f64::from(dice.max[1])],
            water,
            None,
        )
    };
    let name = if generated { "Generated" } else { "Copy" };
    let mut p = MapProject::new_level(name, "mp_005", &base, Profile::Bfbc2, import);
    let c = export::choose_mode(game, &p, Profile::Bfbc2, "conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let file = folder.join(p.file_name());
    p.save(&file).unwrap();
    assert_eq!(MapProject::load(&file).unwrap().new_terrain(), p.new_terrain());
    assert_eq!(p.reader_needed(), 13);
    (p, file)
}

fn failed(r: &export::CheckReport) -> Vec<String> {
    r.checks.iter().filter(|c| !c.0).map(|c| c.1.clone()).collect()
}

#[test]
#[ignore = "game tier: two whole staged exports with a new terrain on MP_005; run_tests.py game runs it"]
fn new_level_exports_and_checks() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let tmp = temp("export");
    // the base's Conquest layer without a new terrain: what it fails is the layer's
    let plain = {
        let base = export::level_partition(&game, "mp_005").unwrap();
        let mut p = MapProject::new("Plain", "mp_005", "levels/MP_005", &base, Profile::Bfbc2);
        let c = export::choose_mode(&game, &p, Profile::Bfbc2, "conquest", None).unwrap();
        p.push(Op::SetMode { to: Box::new(c) }, "target");
        let stage = tmp.join("plain");
        export::build(&game, &p, "plain", &spec_of("MAPEDIT_NT_PLAIN", &p, false), &stage).unwrap();
        failed(&export::check(&game, &stage).unwrap())
    };
    println!("the plain layer fails: {plain:?}");
    for generated in [true, false] {
        let (p, file) = project(&game, &tmp, generated);
        let level = if generated { "MAPEDIT_NT_GEN" } else { "MAPEDIT_NT_COPY" };
        let stage = tmp.join(level);
        let t = std::time::Instant::now();
        let rec = export::build(
            &game,
            &p,
            &file.display().to_string(),
            &spec_of(level, &p, false),
            &stage,
        )
        .unwrap();
        println!("{level}: built in {:.1} s", t.elapsed().as_secs_f64());
        for l in rec.report.iter().filter(|l| l.starts_with("new terrain")) {
            println!("  {l}");
        }
        let nt = rec.new_terrain.clone().unwrap();
        assert_eq!(nt.name, level);
        assert_eq!(nt.replaces, "terrains/mp_005/mp_005");
        // registered alone: the terrain block names no bundle of MP_005
        let tb = rec
            .mft_blocks
            .iter()
            .find(|b| b[0].ends_with(&format!("levels/{}/terrain-00.fbrb", level.to_ascii_lowercase())))
            .unwrap();
        println!("  terrain block: {tb:?}");
        assert!(!tb.iter().any(|l| l.contains("levels/mp_005/")));
        let t = std::time::Instant::now();
        let r = export::check(&game, &stage).unwrap();
        println!("{level}: checked in {:.1} s", t.elapsed().as_secs_f64());
        for (ok, l) in r
            .checks
            .iter()
            .filter(|c| !c.0 || c.1.starts_with("new terrain") || c.1.starts_with("terrain"))
        {
            println!("  {} {l}", if *ok { "ok    " } else { "FAILED" });
        }
        let f = failed(&r);
        assert!(
            f.iter()
                .all(|x| plain.iter().any(|y| y.split(':').next() == x.split(':').next())
                    && !x.starts_with("new terrain")
                    && !x.starts_with("terrain:")),
            "the new terrain fails a rule: {f:#?}"
        );
        assert!(
            r.checks.iter().any(|c| c.0 && c.1.starts_with("new terrain:")),
            "XVAL-TERR-4 ran"
        );
        // the staged main block
        let (h, data) = fbrb::extract_path_uncached(stage.join("terrain-00.fbrb")).unwrap();
        let main = h
            .entries
            .iter()
            .zip(&data)
            .find(|(e, _)| {
                e.name
                    .eq_ignore_ascii_case(&format!("Terrains/{level}/{level}.Heightfield-000.res"))
            })
            .map(|(_, d)| MainBlock::decode(d).unwrap())
            .unwrap();
        assert_eq!(main.tree, minmax_tree(&main.heights, main.size as usize));
        if !generated {
            let base = export::level_partition(&game, "mp_005").unwrap();
            let w = bc2_levelwrite::write::LevelWriter::parse(&base).unwrap();
            let hname = export::heightfield_name(&w).unwrap();
            let dice = MainBlock::decode(&export::base_terrain_file(&game, "mp_005", &hname).unwrap()).unwrap();
            assert_eq!(main.heights, dice.heights, "the imported level's heights are DICE's");
            assert_eq!((main.min, main.max), (dice.min, dice.max));
        }
        assert!(
            !h.entries
                .iter()
                .any(|e| newterrain::is_base_terrain_entry(&e.name, "terrains/mp_005/mp_005"))
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// The staged path data's bot measures on the staged level's playable box, the ground from the staged main block.
fn measures(stage: &Path, level: &str, water: f64) -> bc2_levelwrite::pathdata::BotMeasures {
    let hpd_bytes = std::fs::read(stage.join("ai").join("infantry").join("pathdata.hpd")).unwrap();
    let hpd = bc2_formats::hpd::parse(&hpd_bytes).unwrap();
    // the build check: the file written back byte for byte
    assert_eq!(
        bc2_formats::hpd::write(&hpd).unwrap(),
        hpd_bytes,
        "the path data round-trips"
    );
    let routes = bc2_levelwrite::pathdata::routes_of(&hpd, "staged");
    let (h, data) = fbrb::extract_path_uncached(stage.join("level-00.fbrb")).unwrap();
    let lw = h
        .entries
        .iter()
        .zip(&data)
        .find(|(e, _)| e.name.eq_ignore_ascii_case(&format!("Levels/{level}.dbx")))
        .map(|(_, d)| bc2_levelwrite::write::LevelWriter::parse(d).unwrap())
        .unwrap();
    let area = bc2_levelwrite::modelayer::playable_box(&lw).unwrap();
    let (h, data) = fbrb::extract_path_uncached(stage.join("terrain-00.fbrb")).unwrap();
    let pick = |suffix: &str| {
        h.entries
            .iter()
            .zip(&data)
            .find(|(e, _)| e.name.to_ascii_lowercase().ends_with(suffix))
            .map(|(_, d)| d.clone())
    };
    let hf = terrain::parse(&pick(".heightfield-000.res").unwrap()).unwrap();
    let n = hf.size as usize;
    let scale = f64::from(hf.max[1] - hf.min[1]) / 65535.0;
    let at = |i: usize, j: usize| f64::from(hf.min[1]) + f64::from(hf.raw[j.min(n - 1) * n + i.min(n - 1)]) * scale;
    let ground = |x: f64, z: f64| -> Option<(f64, f64, bool)> {
        let (fi, fj) = (x - f64::from(hf.min[0]), z - f64::from(hf.min[2]));
        if fi < 1.0 || fj < 1.0 || fi >= (n - 1) as f64 || fj >= (n - 1) as f64 {
            return None;
        }
        let (i, j) = (fi as usize, fj as usize);
        let y = at(i, j);
        let gx = (at(i + 1, j) - at(i - 1, j)) / 2.0;
        let gz = (at(i, j + 1) - at(i, j - 1)) / 2.0;
        Some((y, gx.hypot(gz).atan().to_degrees(), y < water))
    };
    bc2_levelwrite::pathdata::bot_measures(&routes, area, &ground)
}

#[test]
#[ignore = "game tier: new terrains on MP_005 sampled offline on the stage and built (needs bin\\bfbc2.exe and MP_005 \
            prepared for NavKit); run_tests.py game runs it"]
fn new_level_path_data() {
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
    let tmp = temp("path");
    // MP_005's sea level (its water mesh's 58.0 m; the generator puts its water there too, TGEN-VIS-2)
    let water_name = "terrains/mp_005/mp_005.water.mesh.res";
    let dice_water = export::base_terrain_file(&game, "mp_005", water_name).unwrap();
    let sea = f64::from(bc2_formats::water::parse_mesh(&dice_water).unwrap().vertices[0][1]);
    for generated in [true, false] {
        let (p, file) = project(&game, &tmp, generated);
        let level = if generated {
            "MAPEDIT_NT_GENPD"
        } else {
            "MAPEDIT_NT_COPYPD"
        };
        let stage = tmp.join(level);
        let t = std::time::Instant::now();
        let rec = export::build(
            &game,
            &p,
            &file.display().to_string(),
            &spec_of(level, &p, true),
            &stage,
        )
        .unwrap();
        let pd = rec.path_data.as_ref().unwrap();
        println!(
            "{level}: built with its path data in {:.1} s ({})",
            t.elapsed().as_secs_f64(),
            pd.source
        );
        assert!(pd.source.contains("offline sampler on the stage"), "{}", pd.source);
        let r = export::check(&game, &stage).unwrap();
        for (ok, l) in r
            .checks
            .iter()
            .filter(|c| c.1.contains("path data") || c.1.contains("route"))
        {
            println!("  {} {l}", if *ok { "ok    " } else { "FAILED" });
        }
        assert!(
            r.checks
                .iter()
                .filter(|c| c.1.contains("pathdata.hpd: the path data recorded"))
                .all(|c| c.0),
            "the staged path data reads (XVAL-AI-1)"
        );
        let m = measures(&stage, level, sea);
        println!(
            "  bots: {} route nodes, connectivity {:.3}, {} walkable 8 m cells in the playable box, coverage {:.3}",
            m.nodes, m.connectivity, m.cells, m.coverage
        );
        assert!(m.nodes > 0 && m.connectivity > 0.5 && m.coverage > 0.0);
        let _ = std::fs::remove_dir_all(bc2_levelwrite::pathdata::stage_tree(&opfor, level));
    }
    // TGEN-OUT-5's canary: MP_005's heights under DICE's name, sampled on the stage the same way (an empty stroke
    // makes the export sample its stage), against the imported copy's
    let base = export::level_partition(&game, "mp_005").unwrap();
    let mut p = MapProject::new("Same", "mp_005", "levels/MP_005", &base, Profile::Bfbc2);
    let c = export::choose_mode(&game, &p, Profile::Bfbc2, "conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let w = bc2_levelwrite::write::LevelWriter::parse(&base).unwrap();
    let hname = export::heightfield_name(&w).unwrap();
    let hf = terrain::parse(&export::base_terrain_file(&game, "mp_005", &hname).unwrap()).unwrap();
    let file = tmp.join("Same.mapedit.json");
    let before = Raster::from_samples(hf.size, hf.size, &hf.raw);
    let tiles = bc2_levelwrite::delta::stroke(&before, &before, &DirStore::of(&file)).unwrap();
    p.push(
        Op::Stroke {
            target: Target::Terrain { file: hname },
            size: [hf.size, hf.size],
            tiles,
            also: Vec::new(),
        },
        "none",
    );
    p.save(&file).unwrap();
    let level = "MAPEDIT_NT_SAMEPD";
    let stage = tmp.join(level);
    let rec = export::build(
        &game,
        &p,
        &file.display().to_string(),
        &spec_of(level, &p, true),
        &stage,
    )
    .unwrap();
    println!("{level}: {}", rec.path_data.as_ref().unwrap().source);
    let same = std::fs::read(stage.join("ai").join("infantry").join("pathdata.hpd")).unwrap();
    let m = measures(&stage, level, sea);
    println!(
        "  DICE's terrain: {} route nodes, connectivity {:.3}, {} walkable cells, coverage {:.3}",
        m.nodes, m.connectivity, m.cells, m.coverage
    );
    let _ = std::fs::remove_dir_all(bc2_levelwrite::pathdata::stage_tree(&opfor, level));
    // the imported copy with DICE's water mesh in place of the import's one quad (TGEN-WATER-1: DICE's MP_005 water
    // covers only the sea, at 58.0 and 58.31 m; the quad covers the square, and the build's water rule leaves out
    // ground more than 1.5 m under it), its path data staged from its own stage: the terrain writer alone
    let (cp, cfile) = project(&game, &tmp, false);
    let clevel = "MAPEDIT_NT_CANARY";
    let cstage = tmp.join(clevel);
    export::build(
        &game,
        &cp,
        &cfile.display().to_string(),
        &spec_of(clevel, &cp, false),
        &cstage,
    )
    .unwrap();
    let (h, data) = fbrb::extract_path_uncached(cstage.join("terrain-00.fbrb")).unwrap();
    let ours = format!("terrains/{0}/{0}.water.mesh.res", clevel.to_ascii_lowercase());
    let entries: Vec<export::StagedEntry> = h
        .entries
        .iter()
        .zip(data)
        .filter(|(e, _)| !e.is_deleted())
        .map(|(e, d)| export::StagedEntry {
            name: e.name.clone(),
            type_name: e.type_name.clone(),
            data: if e.name.eq_ignore_ascii_case(&ours) {
                dice_water.clone()
            } else {
                d
            },
        })
        .collect();
    assert!(entries.iter().any(|e| e.name.eq_ignore_ascii_case(&ours)));
    std::fs::write(cstage.join("terrain-00.fbrb"), export::pack_entries(&entries).unwrap()).unwrap();
    let (cur, base_w) = export::current(&game, &cp).unwrap();
    let pd = bc2_levelwrite::pathdata::stage(&opfor, &cp, &cur, &base_w, &cstage, clevel).unwrap();
    println!("{clevel}: {}", pd.source);
    let canary = std::fs::read(cstage.join("ai").join("infantry").join("pathdata.hpd")).unwrap();
    let _ = std::fs::remove_dir_all(bc2_levelwrite::pathdata::stage_tree(&opfor, clevel));
    let (a, b) = (
        bc2_formats::hpd::parse(&canary).unwrap(),
        bc2_formats::hpd::parse(&same).unwrap(),
    );
    let geo = |f: &bc2_formats::hpd::HpdFile<'_>| -> Vec<(Vec<[f32; 3]>, usize, Vec<u8>)> {
        f.sectors
            .iter()
            .map(|s| (s.nodes.iter().map(|n| n.pos).collect(), s.edges.len(), s.mesh.to_vec()))
            .collect()
    };
    let equal = geo(&a) == geo(&b);
    println!(
        "canary: the imported copy's path data {} DICE's terrain's (sectors {} / {}, nodes {} / {})",
        if equal { "equals" } else { "DIFFERS from" },
        a.sectors.len(),
        b.sectors.len(),
        a.sectors.iter().map(|s| s.nodes.len()).sum::<usize>(),
        b.sectors.iter().map(|s| s.nodes.len()).sum::<usize>()
    );
    assert!(equal, "the new-terrain export perturbs the path data");
    let _ = std::fs::remove_dir_all(&tmp);
}
