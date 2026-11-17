//! The scene on a level built in the test: entities, blueprints, teams, outlines, static groups, collision, terrain,
//! water and the notes, and the editing calls; the errors where `level.load` raises.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    missing_docs
)]

use std::fs;
use std::path::{Path, PathBuf};

use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_formats::havok;
use bc2_scene::scene::{self, LoadOptions, SceneError, Source};

/// A field element of `name` with `text`.
fn text(f: &mut DbxFile, name: &str, value: &str) -> Element {
    Element::text(f, "field", &[("name", name)], value)
}

fn reference(f: &mut DbxFile, name: &str, r: &str) -> Element {
    Element::container(f, "field", &[("name", name), ("ref", r)], Vec::new())
}

fn transform(f: &mut DbxFile, name: &str, values: &[f32]) -> Element {
    let raw: Vec<u8> = values.iter().flat_map(|v| v.to_be_bytes()).collect();
    Element::numbers(f, "complex", &[("name", name)], 4, &raw)
}

fn at(x: f32, y: f32, z: f32) -> Vec<f32> {
    vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0]
}

fn array(f: &mut DbxFile, name: &str, refs: &[&str]) -> Element {
    let items = refs
        .iter()
        .map(|r| Element::container(f, "item", &[("ref", r)], Vec::new()))
        .collect();
    Element::container(f, "array", &[("name", name)], items)
}

fn instance(f: &mut DbxFile, guid: &str, type_name: &str, fields: Vec<Element>) -> Element {
    Element::container(f, "instance", &[("guid", guid), ("type", type_name)], fields)
}

fn write(path: &Path, f: &mut DbxFile, instances: Vec<Element>) {
    let part = Element::container(f, "partition", &[("guid", "P")], instances);
    f.roots.push(part);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, dbx::write(f).unwrap()).unwrap();
}

/// A level: spawns with teams, a placed crate (a dynamic model), a capture point's area (three points, a height),
/// a terrain entity; a static group with two rocks; a height field and water.
fn level(root: &Path) -> LoadOptions {
    let dist = root.join("dist");
    let mut f = DbxFile::default();
    let mut insts = Vec::new();
    let fields = vec![
        transform(&mut f, "Transform", &at(1.0, 2.0, 3.0)),
        text(&mut f, "Team", "Team1"),
        text(&mut f, "PlayerType", "SSAIPlayer"),
        text(&mut f, "Name", "Spawn"),
        array(&mut f, "AlternativeSpawnPoints", &["a1"]),
    ];
    insts.push(instance(&mut f, "S1", "Entity.SoldierSpawnEntityData", fields));
    let fields = vec![transform(&mut f, "Transform", &at(4.0, 5.0, 6.0))];
    insts.push(instance(&mut f, "A1", "Entity.AlternateSpawnEntityData", fields));
    let fields = vec![
        transform(&mut f, "Transform", &at(10.0, 0.0, 10.0)),
        reference(&mut f, "ReferencedObject", "Objects/Crate/Crate/BP"),
    ];
    insts.push(instance(&mut f, "R1", "Entity.ReferenceObjectData", fields));
    let fields = vec![
        transform(&mut f, "Transform", &at(0.0, 0.0, 0.0)),
        text(&mut f, "InitialOwnerTeam", "Team2"),
        array(&mut f, "Geometry", &["p1"]),
        array(&mut f, "RouteWaypoints", &["p1"]),
    ];
    insts.push(instance(&mut f, "C1", "Entity.CapturePointEntityData", fields));
    for (g, next, x) in [("P1", "P2", 0.0), ("P2", "P3", 5.0), ("P3", "P1", 9.0)] {
        let mut fields = vec![
            transform(&mut f, "Transform", &at(x, 1.0, x)),
            reference(&mut f, "Next", next),
        ];
        if g == "P2" {
            fields.push(transform(&mut f, "Height", &[25.0]));
        }
        insts.push(instance(&mut f, g, "Entity.AreaGeometryEntityData", fields));
    }
    let fields = vec![reference(&mut f, "TerrainAsset", "Terrains/Test/Test/T")];
    insts.push(instance(&mut f, "T1", "Entity.TerrainEntityData", fields));
    // a destructible house (its object in another partition) and an M-COM station (neutral)
    let fields = vec![
        transform(&mut f, "Transform", &at(0.0, 0.0, 50.0)),
        reference(&mut f, "ReferencedObject", "Objects/House/House/BP2"),
    ];
    insts.push(instance(&mut f, "R2", "Entity.ReferenceObjectData", fields));
    let fields = vec![
        transform(&mut f, "Transform", &at(0.0, 0.0, 60.0)),
        reference(&mut f, "ReferencedObject", "Objects/Objects/MpObjective/Mcom/BP3"),
        text(&mut f, "Team", "TeamNeutral"),
    ];
    insts.push(instance(&mut f, "R3", "Entity.ReferenceObjectData", fields));
    let partition = dist.join("levels").join("TEST.dbx");
    write(&partition, &mut f, insts);
    // the crate's blueprint: its object a dynamic model (a local reference)
    let mut b = DbxFile::default();
    let fields = vec![reference(&mut b, "Object", "OBJ")];
    let bp = instance(&mut b, "BP", "Entity.ObjectBlueprint", fields);
    let obj = instance(&mut b, "OBJ", "GameSharedResources.DynamicModelEntityData", Vec::new());
    write(&dist.join("Objects/Crate/Crate.dbx"), &mut b, vec![bp, obj]);
    let mut h = DbxFile::default();
    let fields = vec![
        reference(&mut h, "Object", "Objects/House/HouseObj/O2"),
        array(&mut h, "HealthStates", &["hs"]),
    ];
    let bp2 = instance(&mut h, "BP2", "Entity.ObjectBlueprint", fields);
    write(&dist.join("Objects/House/House.dbx"), &mut h, vec![bp2]);
    let mut o = DbxFile::default();
    let o2 = instance(&mut o, "O2", "GameSharedResources.StaticModelEntityData", Vec::new());
    write(&dist.join("Objects/House/HouseObj.dbx"), &mut o, vec![o2]);
    let mut m = DbxFile::default();
    let bp3 = instance(&mut m, "BP3", "GameSharedResources.StaticModelEntityData", Vec::new());
    write(&dist.join("Objects/Objects/MpObjective/Mcom.dbx"), &mut m, vec![bp3]);
    // a static group: two rocks
    let mut g = DbxFile::default();
    let mut vals = at(20.0, 0.0, 0.0);
    vals.extend(at(30.0, 0.0, 0.0));
    let fields = vec![
        transform(&mut g, "Transforms", &vals),
        array(
            &mut g,
            "Assets",
            &["Objects/Rock/Rock_Havok/X", "Objects/Rock/Rock_Havok/Y", "Extra/Z"],
        ),
    ];
    let group = dist.join("levels/test/StaticModelGroupEntity_HavokAsset.dbx");
    let gi = instance(&mut g, "G1", "Physics.CompositeHavokAsset", fields);
    write(&group, &mut g, vec![gi]);
    fs::create_dir_all(dist.join("Objects/Rock")).unwrap();
    fs::write(
        dist.join("Objects/Rock/Rock_Havok.res"),
        havok::build(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], &[[0, 1, 2]]),
    )
    .unwrap();
    // terrain: 4 x 4, and a water triangle with its shader
    let terrain = root.join("terrain/Test.Heightfield-000.res");
    fs::create_dir_all(terrain.parent().unwrap()).unwrap();
    let mut hf: Vec<u8> = [0.0f32, 0.0, 0.0, 40.0, 10.0, 40.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    hf.extend(4u32.to_le_bytes());
    hf.resize(0x31, 0);
    hf.extend((0..16u16).flat_map(|k| (k * 1000).to_le_bytes()));
    fs::write(&terrain, hf).unwrap();
    let le = |w: &[u32]| w.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let mut mesh = le(&[1, 3]);
    mesh.extend(
        [0.0f32, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0]
            .iter()
            .flat_map(|v| v.to_le_bytes()),
    );
    mesh.extend(le(&[3, 0, 1, 2]));
    let mut visual = le(&[10, 36, 0, 12, 0, 1, 2, 1]);
    visual.extend(b"Water/Sea\0\0Systems/Water/WaterOceanFragment\0");
    visual.extend(le(&[1, 0, 1, 0, 3, 0]));
    let (wm, wv) = (
        root.join("terrain/Test.Water.Mesh.res"),
        root.join("terrain/Test.Water.res"),
    );
    fs::write(&wm, mesh).unwrap();
    fs::write(&wv, visual).unwrap();
    LoadOptions {
        groups: vec![group],
        geometry: true,
        heightfield: Some(terrain),
        terrain_stride: 1,
        water: Some((wm, Some(wv))),
        ..LoadOptions::new("TEST", partition, vec![root.join("pkg"), dist])
    }
}

fn temp(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bc2_scene_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    root
}

#[test]
fn loads_a_level() {
    let root = temp("level");
    let options = level(&root);
    let mut s = scene::load(&options).unwrap();
    assert_eq!(s.name, "TEST");
    // 10 instances, then the group's two members (the third item has no transform)
    assert_eq!(s.entities.len(), 12);
    let e = &s.entities;
    assert_eq!(
        (e[0].category.as_str(), e[0].team, e[0].ai),
        ("soldier spawn", Some(1), Some(true))
    );
    assert_eq!(e[0].name.as_deref(), Some("Spawn"));
    assert_eq!(e[0].position(), Some([1.0, 2.0, 3.0]));
    assert_eq!(
        (e[1].team, e[1].ai),
        (Some(1), Some(true)),
        "an alternate spawn takes its spawner's team"
    );
    assert_eq!(e[2].blueprint.as_deref(), Some("objects/crate/crate"));
    assert_eq!(e[2].blueprint_type.as_deref(), Some("DynamicModelEntityData"));
    assert_eq!(e[2].category, "dynamic object");
    assert!(!e[2].destructible);
    assert_eq!((e[3].category.as_str(), e[3].team), ("capture point", Some(2)));
    assert_eq!(e[4].category, "area point");
    assert_eq!(e[7].category, "other");
    assert_eq!((e[8].category.as_str(), e[8].destructible), ("destructible", true));
    assert_eq!(e[8].blueprint_type.as_deref(), Some("StaticModelEntityData"));
    assert_eq!((e[9].category.as_str(), e[9].team), ("objective", Some(0)));
    assert_eq!(e[10].guid, "G1#0");
    assert_eq!(e[10].blueprint.as_deref(), Some("objects/rock/rock"));
    assert_eq!(e[10].partition, "StaticModelGroupEntity_HavokAsset");
    assert_eq!(e[10].group.as_deref(), Some("StaticModelGroupEntity_HavokAsset.dbx"));
    assert_eq!(
        e[11].source,
        Source::GroupMember {
            file: 1,
            instance: 0,
            item: 1
        }
    );
    assert_eq!(e[2].source, Source::Instance { instance: 2 });
    // the area is a ring with the second point's height; the route through the same points is open
    assert_eq!(s.shapes.len(), 2);
    assert_eq!(
        (s.shapes[0].kind, s.shapes[0].closed, s.shapes[0].height),
        ("area", true, 25.0)
    );
    assert_eq!(s.shapes[0].points.len(), 3);
    assert_eq!((s.shapes[1].kind, s.shapes[1].closed), ("route", false));
    // collision: the rocks have triangles, the crate has no packfile
    let c = s.collision.as_ref().unwrap();
    assert_eq!(
        c.meshes,
        vec![
            (10, "objects/rock/rock".to_owned()),
            (11, "objects/rock/rock".to_owned())
        ]
    );
    assert_eq!(c.missing.len(), 3);
    assert_eq!(s.world_triangles(10).unwrap()[0][1], [21.0, 0.0, 0.0]);
    assert!(s.world_triangles(2).is_none());
    // terrain and water
    assert_eq!(s.heightfield.as_ref().unwrap().size, 4);
    assert_eq!(s.terrain.as_ref().unwrap().0.len(), 16);
    let w = s.water.as_ref().unwrap();
    assert_eq!(w.shaders.as_ref().unwrap().0, vec!["Water/Sea".to_owned()]);
    assert!(s.notes.is_empty(), "{:?}", s.notes);
    // the kept partitions, and editing
    assert_eq!(s.file(0).unwrap().0, options.partition.as_path());
    assert!(s.file(5).is_none());
    assert_eq!(s.tree(0).unwrap().unwrap().instances().count(), 10);
    let mut m = s.entities[10].matrix.unwrap();
    m[12] = 50.0;
    assert!(s.set_transform(10, m));
    assert!(!s.set_transform(99, m));
    assert_eq!(s.changed().iter().copied().collect::<Vec<_>>(), vec![10]);
    assert_eq!(s.world_triangles(10).unwrap()[0][0], [50.0, 0.0, 0.0]);
    assert_eq!(
        scene::terrain_name(&options.partition).unwrap().as_deref(),
        Some("test")
    );
    // without geometry: no group members, collision, terrain or water
    let light = scene::load(&LoadOptions {
        geometry: false,
        ..options.clone()
    })
    .unwrap();
    assert_eq!(light.entities.len(), 10);
    assert!(light.collision.is_none() && light.heightfield.is_none() && light.water.is_none());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn notes_and_errors() {
    let root = temp("errors");
    let options = level(&root);
    // a damaged height field and water mesh are notes, as in level.load
    fs::write(options.heightfield.as_ref().unwrap(), b"short").unwrap();
    fs::write(&options.water.as_ref().unwrap().0, b"\x01\0\0\0").unwrap();
    // an XML file that cannot be read stops the mesh aliases (a note)
    fs::create_dir_all(root.join("dist/Objects/broken.xml")).unwrap();
    let s = scene::load(&options).unwrap();
    assert_eq!(s.notes.len(), 3, "{:?}", s.notes);
    assert!(s.notes[0].starts_with("blueprint mesh aliases not read"));
    assert!(s.notes[1].starts_with("terrain height field not read"));
    assert!(s.notes[2].starts_with("water not read"));
    // a damaged packfile is an error
    fs::write(root.join("dist/Objects/Rock/Rock_Havok.res"), havok::MAGIC).unwrap();
    assert!(matches!(scene::load(&options), Err(SceneError::Havok { .. })));
    // a missing, empty or non-binary partition is an error
    let missing = LoadOptions {
        partition: root.join("nope.dbx"),
        ..options.clone()
    };
    assert!(matches!(scene::load(&missing), Err(SceneError::Io { .. })));
    fs::write(&options.partition, b"").unwrap();
    assert!(matches!(scene::load(&options), Err(SceneError::Dbx { .. })));
    fs::write(&options.partition, b"<?xml version=\"1.0\"?><partition/>").unwrap();
    let e = scene::load(&options).unwrap_err();
    assert!(e.to_string().contains("not a binary"));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn bad_values_are_errors() {
    let root = temp("values");
    let options = level(&root);
    // a transform that is not 4-byte numbers
    let mut f = DbxFile::default();
    let t = Element::numbers(&mut f, "complex", &[("name", "Transform")], 2, &[0, 1, 0, 2]);
    let i = instance(&mut f, "X", "Entity.SoldierSpawnEntityData", vec![t]);
    write(&options.partition, &mut f, vec![i]);
    let e = scene::load(&options).unwrap_err();
    assert!(matches!(e, SceneError::Value { .. }), "{e}");
    assert!(e.to_string().contains("Transform"));
    // a point's height that is not 4-byte numbers, met while walking an area
    let mut f = DbxFile::default();
    let t = transform(&mut f, "Transform", &at(0.0, 0.0, 0.0));
    let g = array(&mut f, "Geometry", &["p"]);
    let c = instance(&mut f, "C", "Entity.CapturePointEntityData", vec![t, g]);
    let t = transform(&mut f, "Transform", &at(1.0, 0.0, 0.0));
    let h = Element::numbers(&mut f, "complex", &[("name", "Height")], 2, &[0, 1]);
    let p = instance(&mut f, "P", "Entity.AreaGeometryEntityData", vec![t, h]);
    write(&options.partition, &mut f, vec![c, p]);
    let e = scene::load(&options).unwrap_err();
    assert!(e.to_string().contains("Height"), "{e}");
    fs::remove_dir_all(&root).unwrap();
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    /// A damaged level partition (cut short, bytes changed) loads or gives an error, never a panic.
    #[test]
    fn damaged_partitions_never_panic(
        cut in 0usize..4000,
        flips in proptest::collection::vec((proptest::prelude::any::<usize>(), proptest::prelude::any::<u8>()), 0..6),
    ) {
        let root = temp(&format!("prop_{cut}_{}", flips.len()));
        let options = level(&root);
        let mut d = fs::read(&options.partition).unwrap();
        d.truncate(cut.max(1).min(d.len()));
        for (at, b) in flips {
            let k = at % d.len();
            d[k] = b;
        }
        fs::write(&options.partition, &d).unwrap();
        let _ = scene::load(&options);
        fs::remove_dir_all(&root).unwrap();
    }
}
