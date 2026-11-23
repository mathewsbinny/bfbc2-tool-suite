//! The generated terrain's visual side (TGEN-VIS-1 / -3 / -4, TGEN-WATER-1, TGEN-OCC-1) from DICE's MP_005, and the
//! cell rule against DICE's own choices (game tier: `cargo test --release -p bc2-terragen --test visual --
//! --ignored --nocapture`; passes with a note without the game copy).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    missing_docs
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bc2_formats::shaderdb::open;
use bc2_formats::visual_terrain::VisualTerrain;
use bc2_formats::{fbrb, itexture, terrain, water};
use bc2_terragen::visual::{self, Donor, RuleKind};
use bc2_terragen::{Biome, Settings, Terrain};

fn game() -> Option<PathBuf> {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    let g = root.join("game");
    g.join("Dist").join("win32").join("levels").is_dir().then_some(g)
}

/// Entries of a level's bundle by a test on (name, type): name -> data.
fn read(g: &Path, level: &str, bundle: &str, keep: impl Fn(&str, &str) -> bool) -> BTreeMap<String, Vec<u8>> {
    let p = g.join("Dist").join("win32").join("levels").join(level).join(bundle);
    let h = fbrb::entries_path(&p).unwrap();
    let names: Vec<String> = h
        .entries
        .iter()
        .filter(|e| !e.is_deleted() && keep(&e.name, &e.type_name))
        .map(|e| e.name.clone())
        .collect();
    names
        .iter()
        .cloned()
        .zip(fbrb::read_path_uncached(&p, &names).unwrap())
        .collect()
}

fn one(m: &BTreeMap<String, Vec<u8>>, suffix: &str) -> Vec<u8> {
    m.iter()
        .find(|(k, _)| k.to_ascii_lowercase().ends_with(suffix))
        .unwrap()
        .1
        .clone()
}

// spec: TGEN-VIS-1, TGEN-VIS-3, TGEN-VIS-4, TGEN-WATER-1, TGEN-OCC-1, TGEN-REQ-4
#[test]
#[ignore = "game tier: reads the game copy (run_tests.py game)"]
fn a_generated_terrain_draws_with_mp_005s_shaders() {
    let Some(g) = game() else {
        println!("no game copy: skipped");
        return;
    };
    let terr = read(&g, "mp_005", "terrain-00.fbrb", |n, _| {
        n.to_ascii_lowercase().starts_with("terrains/mp_005/")
    });
    let lvl = read(&g, "mp_005", "level-00.fbrb", |n, t| {
        let low = n.to_ascii_lowercase();
        low.ends_with("mp05_ambientocclusion.res") || t.ends_with("ShaderDatabase")
    });
    let vt = one(&terr, ".visual.res");
    let vw = one(&terr, ".water.res");
    let atlas = one(&terr, ".masktileatlas.res");
    let occ = one(&lvl, "mp05_ambientocclusion.res");
    let dbs: Vec<(u32, Vec<u8>)> = [9u32, 10, 11]
        .iter()
        .map(|&dx| (dx, one(&lvl, &format!("dx{dx}_single.res"))))
        .collect();
    let donor = Donor {
        terrain: "MP_005".into(),
        visual: &vt,
        water: Some(&vw),
        atlas: Some(&atlas),
        occlusion: Some(("Terrains/MP_005/Textures/mp05_AmbientOcclusion".into(), &occ)),
        databases: dbs.iter().map(|(d, b)| (*d, b.as_slice())).collect(),
    };
    let t = Terrain::new(&Settings::new(Biome::Arid, 1024, 7)).unwrap();
    let start = std::time::Instant::now();
    let v = visual::build(&t, "TG_VIS", &donor).unwrap();
    println!("built in {:.2} s", start.elapsed().as_secs_f64());
    for n in &v.notes {
        println!("{n}");
    }
    let names: Vec<&str> = v.terrain.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Terrains/TG_VIS/TG_VIS.Visual.res",
            "Terrains/TG_VIS/TG_VIS.MaskTileAtlas.res",
            "Terrains/TG_VIS/TG_VIS.Water.res"
        ]
    );
    // TGEN-VIS-1: N = 13, the donor's 50 shaders renamed, rule-only cells with no tiles or flags, the donor's atlas
    // layout; the ring's maps 4 x 4 of the plain rule-only shader
    let got = VisualTerrain::decode(&v.terrain[0].data).unwrap();
    let donor_vt = VisualTerrain::decode(&vt).unwrap();
    assert_eq!(got.maps.len(), 13);
    assert_eq!(got.shaders.len(), donor_vt.shaders.len());
    assert!(
        got.shaders
            .iter()
            .all(|s| s.name.starts_with("Terrains/TG_VIS/TG_VIS/PatchMaskShaders/"))
    );
    assert!(
        got.shaders
            .iter()
            .all(|s| s.undergrowth.is_empty() || s.undergrowth.starts_with("Terrains/TG_VIS/TG_VIS/UgMaskShaders/"))
    );
    let main = &got.maps[0];
    assert_eq!(main.header[..4], donor_vt.maps[0].header[..4]);
    assert_eq!((main.header[4], main.shader_side), (64, 64));
    assert_eq!(
        (main.heightfield.as_str(), main.atlas.as_str()),
        (
            "Terrains/TG_VIS/TG_VIS.Heightfield-000",
            "Terrains/TG_VIS/TG_VIS.MaskTileAtlas"
        )
    );
    assert!(main.shader.iter().all(|&s| got.shaders[usize::from(s)].tiled == 0));
    assert!(main.tiles.iter().all(|x| *x == [0; 4]) && main.flags.iter().all(|x| *x == [0; 4]));
    let used: BTreeSet<u8> = main.shader.iter().copied().collect();
    assert!(used.len() >= 3, "{used:?}");
    for (k, m) in got.maps.iter().enumerate().skip(1) {
        assert_eq!(m.header[4], 4);
        assert_eq!(m.heightfield, format!("Terrains/TG_VIS/TG_VIS.Heightfield-{k:03}"));
    }
    // TGEN-VIS-3: a cell under water draws a sea-floor shader, a steep one a slope shader
    let scale = t.height.range_m / 65535.0;
    let h: Vec<f64> = t.height.raw.iter().map(|x| f64::from(*x) * scale).collect();
    let wants = visual::cell_wants(&h, 1024, 1.0, 64, t.height.water_y);
    let agree = wants
        .iter()
        .zip(&main.shader)
        .filter(|(w, s)| {
            let k = visual::rule_kind(&got.shaders[usize::from(**s)].name);
            k.sea == w.sea && k.slope == w.slope
        })
        .count();
    assert_eq!(agree, wants.len(), "MP_005 has all four rule-only combinations");
    // TGEN-WATER-1: the visual water's indices are the water mesh's (WATER-VIS-4), its slot the donor's renamed
    let vwf = water::VisualWaterFile::decode(&v.terrain[2].data).unwrap();
    let mesh = water::parse_mesh(&t.water_mesh().unwrap()).unwrap();
    let parsed = water::parse_visual(&v.terrain[2].data).unwrap();
    let per = water::triangle_shaders(&mesh, &parsed).unwrap();
    assert_eq!(per, vec!["Terrains/TG_VIS/Shaders/Water".to_owned(); 2]);
    assert_eq!(vwf.slots.len(), 4);
    assert!(vwf.slots.iter().any(|s| s.0 == "Shaders/WaterOcean"));
    assert_eq!(mesh.vertices[0][0], -1024.0, "the water covers the ring");
    // TGEN-OCC-1: the colour / occlusion map in the donor's format and layout, its red the donor's distribution
    let occ_res = v
        .level
        .iter()
        .find(|r| r.name == "Terrains/TG_VIS/Textures/TG_VIS_AmbientOcclusion.res")
        .unwrap();
    let (hd, hg) = (
        itexture::Header::read(&occ).unwrap(),
        itexture::Header::read(&occ_res.data).unwrap(),
    );
    assert_eq!(
        (hd.format, hd.flags, hd.width, hd.height, hd.levels, hd.sizes),
        (hg.format, hg.flags, hg.width, hg.height, hg.levels, hg.sizes)
    );
    let pct = |d: &[u8]| {
        let p = itexture::decode_level(d, 0).unwrap();
        let mut r: Vec<u8> = p.rgb.iter().map(|x| x[0]).collect();
        r.sort_unstable();
        [10, 50, 90].map(|q| r[r.len() * q / 100])
    };
    let (pd, pg) = (pct(&occ), pct(&occ_res.data));
    println!("occlusion red p10 / p50 / p90: DICE {pd:?}, generated {pg:?}");
    assert!(pd.iter().zip(&pg).all(|(a, b)| a.abs_diff(*b) <= 4), "{pd:?} {pg:?}");
    // TGEN-VIS-4: each database holds every entry the visual terrain and water name, with the map renamed
    for r in v.level.iter().filter(|r| r.type_name.ends_with("ShaderDatabase")) {
        let dx: u32 = r.name.rsplit_once("Dx").unwrap().1[..2]
            .trim_end_matches('_')
            .parse()
            .unwrap();
        let db = open::open(&r.data, Some(if dx == 11 { 10 } else { dx }), Some(false), "out")
            .unwrap()
            .db;
        let have: BTreeSet<String> = db
            .entries
            .iter()
            .map(|e| String::from_utf8_lossy(&e.name).into_owned())
            .collect();
        let mut want: BTreeSet<String> = got.shaders.iter().map(|s| s.name.clone()).collect();
        want.extend(
            got.shaders
                .iter()
                .filter(|s| !s.undergrowth.is_empty())
                .map(|s| s.undergrowth.clone()),
        );
        want.extend(
            got.shaders
                .iter()
                .flat_map(|s| s.groups.iter().flatten().map(|m| m.shader.clone()))
                .filter(|x| !x.is_empty()),
        );
        want.insert("Terrains/TG_VIS/Shaders/Water".into());
        let missing: Vec<&String> = want.iter().filter(|w| !have.contains(*w)).collect();
        assert!(missing.is_empty(), "{}: {missing:?}", r.name);
    }
    let _ = terrain::parse(&t.resources("TG_VIS").unwrap()[0].data).unwrap();
}

// spec: TGEN-VIS-3
#[test]
#[ignore = "game tier: reads the game copy (run_tests.py game)"]
fn the_cell_rule_against_dices_own_cells() {
    let Some(g) = game() else {
        println!("no game copy: skipped");
        return;
    };
    // (level, terrain): DICE's multiplayer terrains whose rule-only shaders have a sea-floor variant
    let mut sea_total = (0usize, 0usize);
    for (lv, tn) in [
        ("mp_005", "MP_005"),
        ("mp_001", "MP_001"),
        ("mp_009cq", "MP_009"),
        ("mp_003", "MP_003"),
        ("mp_004", "MP_004"),
    ] {
        let low = format!("terrains/{}/", tn.to_ascii_lowercase());
        let terr = read(&g, lv, "terrain-00.fbrb", |n, _| {
            n.to_ascii_lowercase().starts_with(&low)
        });
        if terr.is_empty() {
            println!("{lv}: no terrain bundle of {tn}");
            continue;
        }
        let vt = VisualTerrain::decode(&one(&terr, ".visual.res")).unwrap();
        let hf = terrain::parse(&one(&terr, ".heightfield-000.res")).unwrap();
        let wm = water::parse_mesh(&one(&terr, ".water.mesh.res")).unwrap();
        let mut ys: Vec<f32> = wm.vertices.iter().map(|v| v[1]).collect();
        ys.sort_by(f32::total_cmp);
        let wy = f64::from(ys[ys.len() / 2]);
        let n = hf.size as usize;
        let h: Vec<f64> = hf.heights().iter().map(|x| f64::from(*x)).collect();
        let main = &vt.maps[0];
        let side = main.header[4] as usize;
        let wants = visual::cell_wants(&h, n, 1.0, side, Some(wy));
        let mut usage = vec![0usize; vt.shaders.len()];
        for &s in &main.shader {
            usage[usize::from(s)] += 1;
        }
        let (mut cells, mut sea_same, mut all_same) = (0, 0, 0);
        for (w, &s) in wants.iter().zip(&main.shader) {
            let shader = &vt.shaders[usize::from(s)];
            if shader.tiled != 0 {
                continue;
            }
            cells += 1;
            let dice = visual::rule_kind(&shader.name);
            let ours = visual::rule_kind(&vt.shaders[usize::from(visual::pick(&vt.shaders, &usage, *w).unwrap())].name);
            sea_same += usize::from(dice.sea == ours.sea);
            all_same += usize::from(dice.sea == ours.sea && dice.slope == ours.slope);
        }
        let has_sea = vt
            .shaders
            .iter()
            .any(|s| s.tiled == 0 && visual::rule_kind(&s.name).sea);
        println!(
            "{tn}: {cells} rule-only cells; the sea floor as DICE chose it on {sea_same} ({:.3}), sea floor and slope on {all_same} ({:.3}){}",
            sea_same as f64 / cells.max(1) as f64,
            all_same as f64 / cells.max(1) as f64,
            if has_sea { "" } else { " (no sea-floor variant)" }
        );
        if has_sea {
            sea_total.0 += sea_same;
            sea_total.1 += cells;
            assert!(sea_same as f64 / cells as f64 > 0.6, "{tn}");
        }
    }
    println!(
        "the sea floor as DICE chose it on {} of {} rule-only cells",
        sea_total.0, sea_total.1
    );
    assert!(sea_total.1 > 10_000 && sea_total.0 as f64 / sea_total.1 as f64 > 0.8);
    // DICE's rings: most ring cells of a terrain name one rule-only shader, the plain one [`visual::pick`] names
    let mut same = (0, 0);
    for (lv, tn) in [
        ("mp_001", "MP_001"),
        ("mp_006", "MP_006"),
        ("mp_007", "MP_007"),
        ("mp_008", "MP_008"),
        ("mp_009cq", "MP_009"),
        ("mp_012gr", "MP_012"),
    ] {
        let low = format!("terrains/{}/", tn.to_ascii_lowercase());
        let terr = read(&g, lv, "terrain-00.fbrb", |n, t| {
            n.to_ascii_lowercase().starts_with(&low) && t == "VisualTerrain"
        });
        let Some(d) = terr.values().next() else { continue };
        let vt = VisualTerrain::decode(d).unwrap();
        let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
        for s in vt.maps.iter().skip(1).flat_map(|m| m.shader.iter().copied()) {
            *counts.entry(s).or_insert(0) += 1;
        }
        let ring: BTreeSet<u8> = counts.keys().copied().collect();
        let most = *counts
            .iter()
            .max_by_key(|(s, c)| (**c, std::cmp::Reverse(**s)))
            .unwrap()
            .0;
        let mut usage = vec![0usize; vt.shaders.len()];
        for &s in &vt.maps[0].shader {
            usage[usize::from(s)] += 1;
        }
        let plain = visual::pick(&vt.shaders, &usage, RuleKind::default()).unwrap();
        println!(
            "{tn}: ring cells name {:?}; the plain pick {}",
            ring.iter()
                .map(|s| vt.shaders[usize::from(*s)].name.rsplit('/').next().unwrap())
                .collect::<Vec<_>>(),
            vt.shaders[usize::from(plain)].name.rsplit('/').next().unwrap()
        );
        assert_eq!(vt.shaders[usize::from(most)].tiled, 0, "{tn}");
        same.1 += 1;
        same.0 += usize::from(most == plain);
    }
    println!("the plain pick is DICE's ring shader on {} of {}", same.0, same.1);
    assert!(same.1 >= 4 && same.0 * 4 >= same.1 * 3, "{same:?}");
}
