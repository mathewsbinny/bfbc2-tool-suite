//! Round TG4: vegetation, rocks and props on a generated terrain (spec `transforms/terrain_gen.md` TGEN-VEG-*).
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); writes only into a temporary folder; skipped without
//! a game copy. Nothing is loaded in game.
#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    missing_docs
)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bc2_formats::{fbrb, terrain};
use bc2_levelwrite::delta::{DirStore, Raster};
use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::files::{FileEdit, Sources, Target};
use bc2_levelwrite::members::{self, Member};
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::newterrain;
use bc2_levelwrite::project::{GenerateSpec, HeightFormat, KeptFile, TerrainImport, VegetationSpec};
use bc2_levelwrite::vegetation::{self, LevelGround};
use bc2_levelwrite::{LevelWriter, MapProject, Op, Profile};
use bc2_terragen::Biome;
use bc2_terragen::vegetation::{BiomeStats, Kind, measure};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

/// DICE's vegetation statistics per biome (TGEN-VEG-2): measured on the game copy's multiplayer terrains, printed,
/// and held equal to the table `bc2_terragen::vegetation::BiomeStats::dice` keeps (`BC2_VEG_TABLE=<file>` writes the
/// table's source there instead).
#[test]
#[ignore = "game tier: DICE's multiplayer terrains' vegetation; run_tests.py game runs it"]
fn dice_biome_statistics() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let t0 = std::time::Instant::now();
    let all = vegetation::dice_terrain_stats(&game).unwrap();
    println!("{} terrains measured in {:.1} s", all.len(), t0.elapsed().as_secs_f64());
    let mut src = String::new();
    for b in Biome::ALL {
        let of: Vec<&vegetation::TerrainStats> = all.iter().filter(|t| t.biome == b).collect();
        let names: Vec<String> = of.iter().map(|t| format!("{} ({})", t.terrain, t.level)).collect();
        println!("== {} : {}", b.name(), names.join(", "));
        for t in &of {
            for k in Kind::ALL {
                if let Some(s) = &t.stats.kinds[k.index()] {
                    println!(
                        "  {:10} {:12} n {:5} types {} bands {:.1?} nn {:.1?} ce {:.2} k {:.2?} slope {:.2?} height {:.2?} tilt {:.1?}/{:.1} sink {:.2?}",
                        t.terrain,
                        k.name(),
                        s.count,
                        s.types,
                        s.band_density,
                        s.nn,
                        s.clark_evans,
                        s.k_ratio,
                        s.slope_pref,
                        s.height_pref,
                        s.tilt,
                        s.tilt_normal,
                        s.sink
                    );
                }
            }
        }
        let stats = BiomeStats::of(&of.iter().map(|t| t.stats.clone()).collect::<Vec<_>>()).rounded();
        src.push_str(&format!(
            "        Biome::{b:?} => {stats:?},
"
        ));
        let kept = BiomeStats::dice(b);
        if std::env::var_os("BC2_VEG_TABLE").is_none() {
            assert_eq!(
                stats,
                kept,
                "{}: DICE's statistics measured again differ from the kept table",
                b.name()
            );
        }
    }
    if let Some(p) = std::env::var_os("BC2_VEG_TABLE") {
        std::fs::write(p, src).unwrap();
    }
}

/// A level's group files and partition, as the member edits' sources.
struct GroupSources(HashMap<String, Vec<u8>>);

impl Sources for GroupSources {
    fn file(&self, t: &Target) -> Option<Vec<u8>> {
        match t {
            Target::Group { file } => self.0.get(file).cloned(),
            _ => None,
        }
    }
}

/// TGEN-VEG-4: DICE's forest, middlegrowth and static groups written again through the member writer - every type's
/// members but its first deleted, then added back at DICE's transforms in DICE's order (round SG1's delete and add,
/// as the vegetation step's edits use them), each added member given DICE's leaf - give DICE's group file and
/// partition byte for byte, and the group's physics laid out again for the result is DICE's.
#[test]
#[ignore = "game tier: DICE's model groups of one multiplayer level per biome through the member writer"]
fn dice_groups_round_trip_through_the_member_writer() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let (mut groups, mut members_n, mut same) = (0usize, 0usize, 0usize);
    for lv in ["mp_005", "mp_009cq", "mp_007", "mp_004", "nam_mp_003cq"] {
        let l = LevelGround::read(&game, lv).unwrap();
        let base_part = export::level_partition(&game, lv).unwrap();
        let live = game.live(&game.loaded(lv, "level")).unwrap();
        for g in &l.groups {
            let name = format!("levels/{lv}/{}", g.file);
            let f = &live[&name];
            let dice_file = export::read_found(&[f]).unwrap().pop().unwrap();
            let res_name = format!("{}.res", name.trim_end_matches(".dbx"));
            let dice_res = live
                .get(&res_name)
                .map(|f| export::read_found(&[f]).unwrap().pop().unwrap());
            let dice_file_w = LevelWriter::parse(&dice_file).unwrap();
            // the edits: every type's members after its first added again at the end of the type's run (each a copy
            // of itself: its asset item and record), then the originals deleted - as the vegetation step adds a copy
            // of the DICE instance whose pose it took before deleting
            let mut edits = Vec::new();
            let mut added = Vec::new();
            let mut next = 0u32;
            for &(first, count) in &g.runs {
                for k in first + 1..first + count {
                    next += 1;
                    edits.push(FileEdit::MemberAdd {
                        file: g.file.clone(),
                        instance: g.instance.clone(),
                        item: Member::Added { added: next },
                        like: Member::Base(k),
                        rows: g.members[k].1,
                    });
                    added.push(k);
                }
            }
            for &(first, count) in &g.runs {
                for k in first + 1..first + count {
                    edits.push(FileEdit::MemberDelete {
                        file: g.file.clone(),
                        instance: g.instance.clone(),
                        item: Member::Base(k),
                        asset: g.members[k].0.clone(),
                        rows: g.members[k].1,
                    });
                }
            }
            let mut p = MapProject::new(
                "RoundTrip",
                lv,
                &format!("levels/{}", lv.to_uppercase()),
                &base_part,
                Profile::Bfbc2,
            );
            p.push(Op::Files { edits }, "round trip");
            let w0 = LevelWriter::parse(&base_part).unwrap();
            let schema = bc2_levelwrite::Schema::scan([&w0.d]);
            let src = GroupSources(HashMap::from([(g.file.clone(), dice_file.clone())]));
            let all =
                bc2_levelwrite::files::apply_files(&base_part, &p.operations, &schema, &|_: &str| None, &src).unwrap();
            let mut w = LevelWriter::parse(&all.partition.bytes).unwrap();
            let mut f = LevelWriter::parse(&all.files[&Target::group(&g.file)]).unwrap();
            // each added member's leaf: DICE's (the export gives new members leaves; DICE's are the ones to compare)
            let dice_enums: Vec<i64> = (0..g.members.len())
                .map(|k| members::enum_at(&dice_file_w, &g.instance, k).unwrap_or(0))
                .collect();
            for k in &added {
                members::set_enum(&mut f, &mut w, &g.file, &g.instance, *k, dice_enums[*k]).unwrap();
            }
            let (fb, wb) = (f.write().unwrap(), w.write().unwrap());
            let file_same = fb == dice_file;
            let part_same = wb == base_part;
            let ge = members::group_entity(&w, &g.file).unwrap();
            let counts: Vec<usize> = members::runs(&w, &ge).unwrap().iter().map(|r| r.count).collect();
            let res_same = dice_res.as_ref().is_none_or(|r| {
                let phys = members::GroupPhysics::read(r).unwrap();
                phys.relaid(&counts, &members::layout(&w, &ge).unwrap())
                    .unwrap()
                    .write()
                    == *r
            });
            println!(
                "{lv} {}: {} members, {} re-added: group file {}, partition {}, physics {}",
                g.file,
                g.members.len(),
                added.len(),
                if file_same { "DICE's bytes" } else { "DIFFERS" },
                if part_same { "DICE's bytes" } else { "DIFFERS" },
                if res_same { "DICE's bytes" } else { "DIFFERS" }
            );
            if !part_same {
                // which records of a type differ besides their leaf
                let ge0 = members::group_entity(&w0, &g.file).unwrap();
                let mut odd: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
                for t in &ge0.types {
                    let items =
                        w0.d.child(*t, b"SpawnDatas")
                            .and_then(|a| w0.d.children(a))
                            .unwrap_or_default()
                            .to_vec();
                    let sig = |n: bc2_formats::dbx::edit::NodeId| -> Vec<(String, Vec<u8>)> {
                        w0.d.walk(n)
                            .into_iter()
                            .filter(|&c| c != n)
                            .map(|c| {
                                let name =
                                    String::from_utf8_lossy(w0.d.attr(c, b"name").unwrap_or_default()).into_owned();
                                let v = [w0.d.attr(c, b"ref").unwrap_or_default(), w0.d.raw(c)].concat();
                                (name, v)
                            })
                            .filter(|(nm, _)| nm != "Enumeration")
                            .collect()
                    };
                    if let Some(f0) = items.first() {
                        let a = sig(*f0);
                        for it in &items[1..] {
                            let b = sig(*it);
                            for (x, y) in a.iter().zip(&b) {
                                if x != y {
                                    *odd.entry(x.0.clone()).or_default() += 1;
                                }
                            }
                        }
                    }
                }
                println!("  records differing from their type's first, by field: {odd:?}");
                let k = wb
                    .iter()
                    .zip(&base_part)
                    .position(|(a, b)| a != b)
                    .unwrap_or(wb.len().min(base_part.len()));
                let show =
                    |b: &[u8]| String::from_utf8_lossy(&b[k.saturating_sub(120)..(k + 120).min(b.len())]).into_owned();
                println!(
                    "  first difference at {k} of {} / {}:
  ours: {:?}
  dice: {:?}",
                    wb.len(),
                    base_part.len(),
                    show(&wb),
                    show(&base_part)
                );
            }
            assert!(file_same && part_same && res_same, "{lv} {}", g.file);
            groups += 1;
            members_n += g.members.len();
            same += usize::from(file_same && part_same && res_same);
        }
    }
    println!("{same} of {groups} groups ({members_n} members) round-trip byte for byte");
    assert!(groups >= 15);
}

/// A Conquest project on MP_005 with a generated terrain (arid 1,024, seed 7, a pad), its vegetation placed (TG4),
/// a sculpt stroke and a visual paint stroke made on the new terrain as the editor makes them.
fn veg_project(game: &Game, folder: &Path, strokes: bool) -> (MapProject, PathBuf) {
    let base = export::level_partition(game, "mp_005").unwrap();
    let import = TerrainImport {
        content_set: "BFBC2".into(),
        heightmap: KeptFile {
            file: String::new(),
            source: String::new(),
            size: 0,
            sha1: String::new(),
        },
        format: HeightFormat::Png16,
        samples: 1024,
        sample_range: [0, 0],
        map_size_m: 1024.0,
        height_range_m: [0.0, 0.0],
        water_level_m: 0.0,
        colour_map: None,
        generate: Some(GenerateSpec {
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
        biome: Some("arid".into()),
        ring: true,
        vegetation: Some(VegetationSpec::default()),
        strokes_on_base: false,
    };
    let mut p = MapProject::new_level("Vegetated", "mp_005", &base, Profile::Bfbc2, import.clone());
    let c = export::choose_mode(game, &p, Profile::Bfbc2, "conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let file = folder.join(p.file_name());
    if strokes {
        // the new terrain as the editor shows it (under the base's names)
        let view = newterrain::view(game, "mp_005", &import, &file).unwrap();
        let hname = format!("{}.heightfield-000.res", view.stem);
        let hf = terrain::parse(&view.files[&hname]).unwrap();
        let n = hf.size;
        let before = Raster::from_samples(n, n, &hf.raw);
        let mut after = before.clone();
        // a 15 m hill of radius 40 m at (100, 100)
        let per_m = 65535.0 / f64::from(hf.max[1]);
        for j in 0..n {
            for i in 0..n {
                let (x, z) = (
                    f64::from(i) - f64::from(n) / 2.0 - 100.0,
                    f64::from(j) - f64::from(n) / 2.0 - 100.0,
                );
                let d = x.hypot(z);
                if d < 40.0 {
                    let up = 15.0 * (1.0 - d / 40.0) * per_m;
                    after.set(i, j, (f64::from(before.get(i, j)) + up).min(65535.0) as u16);
                }
            }
        }
        let store = DirStore::of(&file);
        let tiles = bc2_levelwrite::delta::stroke(&before, &after, &store).unwrap();
        p.push(
            Op::Stroke {
                target: Target::Terrain { file: hname },
                size: [n, n],
                tiles,
                also: Vec::new(),
            },
            "Raise",
        );
        // a visual paint stroke of a paintable layer on one 16 m cell whose layers with it have a shader of the
        // donor's (TEDIT-SH-1 / -2: a combination with none to clone is refused, as on DICE's own terrain)
        let vt = bc2_formats::visual_terrain::VisualTerrain::decode(&view.files[&format!("{}.visual.res", view.stem)])
            .unwrap();
        let model = bc2_levelwrite::terrainpaint::LayerModel::of(&vt).unwrap();
        let map = &vt.maps[0];
        let w = map.header[4] as usize;
        let q = n as usize / w;
        let mut pick = None;
        'find: for layer in model.paintable() {
            let l = model.index(layer).unwrap();
            for k in (w * w / 3)..(w * w) {
                let mut want: std::collections::BTreeSet<usize> =
                    model.shaders[usize::from(map.shader[k])].iter().copied().collect();
                if !want.insert(l) {
                    continue;
                }
                if model.find(&want).is_some() {
                    pick = Some((layer.to_owned(), k));
                    break 'find;
                }
            }
        }
        let (layer, k) = pick.unwrap();
        println!("  the paint stroke: {layer} on cell {k}");
        let empty = Raster::new(n, n, 0);
        let mut paint = empty.clone();
        let (cx, cz) = ((k % w) * q, (k / w) * q);
        for j in 0..q {
            for i in 0..q {
                paint.set((cx + i) as u32, (cz + j) as u32, 255);
            }
        }
        let tiles = bc2_levelwrite::delta::stroke(&empty, &paint, &store).unwrap();
        p.push(
            Op::Stroke {
                target: Target::Paint { layer },
                size: [n, n],
                tiles,
                also: Vec::new(),
            },
            "Paint",
        );
    }
    p.save(&file).unwrap();
    assert_eq!(MapProject::load(&file).unwrap().new_terrain(), p.new_terrain());
    assert_eq!(p.reader_needed(), 14);
    (p, file)
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

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_vegetation_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The staged level's placed members of the four kinds on its staged ground (the statistics' instances), and the
/// level's combat areas.
fn staged_vegetation(stage: &Path, level: &str) -> (LevelGround, Vec<Vec<[f64; 2]>>) {
    let (h, data) = fbrb::extract_path_uncached(stage.join("level-00.fbrb")).unwrap();
    let files: HashMap<String, Vec<u8>> = h
        .entries
        .iter()
        .zip(data)
        .map(|(e, d)| (e.name.to_ascii_lowercase(), d))
        .collect();
    let low = level.to_ascii_lowercase();
    let w = LevelWriter::parse(&files[&format!("levels/{low}.dbx")]).unwrap();
    let gs = vegetation::groups(&w, &|n| files.get(&format!("levels/{low}/{n}")).cloned()).unwrap();
    let (h, data) = fbrb::extract_path_uncached(stage.join("terrain-00.fbrb")).unwrap();
    let pick = |suffix: &str| {
        h.entries
            .iter()
            .zip(&data)
            .find(|(e, _)| e.name.to_ascii_lowercase().ends_with(suffix))
            .map(|(_, d)| d.clone())
    };
    let heights = terrain::parse(&pick(".heightfield-000.res").unwrap()).unwrap();
    let water = pick(".water.mesh.res")
        .and_then(|m| bc2_formats::water::parse_mesh(&m).ok())
        .map(|m| f64::from(m.vertices[0][1]));
    let polys = bc2_levelwrite::validate::combat_outlines(&w);
    (
        LevelGround {
            level: low,
            terrain: String::new(),
            partition: w,
            groups: gs,
            heights,
            water,
        },
        polys,
    )
}

/// TGEN-VEG-1 - 4, TEDIT-NEW-4: a generated level with its vegetation placed, a sculpt and a visual paint made on its
/// new terrain, exported and checked: `export::check` passes every rule the base's Conquest layer passes (the group
/// lines: each group's members as its types count them, leaves in the client tree, the physics laid out), the
/// staged vegetation measures on DICE's arid statistics, and the stroke's hill is in the staged main block.
#[test]
#[ignore = "game tier: a whole staged export of a generated level with vegetation on MP_005"]
fn a_generated_level_with_vegetation_exports_and_checks() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let tmp = temp("export");
    let (p, file) = veg_project(&game, &tmp, true);
    let level = "MAPEDIT_TG4_VEG";
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
    for l in rec
        .report
        .iter()
        .filter(|l| l.starts_with("new terrain") || l.contains("group"))
    {
        println!("  {l}");
    }
    assert!(
        rec.report
            .iter()
            .any(|l| l.contains("vegetation:") && l.contains("forest"))
    );
    let t = std::time::Instant::now();
    let r = export::check(&game, &stage).unwrap();
    println!("{level}: checked in {:.1} s", t.elapsed().as_secs_f64());
    for (ok, l) in r.checks.iter() {
        if !*ok || l.contains("group") || l.starts_with("new terrain") || l.starts_with("terrain") {
            println!(
                "  {} {}",
                if *ok { "ok    " } else { "FAILED" },
                l.chars().take(400).collect::<String>()
            );
        }
    }
    // what the base's Conquest layer fails without a new terrain (XVAL-PLACE-5's points: the layer's, TG3)
    let allowed = ["XVAL-PLACE"];
    let failed: Vec<&String> = r.checks.iter().filter(|c| !c.0).map(|c| &c.1).collect();
    assert!(
        failed
            .iter()
            .all(|f| allowed.iter().any(|a| f.contains(a)) || f.contains("points off")),
        "{failed:#?}"
    );
    // the staged vegetation on DICE's arid statistics
    let (lg, polys) = staged_vegetation(&stage, level);
    let g = lg.ground();
    let outside = vegetation::outside_fn(polys, g.side());
    let m = measure(&g, &lg.instances(), &outside);
    let dice = BiomeStats::dice(Biome::Arid);
    for k in Kind::ALL {
        let (Some((_, d)), Some(s)) = (&dice.kinds[k.index()], &m.kinds[k.index()]) else {
            continue;
        };
        println!(
            "  {:12} n {:5} inside {:.1} / {:.1} per ha, nn p50 {:.1} / {:.1} m, k10 {:.2} / {:.2}, slope p50 {:.1} / {:.1}, tilt p50 {:.1} / {:.1}, sink p50 {:.2} / {:.2}",
            k.name(),
            s.count,
            s.band_density[0],
            d.band_density[0],
            s.nn[1],
            d.nn[1],
            s.k_ratio[1],
            d.k_ratio[1],
            s.slope[1],
            d.slope[1],
            s.tilt[0],
            d.tilt[0],
            s.sink[1],
            d.sink[1]
        );
        assert!(
            (s.band_density[0] - d.band_density[0]).abs() / d.band_density[0].max(1e-9) < 0.15,
            "{k:?} density"
        );
    }
    // the sculpt's hill on the staged ground: 15 m over the generated ground at (100, 100)
    let view = newterrain::view(&game, "mp_005", p.new_terrain().unwrap(), &file).unwrap();
    let generated = terrain::parse(&view.files[&format!("{}.heightfield-000.res", view.stem)]).unwrap();
    let rise = lg.heights.height(100.0, 100.0) - generated.height(100.0, 100.0);
    println!("  the stroke's hill: {rise:.2} m");
    assert!((rise - 15.0).abs() < 0.5, "{rise}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// TGEN-VEG-3: the generated level with its vegetation in place sampled offline on the stage and its path data built
/// (needs `bin\bfbc2.exe` and MP_005 prepared for NavKit); its bots' measures against the same level without
/// vegetation.
#[test]
#[ignore = "game tier: new terrains with and without vegetation sampled offline on the stage and built"]
fn a_generated_level_with_vegetation_has_path_data() {
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
    for veg in [true, false] {
        let (mut p, file) = veg_project(&game, &tmp, false);
        if !veg {
            p.terrain_import.as_mut().unwrap().vegetation = None;
            p.save(&file).unwrap();
        }
        let level = if veg { "MAPEDIT_TG4_VEGPD" } else { "MAPEDIT_TG4_BAREPD" };
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
        let r = export::check(&game, &stage).unwrap();
        assert!(
            r.checks
                .iter()
                .filter(|c| c.1.contains("pathdata.hpd: the path data recorded"))
                .all(|c| c.0),
            "the staged path data reads (XVAL-AI-1)"
        );
        let hpd_bytes = std::fs::read(stage.join("ai").join("infantry").join("pathdata.hpd")).unwrap();
        let hpd = bc2_formats::hpd::parse(&hpd_bytes).unwrap();
        assert_eq!(
            bc2_formats::hpd::write(&hpd).unwrap(),
            hpd_bytes,
            "the path data round-trips"
        );
        let routes = bc2_levelwrite::pathdata::routes_of(&hpd, "staged");
        let (lg, _) = staged_vegetation(&stage, level);
        let area = bc2_levelwrite::modelayer::playable_box(&lg.partition).unwrap();
        let g = lg.ground();
        let ground = |x: f64, z: f64| -> Option<(f64, f64, bool)> {
            let half = g.side() / 2.0 - 2.0;
            if x.abs() >= half || z.abs() >= half {
                return None;
            }
            let y = g.height(x, z);
            Some((y, g.slope(x, z), g.water.is_some_and(|w| y < w)))
        };
        let m = bc2_levelwrite::pathdata::bot_measures(&routes, area, &ground);
        println!(
            "  bots: {} route nodes, connectivity {:.3}, {} walkable 8 m cells in the playable box, coverage {:.3}",
            m.nodes, m.connectivity, m.cells, m.coverage
        );
        assert!(m.nodes > 0 && m.connectivity > 0.5 && m.coverage > 0.0);
        let _ = std::fs::remove_dir_all(bc2_levelwrite::pathdata::stage_tree(&opfor, level));
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
