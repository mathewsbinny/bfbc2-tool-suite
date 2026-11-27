//! Round G4's proofs: the start flow of the user's design, driven headless through its buttons.
//!
//! - `edit_existing_level` (always): a made-up game copy in temp (four Package levels, each with a description naming
//!   its game mode: `island_x` WW2 Conquest, `nam_x` Vietnam Rush, `mp_x` BFBC2 Conquest, `sp_x` BFBC2 campaign):
//!   Edit existing level lists no campaign level; Edit existing level, WW2, Open opens the WW2 map.
//! - `create_and_open_projects` (always): Create level, From existing levels lists the campaign level too and starts
//!   a project on it (its kind recorded); Open project from the recent list, with the crash-recovery offer.
//! - `create_from_a_height_map` (always): the form's checks (a good file, the wrong size, unreadable) and the saved
//!   project's settings, then the project opened again.
//! - `top_bar_fits` (always): a project with a long name at 1500 px: its name and badge end before the buttons.
//! - `real_counts` (`BC2_EDITOR_G4_REAL=1`): multiplayer / campaign per set on OPFOR's game copy, read only.
//! - `screenshots` (`BC2_EDITOR_G4_SHOTS=<folder>`): the start screen, each path's pages and the form, dark and light,
//!   1600 and 1280 px wide, on the real game copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::doc_markdown,
    clippy::panic,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::format_push_string,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bc2_editor::app::{EditorApp, StartPage};
use bc2_editor::catalog::LevelKind;
use bc2_editor::headless::Headless;
use bc2_editor::heightmap;
use bc2_editor::levels::{self, ContentSet};
use bc2_editor::theme::ThemeId;
use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_formats::fbrb;
use bc2_levelwrite::{BaseKind, MapProject, Profile};

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_editor_g4_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put(path: &Path, data: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

fn floats(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_be_bytes()).collect()
}

/// A level partition: one placed hut.
fn level_partition() -> Vec<u8> {
    let mut f = DbxFile::default();
    let mut m = [0.0f32; 16];
    for k in [0, 5, 10, 15] {
        m[k] = 1.0;
    }
    m[12] = 10.0;
    let r = Element::text(
        &mut f,
        "field",
        &[("name", "ReferencedObject"), ("ref", "Objects/Hut/Hut/AAAA")],
        "",
    );
    let t = Element::numbers(&mut f, "field", &[("name", "Transform")], 4, &floats(&m));
    let rod = Element::container(
        &mut f,
        "instance",
        &[
            ("guid", "11111111-0000-0000-0000-000000000001"),
            ("type", "GameSharedResources.ReferenceObjectData"),
        ],
        vec![r, t],
    );
    let p = Element::container(&mut f, "partition", &[("guid", "P")], vec![rod]);
    f.roots.push(p);
    dbx::write(&f).unwrap()
}

fn blueprint() -> Vec<u8> {
    let mut f = DbxFile::default();
    let o = Element::text(
        &mut f,
        "field",
        &[("name", "Object"), ("ref", "Objects/Hut/Hut_Data/CCCC")],
        "",
    );
    let i = Element::container(
        &mut f,
        "instance",
        &[("guid", "AAAA"), ("type", "Entity.ObjectBlueprint")],
        vec![o],
    );
    let p = Element::container(&mut f, "partition", &[("guid", "Q")], vec![i]);
    f.roots.push(p);
    dbx::write(&f).unwrap()
}

fn object_data() -> Vec<u8> {
    let mut f = DbxFile::default();
    let i = Element::container(
        &mut f,
        "instance",
        &[("guid", "CCCC"), ("type", "Entity.StaticModelEntityData")],
        vec![],
    );
    let p = Element::container(&mut f, "partition", &[("guid", "R")], vec![i]);
    f.roots.push(p);
    dbx::write(&f).unwrap()
}

/// A level description naming its game mode (`gamemodes/<mode>/...`), as `LevelDescriptionAsset` does.
fn description(mode: &str) -> Vec<u8> {
    let mut f = DbxFile::default();
    let g = Element::text(
        &mut f,
        "field",
        &[("name", "GameMode"), ("ref", &format!("gamemodes/{mode}/1234"))],
        "",
    );
    let d = Element::container(&mut f, "complex", &[("name", "Description")], vec![g]);
    let i = Element::container(
        &mut f,
        "instance",
        &[("guid", "D"), ("type", "GameSharedResources.LevelDescriptionAsset")],
        vec![d],
    );
    let p = Element::container(&mut f, "partition", &[("guid", "D")], vec![i]);
    f.roots.push(p);
    dbx::write(&f).unwrap()
}

/// The levels of the made-up copy: (level, its common bundle, its game mode).
const LEVELS: [(&str, &str, &str); 4] = [
    ("island_x", "ww2_common", "conquest"),
    ("nam_x", "nam_common", "namgoldrush"),
    ("mp_x", "bf_common", "conquest"),
    ("sp_x", "bf_common", "singelplayerfirstlevel"),
];

/// A made-up game copy: four Package levels, each on its common bundle and `mp_common`, with a partition, a hut,
/// a terrain stack with water, and a description.
fn stand_in(root: &Path) {
    let game = root.join("game");
    let dist = game.join("Dist").join("win32");
    let pk = game.join("Package").join("levels");
    let mut mft = String::new();
    let mut descs = Vec::new();
    for (lv, common, mode) in LEVELS {
        mft.push_str(&format!(
            "BundleSource game:levels/{lv}/level-00.fbrb\r\nBundleTarget package:levels/{lv}/level-00.fbrb\r\n\
             BundleTarget package:levels/{common}/level-00.fbrb\r\nBundleTarget game:levels/mp_common/level-00.fbrb\r\n\r\n\
             BundleSource game:levels/{lv}/terrain-00.fbrb\r\nBundleTarget package:levels/{lv}/terrain-00.fbrb\r\n\
             BundleTarget package:levels/{common}/terrain-00.fbrb\r\n\r\n"
        ));
        let name = format!("Levels/{}.dbx", lv.to_uppercase());
        put(
            &pk.join(lv).join("level-00.fbrb"),
            &fbrb::build(&[(name.as_str(), "<non-resource>", &level_partition())], true),
        );
        put(
            &pk.join(lv).join("terrain-00.fbrb"),
            &fbrb::build(&[("Terrains/U/U.Heightfield-000.res", fbrb::DELETED, b"")], true),
        );
        descs.push((
            format!("Levels/{}_description.dbx", lv.to_uppercase()),
            description(mode),
        ));
    }
    for common in ["ww2_common", "nam_common", "bf_common"] {
        put(
            &pk.join(common).join("level-00.fbrb"),
            &fbrb::build(
                &[
                    ("Objects/Hut/Hut.dbx", "<non-resource>", &blueprint()),
                    ("Objects/Hut/Hut_Havok.res", "HavokPhysicsData", b"packfile"),
                ],
                true,
            ),
        );
        put(
            &pk.join(common).join("terrain-00.fbrb"),
            &fbrb::build(&[("Terrains/T/T.Water.Mesh.res", "WaterMesh", b"water")], true),
        );
    }
    put(
        &dist.join("levels").join("mp_common").join("level-00.fbrb"),
        &fbrb::build(&[("Objects/Hut/Hut_Data.dbx", "<non-resource>", &object_data())], true),
    );
    put(&dist.join("package.mft"), mft.as_bytes());
    let entries: Vec<(&str, &str, &[u8])> = descs
        .iter()
        .map(|(n, d)| (n.as_str(), "<non-resource>", d.as_slice()))
        .collect();
    put(
        &game.join("Package").join("async").join("startup-00.fbrb"),
        &fbrb::build(&entries, true),
    );
}

fn headless(size: [u32; 2]) -> Option<Headless> {
    match Headless::new(size) {
        Ok(h) => Some(h),
        Err(e) => {
            println!("skipped: {e}");
            None
        }
    }
}

/// Clicks a start screen button by its label (drawn in the last frame).
fn click(h: &mut Headless, app: &mut EditorApp, label: &str) {
    let r = app
        .start_button(label)
        .unwrap_or_else(|| panic!("{label} not drawn; drawn: {:?}", app.start_button_labels()));
    h.click(app, r.center());
    h.settle(app, 2);
}

/// Frames until a level is open (prepared and loaded), at most a minute.
fn wait_open(h: &mut Headless, app: &mut EditorApp) {
    let t = Instant::now();
    while app.view().is_none() && t.elapsed() < Duration::from_secs(60) {
        if let Some(e) = app.error() {
            panic!("did not open: {e}");
        }
        h.frame(app, Vec::new());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(app.view().is_some(), "opened within a minute");
    h.settle(app, 3);
}

fn app_on(root: &Path, saved: Option<bc2_editor::app::Saved>) -> EditorApp {
    let mut app = EditorApp::new(Some(root), saved).unwrap();
    app.wait_for_catalog(Duration::from_secs(30));
    app
}

#[test]
fn edit_existing_level() {
    let _gate = crate::gate();
    let root = temp("edit");
    stand_in(&root);
    let mut app = app_on(&root, None);
    let names: Vec<&str> = app.levels().iter().map(|l| l.lv.as_str()).collect();
    assert_eq!(names, ["island_x", "mp_x", "nam_x", "sp_x"]);
    assert_eq!(app.level_kind("sp_x"), LevelKind::Campaign);
    assert_eq!(app.level_kind("nam_x"), LevelKind::Multiplayer);
    let mp: Vec<String> = app.edit_levels(ContentSet::Bfbc2).into_iter().map(|l| l.lv).collect();
    assert_eq!(mp, ["mp_x"], "no campaign level in Edit existing level");
    let Some(mut h) = headless([1280, 800]) else {
        let _ = std::fs::remove_dir_all(&root);
        return;
    };
    h.settle(&mut app, 3);
    assert_eq!(app.start_page(), StartPage::Home);
    for path in ["Open project", "Edit existing level", "Create level"] {
        assert!(app.start_button(path).is_some(), "{path}");
    }
    click(&mut h, &mut app, "Edit existing level");
    assert_eq!(app.start_page(), StartPage::EditModes);
    // BFBC2's list: the multiplayer map, not the campaign level
    click(&mut h, &mut app, "Mode:BFBC2");
    assert_eq!(app.chosen_mode(), Some(ContentSet::Bfbc2));
    let labels = app.start_button_labels();
    assert!(labels.contains(&"Open:mp_x".to_owned()), "{labels:?}");
    assert!(!labels.iter().any(|l| l.ends_with(":sp_x")), "{labels:?}");
    click(&mut h, &mut app, "Back");
    assert_eq!(app.start_page(), StartPage::EditModes);
    // WW2: its map opens
    click(&mut h, &mut app, "Mode:WW2 / Pacific War");
    assert_eq!(app.chosen_mode(), Some(ContentSet::Ww2));
    click(&mut h, &mut app, "Open:island_x");
    wait_open(&mut h, &mut app);
    let v = app.view().unwrap();
    assert_eq!(v.lv, "island_x");
    assert!(v.doc.is_none(), "opened read-only");
    println!(
        "g4 edit existing: WW2 -> island_x opened, {} entities",
        v.scene().entities.len()
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn create_and_open_projects() {
    let _gate = crate::gate();
    let root = temp("create");
    stand_in(&root);
    let folder = root.join("projects");
    let mut app = app_on(&root, None);
    let Some(mut h) = headless([1280, 800]) else {
        let _ = std::fs::remove_dir_all(&root);
        return;
    };
    h.settle(&mut app, 3);
    click(&mut h, &mut app, "Create level");
    assert_eq!(app.start_page(), StartPage::CreateChoice);
    click(&mut h, &mut app, "From existing levels");
    assert_eq!(app.start_page(), StartPage::CreateFromExisting);
    let labels = app.start_button_labels();
    for (lv, _, _) in LEVELS {
        assert!(labels.contains(&format!("New project:{lv}")), "{lv} in {labels:?}");
    }
    click(&mut h, &mut app, "New project:sp_x");
    assert_eq!(app.new_project_level(), Some("sp_x"));
    app.set_new_project("Campaign Base", &folder);
    h.settle(&mut app, 2);
    click(&mut h, &mut app, "Create level");
    wait_open(&mut h, &mut app);
    let d = app.view().unwrap().doc.as_ref().expect("a project");
    assert_eq!(d.project.base.level, "sp_x");
    assert_eq!(d.project.base.kind, Some(BaseKind::Campaign));
    let path = d.path.clone();
    assert_eq!(path, folder.join("Campaign Base.mapedit.json"));
    assert_eq!(MapProject::load(&path).unwrap().base.kind, Some(BaseKind::Campaign));
    println!("g4 create from existing: sp_x -> {} (kind campaign)", path.display());
    // the next start: Open project, the recent project (an autosave newer than the file: the recovery offer first)
    let saved = app.saved();
    let mut auto = MapProject::load(&path).unwrap();
    auto.next += 1;
    auto.autosave(&path).unwrap();
    let mut app = app_on(&root, Some(saved));
    h.settle(&mut app, 3);
    click(&mut h, &mut app, "Open project");
    assert_eq!(app.start_page(), StartPage::OpenProject);
    let labels = app.start_button_labels();
    assert!(
        labels.contains(&"found:Campaign Base.mapedit.json".to_owned()),
        "{labels:?}"
    );
    click(&mut h, &mut app, "recent:Campaign Base.mapedit.json");
    assert_eq!(app.recovery_offer(), Some(path.as_path()));
    app.answer_recovery(false);
    wait_open(&mut h, &mut app);
    let d = app.view().unwrap().doc.as_ref().unwrap();
    assert_eq!(d.path, path);
    assert!(!bc2_levelwrite::project::autosave_path(&path).exists());
    println!("g4 open project: from recent, the recovery offer answered, opened");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn create_from_a_height_map() {
    let _gate = crate::gate();
    let root = temp("heightmap");
    stand_in(&root);
    let good = root.join("atoll.png");
    heightmap::write_png16(&good, 1025, &|x, y| ((x + y) * 20) as u16).unwrap();
    let small = root.join("small.png");
    heightmap::write_png16(&small, 300, &|_, _| 9).unwrap();
    let folder = root.join("projects");
    let mut app = app_on(&root, None);
    let Some(mut h) = headless([1280, 1500]) else {
        let _ = std::fs::remove_dir_all(&root);
        return;
    };
    h.settle(&mut app, 3);
    click(&mut h, &mut app, "Create level");
    click(&mut h, &mut app, "From a height map");
    assert_eq!(app.start_page(), StartPage::CreateFromHeightmap);
    let folder_s = folder.display().to_string();
    // unreadable
    app.heightmap.path = root.join("none.png").display().to_string();
    app.heightmap.name = "Atoll".into();
    app.set_new_project("", &folder);
    h.settle(&mut app, 2);
    assert!(app.heightmap.why_not(&folder_s).unwrap().contains("does not read"));
    // the wrong size
    app.heightmap.path = small.display().to_string();
    h.settle(&mut app, 2);
    assert!(app.heightmap.why_not(&folder_s).unwrap().contains("300 x 300"));
    // a RAW of the wrong length for the size chosen
    let raw = root.join("h.raw");
    std::fs::write(&raw, vec![0u8; 2 * 1024 * 1024]).unwrap();
    app.heightmap.path = raw.display().to_string();
    app.heightmap.raw = true;
    app.heightmap.raw_side = 2048;
    h.settle(&mut app, 2);
    assert!(app.heightmap.why_not(&folder_s).unwrap().contains("choose 1024"));
    // the good file, the numbers, WW2
    app.heightmap.raw = false;
    app.heightmap.path = good.display().to_string();
    app.heightmap.settings.map_size_m = 1024.0;
    app.heightmap.settings.height_min_m = -10.0;
    app.heightmap.settings.height_max_m = 250.0;
    app.heightmap.settings.water_level_m = 2.5;
    h.settle(&mut app, 2);
    click(&mut h, &mut app, "Set:WW2 / Pacific War");
    assert_eq!(app.heightmap.settings.content_set, ContentSet::Ww2);
    assert_eq!(app.heightmap.why_not(&folder_s), None);
    // round E7: the project is made on a base level read from the game copy; the stand-in has none, so Create
    // refuses in words and writes nothing
    click(&mut h, &mut app, "Create project");
    assert!(
        app.error().is_some_and(|e| e.contains("mp_005") || e.contains("game")),
        "{:?}",
        app.error()
    );
    let path = folder.join("Atoll.mapedit.json");
    assert!(!path.exists());
    // what Create saves, with a base partition given (the game tier's terrain_tools test makes one from a real base)
    let path = heightmap::save(&heightmap::Request {
        name: "Atoll",
        folder: &folder,
        heightmap: &good,
        source: heightmap::Source::Png,
        colour_map: None,
        settings: app.heightmap.settings.clone(),
        base_bytes: b"stand-in partition",
    })
    .unwrap();
    let p = MapProject::load(&path).unwrap();
    assert!(!p.is_import() && p.new_terrain().is_some());
    assert_eq!(p.base.level, "mp_005");
    let t = p.terrain_import.as_ref().unwrap();
    assert_eq!(t.content_set, "WW2");
    assert_eq!((t.samples, t.sample_range), (1025, [0, 40_960]));
    assert_eq!(t.map_size_m, 1024.0);
    assert_eq!(t.height_range_m, [-10.0, 250.0]);
    assert_eq!(t.water_level_m, 2.5);
    assert_eq!(
        std::fs::read(folder.join(&t.heightmap.file)).unwrap(),
        std::fs::read(&good).unwrap()
    );
    assert_eq!(t.biome.as_deref(), Some("arid"));
    // "Generate": the generator's settings checked in the form, saved the same way
    click(&mut h, &mut app, "Source:Generate");
    assert!(app.heightmap.generate);
    app.heightmap.pads = "10, 20; 0, 0, 40".into();
    app.heightmap.name = "Dunes".into();
    assert_eq!(app.heightmap.why_not(&folder_s), None);
    assert_eq!(
        app.heightmap.gen_settings.pads,
        vec![[10.0, 20.0, 24.0], [0.0, 0.0, 40.0]]
    );
    app.heightmap.pads = "5000, 0".into();
    assert!(app.heightmap.why_not(&folder_s).unwrap().contains("outside"));
    app.heightmap.pads = "1".into();
    assert!(app.heightmap.why_not(&folder_s).unwrap().contains("Pad"));
    app.heightmap.pads.clear();
    click(&mut h, &mut app, "Biome:snow");
    assert_eq!(app.heightmap.gen_settings.biome, bc2_terragen::Biome::Snow);
    let gpath = heightmap::save_generated(&heightmap::GenerateRequest {
        name: "Dunes",
        folder: &folder,
        settings: app.heightmap.settings.clone(),
        generate: app.heightmap.gen_settings.clone(),
        base_bytes: b"stand-in partition",
    })
    .unwrap();
    let g = MapProject::load(&gpath).unwrap();
    let gs = g.new_terrain().unwrap().generate.as_ref().unwrap();
    assert_eq!((gs.biome.as_str(), gs.size, gs.seed, gs.ring), ("snow", 1024, 7, true));
    // a G4 project made without a base: its page, not a level
    let old = MapProject::new_import("Old", Profile::Ww2, t.clone());
    let path = folder.join("Old.mapedit.json");
    old.save(&path).unwrap();
    click(&mut h, &mut app, "Source:From a height map");
    app.go(StartPage::Home);
    app.open_project(&path);
    h.settle(&mut app, 2);
    assert_eq!(app.start_page(), StartPage::ImportProject);
    assert!(app.view().is_none());
    println!(
        "g4 height map: checks (unreadable, 300 x 300, RAW length) refused; {} saved ({} x {}, {} m)",
        path.display(),
        t.samples,
        t.samples,
        t.map_size_m
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn top_bar_fits() {
    let _gate = crate::gate();
    let Some(mut h) = headless([1500, 900]) else { return };
    let dir = temp("topbar");
    let scene = bc2_editor::demo::level(&dir.join("level")).unwrap();
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    app.open_scene_then(
        &gpu,
        "wake_island_s",
        scene,
        bc2_editor::app::Then::Create("Operation Long Project Name North Ridge".into(), dir.clone(), None),
    );
    h.settle(&mut app, 6);
    let [info, buttons] = app.top_bar_spans();
    println!("g4 top bar at 1500 px: name and badge end at {info:.0}, buttons start at {buttons:.0}");
    assert!(info > 0.0 && info <= buttons, "{info} {buttons}");
    if let Some(d) = std::env::var_os("BC2_EDITOR_G4_SHOTS") {
        let d = PathBuf::from(d);
        std::fs::create_dir_all(&d).unwrap();
        h.save(&d.join("g4_top_bar_1500_dark.png")).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn real_counts() {
    let _gate = crate::gate();
    if std::env::var_os("BC2_EDITOR_G4_REAL").is_none() {
        println!("skipped: set BC2_EDITOR_G4_REAL=1 to count the real game copy's levels");
        return;
    }
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no Project OPFOR BFBC2");
        return;
    };
    let app = app_on(&root, None);
    let mut total = [0usize; 3];
    for set in ContentSet::ALL {
        let all = app.create_levels(set);
        let count = |k: LevelKind| all.iter().filter(|l| app.level_kind(&l.lv) == k).count();
        let (mp, sp, un) = (
            count(LevelKind::Multiplayer),
            count(LevelKind::Campaign),
            count(LevelKind::Unknown),
        );
        let sps: Vec<String> = all
            .iter()
            .filter(|l| app.level_kind(&l.lv) == LevelKind::Campaign)
            .map(|l| l.lv.clone())
            .collect();
        println!(
            "g4 real {}: {} levels, {mp} multiplayer, {sp} campaign, {un} unknown; campaign: {sps:?}",
            set.word(),
            all.len()
        );
        assert_eq!(app.edit_levels(set).len(), mp);
        total[0] += mp;
        total[1] += sp;
        total[2] += un;
    }
    println!(
        "g4 real all: {} multiplayer, {} campaign, {} unknown",
        total[0], total[1], total[2]
    );
}

#[test]
fn screenshots() {
    let _gate = crate::gate();
    let Some(out) = std::env::var_os("BC2_EDITOR_G4_SHOTS").map(PathBuf::from) else {
        println!("skipped: set BC2_EDITOR_G4_SHOTS=<folder> for the screenshots");
        return;
    };
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no Project OPFOR BFBC2");
        return;
    };
    std::fs::create_dir_all(&out).unwrap();
    let work = temp("shots");
    let good = work.join("terrain.png");
    heightmap::write_png16(&good, 2049, &|x, y| ((x * 13 + y * 7) % 60_000) as u16).unwrap();
    let projects = work.join("projects");
    for (w, hgt) in [(1600u32, 950u32), (1280, 800)] {
        for theme in [ThemeId::Dark, ThemeId::Light] {
            let Some(mut h) = headless([w, hgt]) else { return };
            let mut app = app_on(&root, None);
            app.theme = theme;
            let t = if theme == ThemeId::Dark { "dark" } else { "light" };
            let shot = |h: &mut Headless, app: &mut EditorApp, name: &str| {
                h.settle(app, 4);
                h.save(&out.join(format!("g4_{name}_{w}_{t}.png"))).unwrap();
            };
            shot(&mut h, &mut app, "1_start");
            app.go(StartPage::OpenProject);
            shot(&mut h, &mut app, "2_open_project");
            app.go(StartPage::EditModes);
            shot(&mut h, &mut app, "3_edit_modes");
            app.choose_mode(Some(ContentSet::Ww2));
            shot(&mut h, &mut app, "4_edit_ww2_maps");
            app.choose_mode(Some(ContentSet::Bfbc2));
            shot(&mut h, &mut app, "5_edit_bfbc2_maps");
            app.start_new_project("mp_005");
            shot(&mut h, &mut app, "6_edit_new_project");
            app.go(StartPage::CreateChoice);
            shot(&mut h, &mut app, "7_create_choice");
            app.go(StartPage::CreateFromExisting);
            shot(&mut h, &mut app, "8_create_from_existing");
            app.start_new_project("sp_01");
            shot(&mut h, &mut app, "9_create_campaign_base");
            app.go(StartPage::CreateFromHeightmap);
            app.heightmap.name = "Atoll".into();
            app.heightmap.path = good.display().to_string();
            app.heightmap.settings.water_level_m = 4.0;
            app.set_new_project("", &projects.join(format!("{w}_{t}")));
            shot(&mut h, &mut app, "10_heightmap_form");
            app.heightmap.raw = true;
            app.heightmap.raw_side = 1024;
            shot(&mut h, &mut app, "11_heightmap_form_error");
            app.heightmap.raw = false;
            h.settle(&mut app, 2);
            app.create_from_heightmap();
            assert_eq!(app.error(), None);
            shot(&mut h, &mut app, "12_heightmap_project");
        }
    }
    let _ = std::fs::remove_dir_all(&work);
}
