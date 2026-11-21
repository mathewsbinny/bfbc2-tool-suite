//! Round G3's proofs: the levels that live only in `game\Package` are listed, prepared from their bundles and opened.
//!
//! - `stand_in_package_levels` (always): a made-up game copy in temp with a Package level on `nam_common` and
//!   `mp_common` - the registry's stack, a `*deleted*` marker hiding a later bundle's partition, the copy's record and
//!   its checks, Cancel, the editor's start screen listing it with its badge, and opening it.
//! - `real_package_levels` (with `BC2_EDITOR_G3=all` or a comma list of levels): every installed Package level of the
//!   real game copy, prepared into a stand-in Project OPFOR BFBC2 folder in temp (the game copy linked read-only by a
//!   junction, removed before the folder) and opened as the editor opens it; `BC2_EDITOR_G3_OUT=<file>` writes what
//!   each opened with (entities, collision, terrain, water, teams, times) as JSON for the independent check
//!   (`package_levels_check.py`), `BC2_EDITOR_SHOTS=<folder>` saves dark and light screenshots of `BC2_EDITOR_G3_SHOTS`
//!   (a comma list).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::doc_markdown,
    clippy::many_single_char_names,
    clippy::panic,
    clippy::too_many_lines,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use bc2_editor::app::{EditorApp, Then};
use bc2_editor::factions::{Profile, Teams};
use bc2_editor::headless::Headless;
use bc2_editor::theme::ThemeId;
use bc2_editor::{levels, package, prepare};
use bc2_formats::dbx::edit::EditDbx;
use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_formats::fbrb;
use bc2_levelwrite::Op;
use bc2_levelwrite::diff::{Change, diff};
use bc2_levelwrite::ops::Moved;
use bc2_levelwrite::write::LevelWriter;
use bc2_scene::scene;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_editor_g3_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn floats(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_be_bytes()).collect()
}

/// A level partition: one placed object of `objects/hut/hut`, a terrain entity naming `terrains/t/t`.
fn level_partition() -> Vec<u8> {
    let mut f = DbxFile::default();
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
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
    let ta = Element::text(
        &mut f,
        "field",
        &[("name", "TerrainAsset"), ("ref", "Terrains/T/T/BBBB")],
        "",
    );
    let ter = Element::container(
        &mut f,
        "instance",
        &[
            ("guid", "11111111-0000-0000-0000-000000000002"),
            ("type", "Terrain.TerrainEntityData"),
        ],
        vec![ta],
    );
    let p = Element::container(&mut f, "partition", &[("guid", "P")], vec![rod, ter]);
    f.roots.push(p);
    dbx::write(&f).unwrap()
}

/// A blueprint partition: `AAAA` an object data naming `Object` in `objects/hut/hut_data`.
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

fn put(path: &Path, data: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

/// The made-up game copy: `nam_x` on `nam_common` and `mp_common`; the level's partition and the hut's blueprint in
/// mp_common under a name nam_common deletes (hidden) and again in nam_common (the one used).
fn stand_in(root: &Path) {
    let game = root.join("game");
    let mft = "BundleSource game:levels/nam_x/level-00.fbrb\r\nBundleTarget package:levels/nam_x/level-00.fbrb\r\n\
               BundleTarget package:levels/nam_common/level-00.fbrb\r\nBundleTarget game:levels/mp_common/level-00.fbrb\r\n\r\n\
               BundleSource game:levels/nam_x/terrain-00.fbrb\r\nBundleTarget package:levels/nam_x/terrain-00.fbrb\r\n\
               BundleTarget package:levels/nam_common/terrain-00.fbrb\r\n\r\n\
               BundleSource game:levels/nam_gone/level-00.fbrb\r\nBundleTarget package:levels/nam_gone/level-00.fbrb\r\n\
               BundleTarget package:levels/missing_common/level-00.fbrb\r\n";
    put(&game.join("Dist").join("win32").join("package.mft"), mft.as_bytes());
    put(
        &game
            .join("Dist")
            .join("win32")
            .join("levels")
            .join("mp_005")
            .join("level-00.fbrb"),
        b"x",
    );
    let lp = level_partition();
    let bp = blueprint();
    let od = object_data();
    let pk = game.join("Package").join("levels");
    put(
        &pk.join("nam_x").join("level-00.fbrb"),
        &fbrb::build(
            &[
                ("Levels/NAM_X.dbx", "<non-resource>", &lp),
                ("Objects/Other/Gone.dbx", fbrb::DELETED, b""),
            ],
            true,
        ),
    );
    put(
        &pk.join("nam_common").join("level-00.fbrb"),
        &fbrb::build(
            &[
                ("Objects/Hut/Hut.dbx", "<non-resource>", &bp),
                ("Objects/Hut/Hut_Havok.res", "HavokPhysicsData", b"packfile"),
                ("Objects/Hut/Hut_Data.dbx", fbrb::DELETED, b""),
            ],
            true,
        ),
    );
    put(
        &game
            .join("Dist")
            .join("win32")
            .join("levels")
            .join("mp_common")
            .join("level-00.fbrb"),
        &fbrb::build(
            &[
                (
                    "Objects/Hut/Hut.dbx",
                    "<non-resource>",
                    b"older, hidden by nam_common's",
                ),
                ("Objects/Hut/Hut_Data.dbx", "<non-resource>", &od),
                ("Objects/Other/Gone.dbx", "<non-resource>", &od),
            ],
            true,
        ),
    );
    put(
        &pk.join("nam_x").join("terrain-00.fbrb"),
        &fbrb::build(&[("Terrains/U/U.Heightfield-000.res", fbrb::DELETED, b"")], true),
    );
    put(
        &pk.join("nam_common").join("terrain-00.fbrb"),
        &fbrb::build(&[("Terrains/T/T.Water.Mesh.res", "WaterMesh", b"water")], true),
    );
    // a level whose common bundle is not installed: not listed
    put(&pk.join("nam_gone").join("level-00.fbrb"), &fbrb::build(&[], true));
    // the common folders are never levels
    put(&pk.join("nam_common").join("level-00.fbrb.keep"), b"");
}

/// [`stand_in`] plus one level of each other set, each set decided by its data: a Bad Company 1 map (`bc1_`, on
/// `mp2_common`), a Pacific War level named like none (`island_x`: its stack loads `ww2_common`), an extra mode layout
/// on a Dist level (`mp_005gr`: BFBC2).
fn stand_in_all_sets(root: &Path) {
    stand_in(root);
    let game = root.join("game");
    let pk = game.join("Package").join("levels");
    let mft_path = game.join("Dist").join("win32").join("package.mft");
    let mut mft = std::fs::read_to_string(&mft_path).unwrap();
    for (lv, common) in [
        ("bc1_x", "package:levels/mp2_common/level-00.fbrb"),
        ("island_x", "package:levels/ww2_common/level-00.fbrb"),
        ("mp_005gr", "game:levels/mp_005/level-00.fbrb"),
    ] {
        std::fmt::Write::write_fmt(
            &mut mft,
            format_args!(
                "\r\nBundleSource game:levels/{lv}/level-00.fbrb\r\nBundleTarget package:levels/{lv}/level-00.fbrb\r\n\
                 BundleTarget {common}\r\nBundleTarget game:levels/mp_common/level-00.fbrb\r\n"
            ),
        )
        .unwrap();
        put(&pk.join(lv).join("level-00.fbrb"), &fbrb::build(&[], true));
    }
    for common in ["mp2_common", "ww2_common"] {
        put(&pk.join(common).join("level-00.fbrb"), &fbrb::build(&[], true));
    }
    std::fs::write(&mft_path, mft).unwrap();
}
/// Each level's set of maps from its data: the `bc1_` maps of game\Package, the stacks loading `nam_common` and
/// `ww2_common` (whatever the level is named), everything else BFBC2; the BFBC1 badge; the start screen draws them.
#[test]
fn sets_from_the_data() {
    use bc2_editor::levels::ContentSet;
    let _gate = crate::gate();
    let root = temp("sets");
    stand_in_all_sets(&root);
    let app = EditorApp::new(Some(&root), None).unwrap();
    let set_of = |lv: &str| app.levels().iter().find(|l| l.lv == lv).unwrap().set;
    assert_eq!(set_of("mp_005"), ContentSet::Bfbc2);
    assert_eq!(set_of("mp_005gr"), ContentSet::Bfbc2);
    assert_eq!(set_of("bc1_x"), ContentSet::Bfbc1);
    assert_eq!(set_of("nam_x"), ContentSet::Vietnam);
    assert_eq!(set_of("island_x"), ContentSet::Ww2);
    assert!(ContentSet::ALL.iter().all(|s| app.levels().iter().any(|l| l.set == *s)));
    assert_eq!(ContentSet::Bfbc1.badge(), "BFBC1");
    assert_eq!(bc2_editor::app::profile_of("bc1_x"), "BFBC1");
    assert_eq!(bc2_editor::app::profile_of("mp_005"), "BFBC2");
    assert_eq!(Profile::of("island_x"), Profile::Ww2, "recognised from its bundles");
    // a Dist level is never BFBC1, whatever its name
    assert_eq!(ContentSet::of("bc1_y", Profile::Bfbc2, false), ContentSet::Bfbc2);
    if let Ok(mut h) = Headless::new([1280, 800]) {
        let mut app = app;
        h.settle(&mut app, 4);
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The "Choose mode" cards without the game's art (this made-up copy has no loading screens): the catalog finds no
/// picture, every card is drawn as the plain tile, a set's map list opens and goes back; a card picture that is not a
/// texture the reader decodes falls back the same way.
#[test]
fn mode_cards_fall_back_to_the_tile() {
    use bc2_editor::catalog;
    use bc2_editor::levels::ContentSet;
    let _gate = crate::gate();
    let root = temp("tiles");
    stand_in_all_sets(&root);
    // a loading screen that is not a texture: no picture
    put(
        &root
            .join("game")
            .join("Package")
            .join("async")
            .join("ondemand_awards-00.fbrb"),
        &fbrb::build(
            &[(
                "awards/images/loadingscreens/nam_mp_002.res",
                "ITexture",
                b"RES\x02 not a texture",
            )],
            true,
        ),
    );
    let mut app = EditorApp::new(Some(&root), None).unwrap();
    app.wait_for_catalog(std::time::Duration::from_secs(30));
    let c = app.catalog().unwrap();
    assert!(c.cards.is_empty() && c.thumbs.is_empty(), "no art: the tiles");
    assert!(
        root.join("work")
            .join("editor")
            .join("art")
            .join(catalog::RECORD)
            .is_file()
    );
    let Ok(mut h) = Headless::new([1280, 800]) else {
        println!("skipped the drawing: no graphics adapter");
        let _ = std::fs::remove_dir_all(&root);
        return;
    };
    h.settle(&mut app, 4);
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        h.save(&Path::new(&dir).join("g3_mode_tiles_check.png")).unwrap();
    }
    assert_eq!(app.chosen_mode(), None);
    app.choose_mode(Some(ContentSet::Vietnam));
    h.settle(&mut app, 3);
    assert_eq!(app.chosen_mode(), Some(ContentSet::Vietnam));
    app.choose_mode(None);
    h.settle(&mut app, 2);
    // a second start reads the cache
    let mut again = EditorApp::new(Some(&root), None).unwrap();
    again.wait_for_catalog(std::time::Duration::from_secs(30));
    assert!(!again.catalog().unwrap().fresh, "from the cache");
    let _ = std::fs::remove_dir_all(&root);
}

/// The cards' art on the real game copy (`BC2_EDITOR_G3_ART=1`): each set's loading screen found and decoded, the
/// level thumbnails and game modes read, then the same from the cache; on a stand-in OPFOR folder in temp (the game
/// copy linked read only, the junction removed first).
#[test]
fn card_art_on_the_real_game_copy() {
    use bc2_editor::catalog;
    use bc2_editor::levels::ContentSet;
    let _gate = crate::gate();
    if std::env::var_os("BC2_EDITOR_G3_ART").is_none() {
        println!("skipped: set BC2_EDITOR_G3_ART=1 to read the cards' art from the real game copy");
        return;
    }
    let Ok(real) = levels::find_opfor(None) else {
        println!("skipped: no Project OPFOR BFBC2");
        return;
    };
    let root = temp("art");
    junction(&root.join("game"), &real.join("game"));
    let names: Vec<String> = levels::list_levels(&root).into_iter().map(|l| l.lv).collect();
    let t = std::time::Instant::now();
    let c = catalog::load(&root, &names);
    let cold = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let warm = catalog::load(&root, &names);
    let warm_s = t.elapsed().as_secs_f64();
    std::fs::remove_dir(root.join("game")).unwrap();
    assert!(real.join("game").is_dir());
    let _ = std::fs::remove_dir_all(&root);
    for set in ContentSet::ALL {
        let p = c.cards.get(&set).unwrap_or_else(|| panic!("{set:?}: no card art"));
        println!("{set:?}: {} x {}", p.size[0], p.size[1]);
        assert!(p.size[0] >= 256 && p.size[1] >= 128);
    }
    println!(
        "{} levels: {} thumbnails, {} modes; read {cold:.1} s, from the cache {warm_s:.2} s",
        names.len(),
        c.thumbs.len(),
        c.modes.len()
    );
    assert!(c.fresh && !warm.fresh);
    assert_eq!(warm.thumbs.len(), c.thumbs.len());
    assert_eq!(c.modes.get("wake_island_s").map(String::as_str), Some("Conquest"));
    assert_eq!(c.modes.get("nam_mp_002r").map(String::as_str), Some("Rush"));
}

#[test]
fn stand_in_package_levels() {
    let _gate = crate::gate();
    let root = temp("stand_in");
    stand_in(&root);
    let all = package::package_levels(&root);
    let names: Vec<&str> = all.iter().map(|l| l.lv.as_str()).collect();
    assert_eq!(names, ["nam_x"], "installed Package-only levels");
    let l = &all[0];
    assert_eq!(l.profile, Profile::Vietnam);
    assert_eq!(l.level.len(), 3);
    assert_eq!(l.terrain.len(), 2);
    assert_eq!(prepare::plan(&root, "nam_x"), prepare::Plan::Package);
    // the start screen lists it, not prepared, with its badge
    let listed = levels::list_levels(&root);
    let info = listed.iter().find(|i| i.lv == "nam_x").unwrap();
    assert!(info.package && !info.prepared && info.profile == Profile::Vietnam);
    assert!(listed.iter().any(|i| i.lv == "mp_005" && !i.package));
    // cancelled before it starts: nothing left
    let stop = AtomicBool::new(true);
    let e = package::prepare(&root, l, &mut |_| {}, &stop).unwrap_err();
    assert!(e.contains("cancelled"), "{e}");
    assert!(!package::cache_dir(&root, "nam_x").exists());
    // prepared: the steps, the files in the game's order, the masked name, the record
    let mut steps = Vec::new();
    let p = package::prepare(
        &root,
        l,
        &mut |e| {
            if let package::Event::Step { n, .. } = e {
                steps.push(n);
            }
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(steps, [1, 2, 3, 4]);
    assert_eq!(p.layers, ["00_nam_x", "01_nam_common", "02_mp_common"]);
    let dir = package::cache_dir(&root, "nam_x");
    assert_eq!(dir, root.join("work").join("editor").join("levels").join("nam_x"));
    assert!(!root.join("work").join("levels").exists(), "nothing in work\\levels");
    assert!(dir.join("00_nam_x").join("Levels").join("NAM_X.dbx").is_file());
    assert_eq!(
        std::fs::read(dir.join("01_nam_common").join("Objects").join("Hut").join("Hut.dbx")).unwrap(),
        blueprint()
    );
    assert!(
        dir.join("01_nam_common")
            .join("Objects")
            .join("Hut")
            .join("Hut.xml")
            .is_file()
    );
    assert!(
        dir.join("01_nam_common")
            .join("Objects")
            .join("Hut")
            .join("Hut_Havok.res")
            .is_file()
    );
    // nam_common's marker hides mp_common's Hut_Data: not read, counted
    assert!(
        !dir.join("02_mp_common")
            .join("Objects")
            .join("Hut")
            .join("Hut_Data.dbx")
            .exists()
    );
    assert_eq!(p.stats.masked, 1);
    assert_eq!(p.terrain_name.as_deref(), Some("t"));
    assert!(p.heightfield.is_none() && p.water.is_some());
    assert!(p.notes.iter().any(|n| n.contains("no t height field")));
    assert!(package::ready(&root, l).is_ok());
    assert_eq!(prepare::plan(&root, "nam_x"), prepare::Plan::Ready);
    // the options the editor opens it with; the scene's entities
    let found = levels::load_options(&root, "nam_x").unwrap();
    assert_eq!(found.options.name, "NAM_X");
    assert_eq!(found.options.bases.len(), 3);
    let s = scene::load(&found.options).unwrap();
    assert_eq!(s.entities.len(), 2);
    // its object data (the second level) is the one nam_common's marker hides: no type, as in the game
    assert_eq!(s.entities[0].blueprint.as_deref(), Some("objects/hut/hut"));
    assert_eq!(s.entities[0].blueprint_type, None);
    assert!(
        s.water.is_none() && s.notes.iter().any(|n| n.starts_with("water")),
        "{:?}",
        s.notes
    );
    // a file lost: prepared again; a bundle changed: prepared again
    std::fs::remove_file(dir.join("01_nam_common").join("Objects").join("Hut").join("Hut.xml")).unwrap();
    assert!(package::ready(&root, l).unwrap_err().contains("lost"));
    assert!(package::recorded(&root, l).is_ok());
    package::prepare(&root, l, &mut |_| {}, &AtomicBool::new(false)).unwrap();
    std::fs::write(
        root.join("game")
            .join("Package")
            .join("levels")
            .join("nam_x")
            .join("terrain-00.fbrb"),
        fbrb::build(&[], true),
    )
    .unwrap();
    assert!(package::recorded(&root, l).unwrap_err().contains("changed"));
    // the job: in process, steps and done
    let mut job = prepare::Job::start(&root, "nam_x", prepare::Plan::Package).unwrap();
    assert_eq!(job.wait(std::time::Duration::from_secs(60)), prepare::Outcome::Done);
    assert_eq!(job.progress.steps, 4);
    let _ = std::fs::remove_dir_all(&root);
}

// ------------------------------------------------------------------------------------------------ the real game copy

fn junction(link: &Path, target: &Path) {
    let made = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
}

#[test]
fn real_package_levels() {
    let _gate = crate::gate();
    let Some(which) = std::env::var_os("BC2_EDITOR_G3").map(|v| v.to_string_lossy().to_lowercase()) else {
        println!("skipped: set BC2_EDITOR_G3=all (or a comma list) to prepare and open the real Package levels");
        return;
    };
    let Ok(real) = levels::find_opfor(None) else {
        println!("skipped: no Project OPFOR BFBC2");
        return;
    };
    let root = std::env::var_os("BC2_EDITOR_G3_ROOT").map_or_else(|| temp("real"), PathBuf::from);
    let made_link = !root.join("game").exists();
    if made_link {
        std::fs::create_dir_all(&root).unwrap();
        junction(&root.join("game"), &real.join("game"));
    }
    let all = package::package_levels(&root);
    let chosen: Vec<&package::PackageLevel> = all
        .iter()
        .filter(|l| which == "all" || which.split(',').any(|w| w.trim() == l.lv))
        .collect();
    println!("{} installed Package levels, {} chosen", all.len(), chosen.len());
    let mut rows = Vec::new();
    for l in &chosen {
        let t = std::time::Instant::now();
        let p = match package::ready(&root, l) {
            Ok(p) => p,
            Err(_) => package::prepare(&root, l, &mut |_| {}, &AtomicBool::new(false)).unwrap(),
        };
        let prep = t.elapsed().as_secs_f64();
        let t = std::time::Instant::now();
        let found = levels::load_options(&root, &l.lv).unwrap();
        let s = scene::load(&found.options).unwrap();
        let open = t.elapsed().as_secs_f64();
        let teams = Teams::from_partition(s.file(0).unwrap().1, Profile::of(&l.lv));
        let c = s.collision.as_ref();
        let size: u64 = p.files.iter().map(|f| f.size).sum();
        let row = serde_json::json!({
            "level": l.lv, "profile": l.profile.word(), "entities": s.entities.len(),
            "guids": s.entities.iter().map(|e| e.guid.clone()).collect::<Vec<_>>(),
            "mesh_list": c.map_or_else(Vec::new, |c| c.meshes.iter().map(|(i, bp)| format!("{i} {bp}")).collect::<Vec<_>>()),
            "with_blueprint_type": s.entities.iter().filter(|e| e.blueprint_type.is_some()).count(),
            "meshes": c.map_or(0, |c| c.meshes.len()),
            "missing": c.map_or(0, |c| c.missing.len()),
            "triangles": c.map_or(0, |c| c.meshes.iter().filter_map(|(_, bp)| c.blueprints.get(bp).and_then(|t| t.as_ref()).map(|t| t.len())).sum::<usize>()),
            "terrain": s.heightfield.as_ref().map(|h| h.size),
            "terrain_name": p.terrain_name,
            "water_triangles": s.water.as_ref().map(|w| w.mesh.triangles.len()),
            "teams": [teams.label(1), teams.label(2)],
            "notes": s.notes.iter().chain(&found.notes).cloned().collect::<Vec<_>>(),
            "prepare_s": prep, "recorded_prepare_s": p.seconds, "open_s": open,
            "files": p.files.len(), "bytes": size, "stats": p.stats,
        });
        println!("{row}");
        rows.push(row);
    }
    if let Some(out) = std::env::var_os("BC2_EDITOR_G3_OUT") {
        std::fs::write(out, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
    }
    // "New map from an existing map" on Package levels: a project made through the editor's own flow, its profile
    // from the bundles, a move and a delete applied by levelwrite to the partition and nothing else changed
    if let Some(list) = std::env::var_os("BC2_EDITOR_G3_PROJECTS")
        && let Ok(mut h) = Headless::new([1400, 860])
    {
        for lv in list.to_string_lossy().split(',').map(str::trim) {
            let dir = temp(&format!("project_{lv}"));
            let mut app = EditorApp::new(Some(&root), None).unwrap();
            h.settle(&mut app, 2);
            let gpu = h.gpu.clone();
            app.open_level_blocking_then(&gpu, lv, Then::Create(format!("{lv} edit"), dir.clone()))
                .unwrap();
            h.settle(&mut app, 3);
            let v = app.view_mut().unwrap();
            let d = v.doc.as_mut().expect("a project");
            let want = match package::find(&root, lv).unwrap().profile {
                Profile::Vietnam => bc2_levelwrite::Profile::Vietnam,
                Profile::Ww2 => bc2_levelwrite::Profile::Ww2,
                Profile::Bfbc2 => bc2_levelwrite::Profile::Bfbc2,
            };
            assert_eq!(d.project.base.profile, want, "{lv}");
            let editable: Vec<usize> = d
                .scene
                .collision
                .as_ref()
                .unwrap()
                .meshes
                .iter()
                .map(|(k, _)| *k)
                .filter(|&k| d.why_not(k).is_none() && d.scene.entities[k].type_name.ends_with("ReferenceObjectData"))
                .take(2)
                .collect();
            let (a, b) = (editable[0], editable[1]);
            let (ga, gb) = (d.scene.entities[a].guid.clone(), d.scene.entities[b].guid.clone());
            let from = d.scene.entities[a].matrix.unwrap();
            let mut to = from;
            to[12] += 10.0;
            d.commit(
                Op::Move {
                    items: vec![Moved {
                        guid: ga.clone(),
                        from: bc2_levelwrite::rows_of(&from),
                        to: bc2_levelwrite::rows_of(&to),
                        placement: None,
                    }],
                },
                "move",
            );
            d.commit(
                Op::Delete {
                    guids: vec![gb.clone()],
                },
                "delete",
            );
            let base = EditDbx::parse(d.scene.file(0).unwrap().1).unwrap();
            let w = LevelWriter::parse(&d.partition().unwrap()).unwrap();
            assert_eq!(w.transform(&ga).unwrap(), bc2_levelwrite::rows_of(&to));
            let changes = diff(&base, &w.d);
            assert!(
                changes.contains(&Change::Field(ga.to_ascii_uppercase(), "Transform".into())),
                "{changes:?}"
            );
            // the delete: the instance gone and its entry in LevelData's Objects (DbxEdit's drop); nothing else
            assert!(
                changes.contains(&Change::Removed(gb.to_ascii_uppercase())),
                "{changes:?}"
            );
            // (and its event connections, where it had some)
            assert!(
                changes.iter().all(|c| match c {
                    Change::Removed(g) => *g == gb.to_ascii_uppercase(),
                    Change::Field(g, f) =>
                        (*g == ga.to_ascii_uppercase() && f == "Transform")
                            || f == "Objects"
                            || f == "EventConnections",
                    _ => false,
                }),
                "{lv}: {changes:?}"
            );
            assert!(
                changes
                    .iter()
                    .any(|c| matches!(c, Change::Field(_, f) if f == "Objects")),
                "{changes:?}"
            );
            let msg = d.check();
            assert!(msg.starts_with("2 operation(s) apply cleanly"), "{msg}");
            println!("{lv} project ({want:?}): moved {ga}, deleted {gb}; {changes:?}; {msg}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    // screenshots: the editor's own open (start screen -> prepare -> view), dark and light
    if let (Some(dir), Some(shots)) = (
        std::env::var_os("BC2_EDITOR_SHOTS"),
        std::env::var_os("BC2_EDITOR_G3_SHOTS"),
    ) && let Ok(mut h) = Headless::new([1600, 950])
    {
        {
            for lv in shots.to_string_lossy().split(',') {
                for theme in [ThemeId::Dark, ThemeId::Light] {
                    let mut app = EditorApp::new(Some(&root), None).unwrap();
                    app.theme = theme;
                    h.settle(&mut app, 2);
                    app.open_level_blocking(&h.gpu, lv.trim()).unwrap();
                    h.settle(&mut app, 25);
                    let p = Path::new(&dir).join(format!("g3_{}_{}.png", lv.trim(), theme.key()));
                    bc2_editor::render::write_png(&p, [1600, 950], &h.image().unwrap()).unwrap();
                    println!("saved {}", p.display());
                }
            }
            // the start screen with its Package levels and badges
            let mut app = EditorApp::new(Some(&root), None).unwrap();
            h.settle(&mut app, 4);
            let p = Path::new(&dir).join("g3_start_dark.png");
            bc2_editor::render::write_png(&p, [1600, 950], &h.image().unwrap()).unwrap();
        }
    }
    if made_link && std::env::var_os("BC2_EDITOR_G3_KEEP").is_none() {
        std::fs::remove_dir(root.join("game")).unwrap();
        assert!(real.join("game").is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }
}
