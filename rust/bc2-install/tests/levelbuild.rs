//! The WW2 level build's parts (round OR2, feature `levelbuild`) on a fake game copy and fake builds: the stock bundles'
//! order and the index (an unreadable bundle left out and not kept), our files (the later root winning, nested roots,
//! the skips), the closure (ours, stock partitions with their resources, the loaded bundles' resources, a level-local
//! copy, DICE's conventions, unresolved and dangling references), the staging and its manifest, a shared bundle's
//! patch and `ww2_common` (unpacked, entries replaced in place or added, packed), the model groups against their
//! members, the level's own edits and our copies' remaps, the description and the menus' level list through
//! DbxEdit. Project OPFOR's install oracle holds the whole on the real game copy.

#![cfg(feature = "levelbuild")]
#![allow(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::assert_is_empty,
    reason = "test assertions on known fixtures"
)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_install::levelbuild::closure::{self, ClosureSpec, Src, Take};
use bc2_install::levelbuild::index::{self, OursSpec};
use bc2_install::levelbuild::{OMap, describe, gates, levelfiles, stage};

/// The stock index is kept once per process (`index::stock_index`): the tests that read it or forget it run one at a
/// time, or one test's index replaces another's between its two reads.
static INDEX_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn index_lock() -> std::sync::MutexGuard<'static, ()> {
    INDEX_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

const G1: &str = "11111111-1111-1111-1111-111111111111";
const G2: &str = "22222222-2222-2222-2222-222222222222";
const G3: &str = "33333333-3333-3333-3333-333333333333";
const G9: &str = "99999999-9999-9999-9999-999999999999";

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2-levelbuild-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn put(p: &Path, data: &[u8]) -> String {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, data).unwrap();
    s(p)
}

fn bundle(p: &Path, items: &[(&str, &str, &[u8])]) -> String {
    put(p, &bc2_formats::fbrb::build(items, true))
}

/// A partition: its primary instance `guid` of `kind`, with `refs` (field name, target) and text fields.
fn partition(guid: &str, kind: &str, refs: &[(&str, &str)], texts: &[(&str, &str)]) -> Vec<u8> {
    let mut f = DbxFile::default();
    let mut kids = Vec::new();
    for (n, r) in refs {
        kids.push(Element::text(&mut f, "field", &[("name", n), ("ref", r)], ""));
    }
    for (n, t) in texts {
        kids.push(Element::text(&mut f, "field", &[("name", n)], t));
    }
    let inst = Element::container(&mut f, "instance", &[("guid", guid), ("type", kind)], kids);
    let part = Element::container(
        &mut f,
        "partition",
        &[("guid", G9), ("primaryInstance", guid)],
        vec![inst],
    );
    f.roots.push(part);
    dbx::write(&f).unwrap()
}

fn names(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

#[test]
fn stock_bundles_index_and_our_files() {
    let _index = index_lock();
    let d = tmp("index");
    let g = d.join("game");
    let p = |r: &str| g.join(r.replace('/', "\\"));
    bundle(
        &p("Package/levels/mp_009/level-00.fbrb"),
        &[("Objects/A.dbx", "<non-resource>", b"a")],
    );
    bundle(
        &p("Package/levels/wake_island_s/level-00.fbrb"),
        &[("x.dbx", "<non-resource>", b"x")],
    );
    bundle(
        &p("Package/levels/ons_mp_005/level-00.fbrb"),
        &[("x.dbx", "<non-resource>", b"x")],
    );
    bundle(
        &p("Package/async/ondemand_awards-00.fbrb"),
        &[("x.res", "ITexture", b"x")],
    );
    bundle(&p("Package/streaming_sounds-00.fbrb"), &[("x.res", "Wave", b"x")]);
    put(&p("Package/levels/broken/level-00.fbrb"), b"not a bundle");
    bundle(
        &p("Dist/win32/levels/mp_common/level-00.fbrb"),
        &[("Objects/A.dbx", "<non-resource>", b"b")],
    );
    bundle(
        &p("Dist/win32/async/startup-00.fbrb"),
        &[("Objects/B.res", "ITexture", b"c")],
    );
    let game = s(&g);
    let lvs = vec!["wake_island_s".to_owned(), "coral_sea".to_owned()];
    let got = index::stock_bundles(&game, "wake_island_s", &lvs);
    let rel: Vec<String> = got.iter().map(|b| b.trim_start_matches(&game).to_owned()).collect();
    assert_eq!(
        rel,
        [
            r"\Package\levels\broken\level-00.fbrb",
            r"\Package\levels\mp_009\level-00.fbrb",
            r"\Dist\win32\levels\mp_common\level-00.fbrb",
            r"\Dist\win32\async\startup-00.fbrb"
        ]
    );
    let od = index::ondemand_bundles(&game);
    assert_eq!(od.len(), 2);
    let ix = index::stock_index(&game, "wake_island_s", &lvs);
    assert_eq!(ix.unread.len(), 1, "{:?}", ix.unread);
    assert!(ix.unread[0].contains("broken"));
    assert_eq!(ix.bundles.len(), 3);
    // the first bundle holding a name wins
    let (b, n, t) = ix.first("objects/a.dbx").unwrap();
    assert!(b.ends_with(r"mp_009\level-00.fbrb") && n == "Objects/A.dbx" && t == "<non-resource>");
    // not kept while a bundle cannot be read: a fixed bundle is read on the next call
    bundle(
        &p("Package/levels/broken/level-00.fbrb"),
        &[("Objects/A.dbx", "<non-resource>", b"z")],
    );
    let ix2 = index::stock_index(&game, "wake_island_s", &lvs);
    assert!(ix2.unread.is_empty());
    assert!(ix2.first("objects/a.dbx").unwrap().0.contains("broken"));
    let ix3 = index::stock_index(&game, "wake_island_s", &lvs);
    assert!(std::sync::Arc::ptr_eq(&ix2, &ix3), "kept for the process");
    index::forget_index();

    // our files: the later root wins (a clash when the bytes differ), skips and nested roots
    let w = d.join("work");
    put(&w.join("a/Objects/X.dbx"), b"one");
    put(&w.join("a/Objects/Same.res"), b"same");
    put(&w.join("a/_work/Skip.dbx"), b"skip");
    put(&w.join("a/sub/Objects/N.dbx"), b"nested");
    put(&w.join("a/notes.txt"), b"not level data");
    put(&w.join("b/Objects/X.dbx"), b"two");
    put(&w.join("b/Objects/Same.res"), b"same");
    let spec = OursSpec {
        w: s(&w),
        roots: vec!["a".into(), "b".into(), "a/sub".into()],
        skip: HashMap::from([("a".to_owned(), vec!["_WORK/".to_owned()])]),
        vietnam_vo: false,
        stock_over_ours: vec![],
    };
    let (ours, clash) = index::our_files(&spec);
    let keys: Vec<&str> = ours.keys().collect();
    assert_eq!(keys, ["objects/same.res", "objects/x.dbx", "objects/n.dbx"]);
    assert!(ours.get("objects/x.dbx").unwrap().1.ends_with(r"b\Objects\X.dbx"));
    assert_eq!(clash, [r"Objects/X.dbx: a\Objects\X.dbx replaced by b\Objects\X.dbx"]);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn closure_walks_ours_stock_and_loaded() {
    let _index = index_lock();
    let d = tmp("closure");
    let g = d.join("game");
    let w = d.join("work");
    let p = |r: &str| g.join(r.replace('/', "\\"));
    let level = "levels/wake_island_s";
    // stock: objects/b (refs objects/c, a terrain water, a shader, a partition nowhere, and an instance b lacks)
    let b = partition(
        G2,
        "X.B",
        &[
            ("C", &format!("Objects/C/{G3}")),
            ("Water", &format!("terrains/t/t-water/{G1}")),
            ("Shader", &format!("levels/x/shaders/s/{G1}")),
            ("Gone", &format!("objects/missing/{G1}")),
            ("Self", &format!("objects/b/{G1}")),
        ],
        &[],
    );
    bundle(
        &p("Package/levels/mp_009/level-00.fbrb"),
        &[
            ("Objects/B.dbx", "<non-resource>", &b),
            ("Objects/B.res", "RigidMeshSet", b"mesh"),
            ("Objects/B_lod0_data.res", "MeshData", b"data"),
            ("Objects/B_Big.dbx", "<non-resource>", &partition(G1, "X.Y", &[], &[])),
            ("Objects/B_Big_lod0_data.res", "MeshData", b"big"),
            ("Objects/C.res", "ITexture", b"tex of the loaded c"),
            (
                "Levels/mp_009/Local/Thing.dbx",
                "<non-resource>",
                &partition(G1, "X.T", &[], &[]),
            ),
        ],
    );
    // loaded: objects/c (its partition loaded, its resource not)
    let loaded_bundle = bundle(
        &p("Dist/win32/levels/mp_common/level-00.fbrb"),
        &[("Objects/C.dbx", "<non-resource>", &partition(G3, "X.C", &[], &[]))],
    );
    // ours: the level (refs ours/p, objects/b, local/thing - the alias of mp_009's level-local copy - and a missing
    // one), ours/p with its mesh set (naming ours/unreached: a start), a texture of no partition, one not reached
    let lv = partition(
        G1,
        "GameSharedResources.LevelData",
        &[
            ("P", &format!("Ours/P/{G2}")),
            ("B", &format!("Objects/B/{G2}")),
            ("T", &format!("local/thing/{G1}")),
            ("M", &format!("ours/none/{G1}")),
        ],
        &[("Name", "Levels/Wake_Island_S")],
    );
    let mut ours = OMap::new();
    for (low, rel, data) in [
        (format!("{level}.dbx"), "Levels/Wake_Island_S.dbx", lv.clone()),
        ("ours/p.dbx".to_owned(), "Ours/P.dbx", partition(G2, "X.P", &[], &[])),
        (
            "ours/p_mesh.res".to_owned(),
            "Ours/P_Mesh.res",
            b"\0Ours/Unreached\0".to_vec(),
        ),
        (
            "ours/unreached.dbx".to_owned(),
            "Ours/Unreached.dbx",
            partition(G1, "X.U", &[], &[]),
        ),
        (
            "ours/lonely.dbx".to_owned(),
            "Ours/Lonely.dbx",
            partition(G1, "X.L", &[], &[]),
        ),
        (
            "textures/free.res".to_owned(),
            "Textures/Free.res",
            b"a texture of no partition".to_vec(),
        ),
    ] {
        let path = put(&w.join("b").join(rel.replace('/', "\\")), &data);
        ours.insert(&low, (rel.to_owned(), path));
    }
    let lvs = vec!["wake_island_s".to_owned()];
    let ix = index::stock_index(&s(&g), "wake_island_s", &lvs);
    let spec = ClosureSpec {
        game: s(&g),
        w: s(&w),
        level: level.into(),
        lv: "wake_island_s".into(),
        ww2_levels: lvs.clone(),
        level_build_roots: vec!["wake".into()],
        level_root: "wake".into(),
        loaded_bundles: vec![loaded_bundle],
        masked: vec![],
        masked_by: String::new(),
    };
    let loaded: HashSet<String> = ["objects/c.dbx".to_owned()].into();
    let pre = HashSet::new();
    let mut report = Vec::new();
    let c = closure::closure(
        &spec,
        &closure::Inputs {
            level_data: &lv,
            ours: &ours,
            index: &ix,
            loaded: &loaded,
            preloaded: &pre,
            starts: &[],
        },
        &mut report,
    )
    .unwrap();
    let keys: Vec<&str> = c.take.keys().collect();
    assert_eq!(
        keys,
        [
            "objects/b.dbx",
            "objects/b.res",
            "objects/b_lod0_data.res",
            "local/thing.dbx",
            "objects/c.res",
            "ours/p_mesh.res",
            "textures/free.res",
            "ours/p.dbx",
            "ours/unreached.dbx"
        ],
        "{report:#?}"
    );
    // a level-local copy by its alias: under the name the reference gives
    let t = c.take.get("local/thing.dbx").unwrap();
    assert_eq!(t.name, "local/thing.dbx");
    assert!(matches!(&t.src, Src::Stock(_, n) if n == "Levels/mp_009/Local/Thing.dbx"));
    // B_Big's resources are its own partition's, not B's
    assert!(!c.take.contains("objects/b_big_lod0_data.res"));
    assert_eq!(c.take.get("ours/p_mesh.res").unwrap().kind, None);
    assert_eq!(
        c.unresolved,
        [
            ("ours/none".to_owned(), level.to_owned(), "ours".to_owned()),
            ("objects/missing".to_owned(), "objects/b".to_owned(), "stock".to_owned())
        ]
    );
    assert_eq!(names(&c.shaders), ["levels/x/shaders/s"]);
    assert_eq!(names(&c.unreached), ["ours/lonely"]);
    assert_eq!(c.dangling.len(), 1);
    assert_eq!(c.dangling[0].0, "objects/b");
    assert_eq!(c.dangling[0].3, "stock");
    assert_eq!(
        report[0],
        "closure: 10 partitions reached; 3 ours, 2 stock partitions copied; 9 files in all (0 of ours also in a \
         loaded bundle)"
    );
    assert_eq!(report[1], r"       5  Package\levels\mp_009\level-00.fbrb");
    assert_eq!(report[2], "       4  ours");
    assert_eq!(report[3], "  our partitions not reached from the level: 1");
    index::forget_index();
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn staging_patches_and_common() {
    let d = tmp("stage");
    let src = put(&d.join("src.res"), b"from a file");
    let data = b"bytes".to_vec();
    let files = [
        stage::StageFile {
            low: "levels/x.dbx",
            name: "Levels/X.dbx",
            kind: Some("<non-resource>"),
            src: stage::StageSrc::Bytes(&data, 1),
        },
        stage::StageFile {
            low: "b.res",
            name: "B.res",
            kind: Some("ITexture"),
            src: stage::StageSrc::Path(&src),
        },
        stage::StageFile {
            low: "a.res",
            name: "A.res",
            kind: None,
            src: stage::StageSrc::Bytes(&data, 2),
        },
    ];
    let folder = s(&d.join("level"));
    let mut st = stage::Staged::default();
    let lines = stage::write_stage(&folder, &files, Some("levels/x.dbx"), &mut st).unwrap();
    assert_eq!(
        lines,
        [
            "A.res|None|00010000|5|5",
            "B.res|ITexture|00010000|11|11",
            "Levels/X.dbx|<non-resource>|00010000|5|5"
        ]
    );
    let man = fs::read_to_string(d.join("level/fbrb.manifest")).unwrap();
    assert_eq!(man, format!("{}{}\n", stage::MANIFEST_HEAD, lines.join("\n")));
    // written again: the same sources are not written again; a changed staged file is
    let a = d.join("level/A.res");
    fs::write(&a, b"edited").unwrap();
    stage::write_stage(&folder, &files, Some("levels/x.dbx"), &mut st).unwrap();
    assert_eq!(fs::read(&a).unwrap(), b"bytes");
    stage::pack_many(&[(folder.clone(), s(&d.join("level-00.fbrb")))]).unwrap();
    let h = bc2_gamefs::bundles::entries_path(d.join("level-00.fbrb")).unwrap();
    assert_eq!(h.entries.len(), 3);
    assert!(stage::pack_many(&[(s(&d.join("nothing")), s(&d.join("x.fbrb")))]).is_err());

    // a shared bundle: an entry replaced in place (its flags kept), one added at the end
    let g = d.join("game");
    let shared = bundle(
        &g.join(r"Package\async\startup-00.fbrb"),
        &[
            ("Menu/A.dbx", "<non-resource>", b"old a"),
            ("Menu/B.dbx", "<non-resource>", b"b"),
        ],
    );
    let na = put(&d.join("new_a.dbx"), b"new a!");
    let nc = put(&d.join("new_c.res"), b"c");
    let work = s(&d.join("work"));
    let mut report = Vec::new();
    let out = stage::patch_bundle(
        &s(&g),
        &work,
        &shared,
        &shared,
        &[
            ("Menu/C.res".into(), "ITexture".into(), nc),
            ("Menu/a.dbx".into(), "<non-resource>".into(), na.clone()),
        ],
        "startup",
        &mut report,
    )
    .unwrap();
    assert!(out.ends_with(r"work\startup-00.fbrb"));
    let h = bc2_gamefs::bundles::entries_path(&out).unwrap();
    let got: Vec<(&str, u32)> = h.entries.iter().map(|e| (e.name.as_str(), e.size)).collect();
    assert_eq!(got, [("Menu/A.dbx", 6), ("Menu/B.dbx", 1), ("Menu/C.res", 1)]);
    assert!(
        report[0].starts_with(r"Package\async\startup-00.fbrb: 1 added, 1 replaced ("),
        "{report:?}"
    );

    // ww2_common: nam_common's three bundles with the files in place (nam_common's spelling kept)
    for kind in stage::COMMON_KINDS {
        bundle(
            &g.join(format!(r"Package\levels\nam_common\{kind}-00.fbrb")),
            &[
                ("Menu/WeaponIcon.res", "ITexture", b"nam"),
                ("Other.dbx", "<non-resource>", b"o"),
            ],
        );
    }
    let files = HashMap::from([(
        "ingame".to_owned(),
        vec![("menu/weaponicon.res".to_owned(), "ITexture".to_owned(), na)],
    )]);
    let mut report = Vec::new();
    let built = stage::build_ww2_common(&s(&g), &work, &files, &mut report).unwrap();
    assert_eq!(built.len(), 3);
    assert_eq!(
        report[1],
        format!(
            "ww2_common/ingame-00: nam_common's 2 entries, 1 replaced, 0 added ({} bytes)",
            stage::thousands(fs::metadata(&built[1].1).unwrap().len())
        )
    );
    let h = bc2_gamefs::bundles::entries_path(&built[1].1).unwrap();
    assert_eq!(
        (h.entries[0].name.as_str(), h.entries[0].size),
        ("Menu/WeaponIcon.res", 6)
    );
    assert!(!d.join(r"work\ww2_common\ingame").exists(), "the stage folders removed");
    // again with nothing changed: the same bundles and lines, from the process's build
    let mut again = Vec::new();
    let built2 = stage::build_ww2_common(&s(&g), &work, &files, &mut again).unwrap();
    assert_eq!((built2, again), (built, report));
    let _ = fs::remove_dir_all(&d);
}

fn group(assets: &[&str], enums: &[u32]) -> Vec<u8> {
    let mut f = DbxFile::default();
    let items: Vec<Element> = assets
        .iter()
        .map(|a| Element::text(&mut f, "item", &[("ref", &format!("{a}/{G1}"))], ""))
        .collect();
    let arr = Element::container(&mut f, "array", &[("name", "Assets")], items);
    let vals: Vec<u8> = enums.iter().flat_map(|e| e.to_be_bytes()).collect();
    let en = Element::numbers(&mut f, "array", &[("name", "Enums")], 4, &vals);
    let inst = Element::container(
        &mut f,
        "instance",
        &[("guid", G1), ("type", "Physics.CompositeHavokAsset")],
        vec![arr, en],
    );
    let part = Element::container(
        &mut f,
        "partition",
        &[("guid", G9), ("primaryInstance", G1)],
        vec![inst],
    );
    f.roots.push(part);
    dbx::write(&f).unwrap()
}

fn physics(ids: &[u32]) -> Vec<u8> {
    let n = ids.len() as u32;
    let mut b = Vec::new();
    b.extend(n.to_le_bytes());
    b.extend(n.to_le_bytes());
    for i in ids {
        b.extend(i.to_le_bytes());
    }
    b.extend(gates::HAVOK_MAGIC);
    b.extend([0u8; 16]);
    b
}

#[test]
fn model_groups_against_their_members() {
    let d = tmp("groups");
    let lv = d.join("level");
    let g = r"Levels\WAKE_ISLAND_S";
    put(
        &lv.join(format!(r"{g}\StaticModelGroupEntity_HavokAsset.dbx")),
        &group(
            &[
                "objects/x/sandbag_havok",
                "objects/x/wall_havok",
                "objects/x/sandbag_havok",
            ],
            &[63, 64, 65],
        ),
    );
    put(
        &lv.join(format!(r"{g}\StaticModelGroupEntity_HavokAsset.res")),
        &physics(&[63, 64, 64, 65]),
    );
    put(
        &lv.join(format!(r"{g}\ForestEntity_HavokAsset.dbx")),
        &group(&["objects/t/acacia_havok"], &[70]),
    );
    put(
        &lv.join(format!(r"{g}\ForestEntity_HavokAsset.res")),
        &physics(&[70, 70, 70]),
    );
    put(&lv.join(r"Objects\x\Sandbag_Havok.res"), &physics(&[0]));
    put(
        &lv.join(r"Objects\t\Acacia_Havok.res"),
        &physics(&(0..13).collect::<Vec<u32>>()),
    );
    let wall = lv.join(r"Objects\x\Wall_Havok.res");
    put(&wall, &physics(&[0, 1]));
    let dir = s(&lv);
    let mut report = Vec::new();
    assert!(gates::model_group_check(&dir, &[], &mut report).unwrap(), "{report:?}");
    assert_eq!(
        report,
        ["model groups: 2 groups, 3 member objects checked against their group physics - ok"]
    );
    put(&wall, &physics(&(0..6).collect::<Vec<u32>>()));
    let mut report = Vec::new();
    assert!(!gates::model_group_check(&dir, &[], &mut report).unwrap());
    assert_eq!(
        report[1],
        r"  MISMATCH levels/wake_island_s/staticmodelgroupentity_havokasset.dbx: objects/x/wall_havok (ours) has 6 physics parts, the group expects 2 per instance"
    );
    // not staged: every loaded bundle's copy is checked
    fs::remove_file(&wall).unwrap();
    let b = bundle(
        &d.join(r"g\nam_common\level-00.fbrb"),
        &[("Objects/X/Wall_Havok.res", "HavokPhysicsData", &physics(&[0, 1, 2]))],
    );
    let loaded = vec![(
        b,
        HashMap::from([(
            "objects/x/wall_havok.res".to_owned(),
            "Objects/X/Wall_Havok.res".to_owned(),
        )]),
    )];
    let mut report = Vec::new();
    assert!(!gates::model_group_check(&dir, &loaded, &mut report).unwrap());
    assert!(
        report[1].contains(r"objects/x/wall_havok (nam_common\level-00.fbrb) has 3 physics parts"),
        "{report:?}"
    );
    let mut report = Vec::new();
    assert!(!gates::model_group_check(&dir, &[], &mut report).unwrap());
    assert!(report[1].ends_with("objects/x/wall_havok: no physics resource in the level bundle or a bundle it loads"));
    // no groups: not a level that passes
    let empty = d.join("empty");
    fs::create_dir_all(&empty).unwrap();
    let mut report = Vec::new();
    assert!(!gates::model_group_check(&s(&empty), &[], &mut report).unwrap());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn level_edits_and_remaps() {
    let d = tmp("edits");
    let w = d.join("work");
    put(
        &w.join(r"kits\Objects\Teams\WW2_US.dbx"),
        &partition(G2, "X.Team", &[], &[]),
    );
    let mut ours = OMap::new();
    let grid = put(
        &w.join(r"wake\levels\x\materials_xenon.dbx"),
        &partition(G3, "X.Grid", &[], &[]),
    );
    ours.insert(
        "levels/x/materials_xenon.dbx",
        ("levels/x/materials_xenon.dbx".to_owned(), grid),
    );
    let level = partition(
        G1,
        "GameSharedResources.LevelData",
        &[
            ("Team1", &format!("Objects/Teams/US/{G9}")),
            ("Grid", &format!("Levels/X/Materials_Xenon/{G9}")),
            ("Other", &format!("objects/other/{G9}")),
        ],
        &[("Name", "Levels/Old")],
    );
    // VoiceOvers / VoiceOverLines on the LevelData
    let mut f = dbx::parse(&level).unwrap();
    let vo_item = Element::text(&mut f, "item", &[("ref", &format!("sound/vo_triggers/bf/{G1}"))], "");
    let vo = Element::container(&mut f, "array", &[("name", "VoiceOvers")], vec![vo_item]);
    let lines = Element::container(&mut f, "array", &[("name", "VoiceOverLines")], vec![]);
    if let dbx::Body::Container(c) = &mut f.roots[0].body
        && let dbx::Body::Container(k) = &mut c[0].body
    {
        k.push(vo);
        k.push(lines);
    }
    let level = dbx::write(&f).unwrap();
    let chatter = put(&w.join(r"vo\chatter.dbx"), &partition(G3, "X.VO", &[], &[]));
    ours.insert(
        "sound/vo_triggers/sp/japanese/vo_sp_jp_generic.dbx",
        ("Sound/VO_Triggers/SP/Japanese/VO_SP_JP_Generic.dbx".to_owned(), chatter),
    );
    let spec = levelfiles::LevelSpec {
        level: "levels/x".into(),
        name: "X_NEW".into(),
        lv: "x".into(),
        w: s(&w),
        teams: vec![("objects/teams/us".into(), "Objects/Teams/WW2_US".into())],
        sky_dome: "objects/dome".into(),
        ..levelfiles::LevelSpec::default()
    };
    let mut report = Vec::new();
    let out = levelfiles::edit_level(&level, &ours, &spec, &mut report).unwrap();
    let x = dbx::to_xml(&out).unwrap();
    assert!(
        x.contains(&format!(r#"ref="objects/teams/ww2_us/{}""#, G2.to_lowercase())),
        "{x}"
    );
    assert!(x.contains(&format!(r#"ref="levels/x/materials_xenon/{}""#, G3.to_lowercase())));
    assert!(x.contains(&format!("objects/other/{G9}")));
    assert!(x.contains(">Levels/X_NEW<"));
    assert!(x.contains(&format!(
        "sound/vo_triggers/sp/japanese/vo_sp_jp_generic/{}",
        G3.to_lowercase()
    )));
    assert_eq!(
        report,
        [
            "level: BF1943's sky dome left out (0 placement)",
            "level: voice-over: DICE's Japanese battle chatter (VO_SP_JP_Generic) added",
            "level: 2 references re-pointed (teams -> WW2_US / WW2_JP, the material grid); voice-over: BF1943's (2 \
             trigger asset(s), 0 lines); LevelData named Levels/X_NEW"
        ]
    );
    // the edit again: the chatter is not added twice
    let mut report = Vec::new();
    let again = levelfiles::edit_level(&out, &ours, &spec, &mut report).unwrap();
    assert!(!report.iter().any(|l| l.contains("chatter")));
    assert_eq!(dbx::parse(&again).unwrap(), dbx::parse(&out).unwrap());

    // patch_refs: a remapped reference and a nulled one; nothing to remap gives the bytes back
    let to = HashMap::from([
        ("objects/other".to_owned(), Some(format!("objects/new/{G2}"))),
        ("objects/teams/ww2_us".to_owned(), None),
    ]);
    let (p, n) = levelfiles::patch_refs(&out, &to, &HashMap::new()).unwrap();
    assert_eq!(n, 2);
    let x = dbx::to_xml(&p).unwrap();
    assert!(x.contains(&format!(r#"ref="objects/new/{G2}""#)) && x.contains(r#"ref="null""#));
    assert_eq!(
        levelfiles::patch_refs(&out, &HashMap::new(), &HashMap::new()).unwrap(),
        (out.clone(), 0)
    );
    // drop_instances: the instance gone, references to it nulled
    let mut f = dbx::parse(&out).unwrap();
    levelfiles::drop_instances(&mut f, &[G1.to_lowercase()]);
    assert_eq!(f.instances().count(), 0);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn description_and_level_list_through_dbxedit() {
    let d = tmp("describe");
    let mut f = DbxFile::default();
    let ld_name = Element::text(&mut f, "field", &[("name", "Name")], "NAM_MP_002_NAME");
    let ld_desc = Element::text(&mut f, "field", &[("name", "Description")], "NAM_MP_002_DESC");
    let vo_item = Element::text(&mut f, "item", &[], "Sound/VO/intro");
    // a reference field as DICE's files hold it: the reference first, then the field's name
    let mut kids = vec![
        Element::text(&mut f, "field", &[("name", "Name")], "Levels/NAM_MP_002CQ_description"),
        Element::text(&mut f, "field", &[("name", "LevelName")], "Levels/NAM_MP_002CQ"),
    ];
    kids.push(Element::container(
        &mut f,
        "complex",
        &[("name", "LevelDescription")],
        vec![ld_name, ld_desc],
    ));
    kids.push(Element::text(
        &mut f,
        "field",
        &[("ref", &format!("sound/old/{G9}")), ("name", "Music")],
        "",
    ));
    kids.push(Element::text(
        &mut f,
        "field",
        &[("ref", &format!("tex/old/{G9}")), ("name", "MinimapTexture")],
        "",
    ));
    kids.push(Element::container(
        &mut f,
        "array",
        &[("name", "StartVO")],
        vec![vo_item],
    ));
    kids.push(Element::text(
        &mut f,
        "field",
        &[("name", "MatchmakingModID")],
        "VIETNAM",
    ));
    let inst = Element::container(
        &mut f,
        "instance",
        &[
            ("guid", G1),
            ("type", "GameSharedResources.LevelDescriptionAsset"),
            ("id", "NAM_MP_002CQ_description"),
        ],
        kids,
    );
    let part = Element::container(
        &mut f,
        "partition",
        &[("guid", G2), ("primaryInstance", G1)],
        vec![inst],
    );
    f.roots.push(part);
    let donor = dbx::write(&f).unwrap();
    let list = {
        let mut f = DbxFile::default();
        let items = ["MP_005.dbx", "coral_sea.dbx", "MP_005_description.dbx"]
            .iter()
            .map(|i| Element::text(&mut f, "item", &[], i))
            .collect();
        let arr = Element::container(&mut f, "array", &[("name", "Results")], items);
        let inst = Element::container(
            &mut f,
            "instance",
            &[("guid", G1), ("type", "Core.FileQueryData")],
            vec![arr],
        );
        let part = Element::container(
            &mut f,
            "partition",
            &[("guid", G2), ("primaryInstance", G1)],
            vec![inst],
        );
        f.roots.push(part);
        dbx::write(&f).unwrap()
    };
    let startup = bundle(
        &d.join(r"game\Package\async\startup-00.fbrb"),
        &[
            ("Levels/NAM_MP_002CQ_description.dbx", "<non-resource>", &donor),
            ("LevelListQuery.dbx", "<non-resource>", &list),
        ],
    );
    let minimap = put(&d.join("minimap.dbx"), &partition(G3, "X.Tex", &[], &[]));
    let music = put(&d.join("music.dbx"), &partition(G2, "X.Music", &[], &[]));
    let spec = describe::DescSpec {
        work: s(&d.join("work")),
        startup: startup.clone(),
        donor: "Levels/NAM_MP_002CQ_description.dbx".into(),
        name: "WAKE_ISLAND_S".into(),
        seed: "wake-description".into(),
        text: "WAKE_ISLAND_S_DESC".into(),
        minimap: ("textures/ui/minimap/wake".into(), minimap),
        music: ("sound/music/wake".into(), music),
        mode: None,
    };
    let mut report = Vec::new();
    let out = describe::description(&spec, &mut report).unwrap();
    let x = dbx::to_xml(&fs::read(&out).unwrap()).unwrap();
    let new_g1 = describe::ww2_guid(&["wake-description", G1]);
    assert!(x.contains(&format!(r#"guid="{new_g1}""#)), "{x}");
    for want in [
        ">Levels/WAKE_ISLAND_S_description<",
        ">Levels/WAKE_ISLAND_S<",
        ">WAKE_ISLAND_S_NAME<",
        ">WAKE_ISLAND_S_DESC<",
        ">PACIFIC<",
        r#"id="WAKE_ISLAND_S_description""#,
    ] {
        assert!(x.contains(want), "{want} not in {x}");
    }
    assert!(x.contains(&format!(r#"ref="sound/music/wake/{}""#, G2.to_lowercase())));
    assert!(!x.contains("Sound/VO/"));
    assert_eq!(report.len(), 1);
    // a donor whose edits do not all match is refused
    let mut bad = spec.clone();
    bad.donor = "LevelListQuery.dbx".into();
    assert!(
        describe::description(&bad, &mut Vec::new())
            .unwrap_err()
            .starts_with("description: nothing matched")
    );

    let ls = describe::ListSpec {
        work: s(&d.join("work")),
        source: startup,
        entry: "LevelListQuery.dbx".into(),
        ww2_levels: vec!["wake_island_s".into(), "coral_sea".into()],
        levels: vec![("wake_island_s".into(), "WAKE_ISLAND_S".into())],
    };
    let mut report = Vec::new();
    let out = describe::level_list_query(&ls, &mut report).unwrap();
    let x = dbx::to_xml(&fs::read(&out).unwrap()).unwrap();
    let items: Vec<&str> = x
        .match_indices("<item>")
        .map(|(i, _)| &x[i + 6..i + 6 + x[i + 6..].find('<').unwrap()])
        .collect();
    assert_eq!(
        items,
        [
            "MP_005.dbx",
            "MP_005_description.dbx",
            "wake_island_s.dbx",
            "wake_island_s_description.dbx"
        ]
    );
    assert_eq!(
        report,
        ["level list: LevelListQuery.dbx names WAKE_ISLAND_S (4 items) - the catalog the menus list levels from"]
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn takes_flags_and_waves() {
    let d = tmp("flags");
    let mut f = DbxFile::default();
    let name = Element::text(&mut f, "field", &[("name", "Name")], "Objects/M_Mesh");
    let occ = Element::scalar(&mut f, "field", 6, &[("name", "OccluderMeshEnable")], [1, 1]);
    let irr = Element::scalar(&mut f, "field", 6, &[("name", "IrradianceVolumeEnable")], [1, 1]);
    let inst = Element::container(
        &mut f,
        "instance",
        &[("guid", G1), ("type", "Render.RigidMeshAsset")],
        vec![name, occ, irr],
    );
    let part = Element::container(
        &mut f,
        "partition",
        &[("guid", G2), ("primaryInstance", G1)],
        vec![inst],
    );
    f.roots.push(part);
    let mesh = dbx::write(&f).unwrap();
    let mut take = OMap::new();
    take.insert(
        "objects/m_mesh.dbx",
        Take {
            name: "Objects/M_Mesh.dbx".into(),
            kind: Some("<non-resource>".into()),
            src: Src::Bytes(mesh),
        },
    );
    let have: HashSet<String> = ["objects/m_mesh_irradiance.res".to_owned()].into();
    let mut report = Vec::new();
    levelfiles::mesh_flags(&mut take, &have, &mut report).unwrap();
    assert_eq!(
        report,
        ["mesh flags: 1 switched off (their resource is in no bundle): ['Objects/M_Mesh.OccluderMeshEnable']"]
    );
    let Src::Bytes(b) = &take.get("objects/m_mesh.dbx").unwrap().src else {
        panic!()
    };
    let x = dbx::to_xml(b).unwrap();
    assert!(x.contains(r#"<field name="OccluderMeshEnable">false</field>"#), "{x}");
    assert!(x.contains(r#"<field name="IrradianceVolumeEnable">true</field>"#));

    // an on-demand wave made an ordinary one, its audio unwrapped
    let mut f = DbxFile::default();
    let od = Element::scalar(&mut f, "field", 6, &[("name", "IsOnDemand")], [1, 1]);
    let inst = Element::container(
        &mut f,
        "instance",
        &[("guid", G1), ("type", "Audio.WaveAsset")],
        vec![od],
    );
    let part = Element::container(
        &mut f,
        "partition",
        &[("guid", G2), ("primaryInstance", G1)],
        vec![inst],
    );
    f.roots.push(part);
    take.insert(
        "sound/x_wave.dbx",
        Take {
            name: "Sound/X_Wave.dbx".into(),
            kind: Some("<non-resource>".into()),
            src: Src::Bytes(dbx::write(&f).unwrap()),
        },
    );
    let mut wrapped = b"RES\x02Wave".to_vec();
    wrapped.resize(56, 0);
    wrapped.extend(5u32.to_le_bytes());
    wrapped.extend(5u32.to_le_bytes());
    wrapped.extend(b"audio and more");
    let ondemand = bundle(
        &d.join("ondemand_sounds-00.fbrb"),
        &[("Sound/X_Wave.res", "Wave", &wrapped)],
    );
    let mut report = Vec::new();
    levelfiles::ondemand_waves(&mut take, &HashSet::new(), &[ondemand], &mut report).unwrap();
    assert_eq!(
        report,
        [
            "on-demand waves: 1 wave partitions made ordinary level waves, 1 audio resources unwrapped into the level bundle"
        ]
    );
    let t = take.get("sound/x_wave.res").unwrap();
    assert_eq!(
        (t.name.as_str(), t.kind.as_deref(), &t.src),
        ("Sound/X_Wave.res", Some("Wave"), &Src::Bytes(b"audio".to_vec()))
    );
    let _ = fs::remove_dir_all(&d);
}
