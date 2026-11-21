//! A small made-up scene for the tests and for a look at the view without a game copy: a terrain patch, a placed
//! object with collision, a team-1 spawn, a neutral flag with its capture area, a team-2 vehicle spawn and an area
//! point. Nothing in it comes from the game.

use std::collections::HashMap;
use std::sync::Arc;

use bc2_scene::scene::{Collision, Entity, Scene, Shape, Source};

fn matrix(at: [f64; 3]) -> [f64; 16] {
    let mut m = [0.0; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m[12] = at[0];
    m[13] = at[1];
    m[14] = at[2];
    m
}

fn entity(index: usize, ty: &str, category: &str, team: Option<u8>, name: Option<&str>, at: [f64; 3]) -> Entity {
    Entity {
        index,
        guid: format!("00000000-0000-0000-0000-{index:012}"),
        type_name: format!("Entity.{ty}"),
        short_type: ty.into(),
        category: category.into(),
        id: None,
        name: name.map(str::to_owned),
        partition: "levels/DEMO".into(),
        matrix: Some(matrix(at)),
        blueprint: matches!(category, "placed object" | "destructible").then(|| "objects/props/demo_crate".to_owned()),
        group: None,
        blueprint_type: None,
        destructible: false,
        team,
        ai: (category == "soldier spawn").then_some(false),
        source: Source::Instance { instance: index },
    }
}

/// The demo scene's teams, as a BFBC2 level's description would say them (US / RU; made up like the scene).
#[must_use]
pub fn teams() -> crate::factions::Teams {
    use crate::factions::{Profile, Side, Teams, faction_side};
    let side = |f: &str| {
        let (label, name) = faction_side(Profile::Bfbc2, f).unwrap_or(("?", "?"));
        Side {
            label: label.into(),
            name: name.into(),
            faction: Some(f.into()),
        }
    };
    Teams {
        team1: side("FactionUS"),
        team2: side("FactionRUS"),
        attacker: None,
        source: "The demo scene's made-up level description: FactionTeam1 FactionUS, FactionTeam2 FactionRUS.".into(),
        from_data: true,
    }
}

/// The demo scene's entities: 0 the house (with collision), 1 a team-1 soldier spawn, 2 a neutral flag, 3 a team-2
/// vehicle spawn, 4 an area point.
#[must_use]
pub fn scene() -> Scene {
    let mut s = Scene::default();
    s.name = "DEMO".into();
    s.entities = vec![
        entity(
            0,
            "ReferenceObjectData",
            "placed object",
            None,
            Some("House"),
            [0.0, 0.0, 0.0],
        ),
        entity(
            1,
            "AlternateSpawnEntityData",
            "soldier spawn",
            Some(1),
            Some("Spawn 1"),
            [-25.0, 0.0, 10.0],
        ),
        entity(
            2,
            "CapturePointEntityData",
            "capture point",
            Some(0),
            Some("Flag A"),
            [20.0, 0.0, -15.0],
        ),
        entity(
            3,
            "VehicleSpawnEntityData",
            "vehicle spawn",
            Some(2),
            Some("Tank"),
            [30.0, 0.0, 25.0],
        ),
        entity(
            4,
            "AreaGeometryEntityData",
            "area point",
            None,
            None,
            [-30.0, 0.0, -30.0],
        ),
    ];
    // a house: a 10 x 6 x 8 m box, as collision triangles in its own space
    let (x, y, z) = (5.0f32, 6.0f32, 4.0f32);
    let c = |i: usize| -> [f32; 3] {
        [
            if i & 4 == 0 { -x } else { x },
            if i & 2 == 0 { 0.0 } else { y },
            if i & 1 == 0 { -z } else { z },
        ]
    };
    let faces = [
        (0, 1, 3, 2),
        (4, 6, 7, 5),
        (0, 4, 5, 1),
        (2, 3, 7, 6),
        (0, 2, 6, 4),
        (1, 5, 7, 3),
    ];
    let mut tris = Vec::new();
    for (a, b, cc, d) in faces {
        tris.push([c(a), c(b), c(cc)]);
        tris.push([c(a), c(cc), c(d)]);
    }
    let mut blueprints = HashMap::new();
    blueprints.insert("objects/props/demo_crate".to_owned(), Some(Arc::new(tris)));
    s.collision = Some(Collision {
        blueprints,
        meshes: vec![(0, "objects/props/demo_crate".to_owned())],
        missing: Vec::new(),
        parts: HashMap::new(),
    });
    // terrain: 41 x 41 vertices over 100 m, gently rolling
    let n = 41u32;
    let mut v = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let (px, pz) = (f64::from(i) * 2.5 - 50.0, f64::from(j) * 2.5 - 50.0);
            let h = ((px * 0.05).sin() + (pz * 0.07).cos()) * 0.8 - 1.6;
            v.push([px as f32, h as f32, pz as f32]);
        }
    }
    let mut t = Vec::new();
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let a = j * n + i;
            t.push([a, a + n, a + 1]);
            t.push([a + 1, a + n, a + n + 1]);
        }
    }
    s.terrain = Some((v, t));
    // the flag's capture area: a ring with a 5 m prism
    let ring: Vec<[f64; 3]> = (0..12)
        .map(|k| {
            let a = f64::from(k) / 12.0 * std::f64::consts::TAU;
            [20.0 + 8.0 * a.cos(), 0.0, -15.0 + 8.0 * a.sin()]
        })
        .collect();
    s.shapes = vec![Shape {
        entity: 2,
        kind: "area",
        points: ring,
        closed: true,
        height: 5.0,
        field: "Geometry",
    }];
    s
}

/// The guid of the demo level's instance `k` (upper case, as DICE writes them).
#[must_use]
pub fn level_guid(k: u32) -> String {
    format!("DE300000-0000-0000-0000-{k:012}")
}

/// The blueprints the demo level places: (path, the reference's guid).
pub const LEVEL_BLUEPRINTS: [(&str, &str); 2] = [
    (
        "objects/buildings/demo_house/demo_house",
        "d0000000-0000-0000-0000-000000000001",
    ),
    (
        "objects/props/demo_crate/demo_crate",
        "d0000000-0000-0000-0000-000000000002",
    ),
];

/// The demo level's partition (made up, written with the suite's DBX writer): `LevelData` (every object in
/// `Objects`; one event connection, the flag to its map marker), then 1 a house (`ReferenceObjectData`, Health 100),
/// 2 and 3 crates (Health 25 and 30), 4 a team-1 spawn, 5 a neutral flag (`Geometry`: the ring 6-9, `MapMarker` 10),
/// 6-9 its capture area's points, 10 the marker, 11 a team-2 vehicle spawn.
///
/// # Panics
/// Never: the DBX writer takes what it is given.
#[must_use]
pub fn level_partition() -> Vec<u8> {
    use bc2_formats::dbx::{self, DbxFile, Element};
    let mut f = DbxFile::default();
    let g = level_guid;
    let lg = |k: u32| g(k).to_ascii_lowercase();
    let tr = |f: &mut DbxFile, p: [f32; 3]| {
        let mut t = Vec::new();
        for v in [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, p[0], p[1], p[2], 0.0f32,
        ] {
            t.extend(v.to_be_bytes());
        }
        Element::numbers(f, "complex", &[("name", "Transform")], 4, &t)
    };
    let items: Vec<Element> = (1..=11)
        .map(|k| Element::text(&mut f, "item", &[("ref", &lg(k))], ""))
        .collect();
    let objs = Element::container(&mut f, "array", &[("name", "Objects")], items);
    let src = Element::text(&mut f, "field", &[("ref", &lg(5)), ("name", "Source")], "");
    let tgt = Element::text(&mut f, "field", &[("ref", &lg(10)), ("name", "Target")], "");
    let sn = Element::text(&mut f, "field", &[("name", "Name")], "OnCaptured");
    let se = Element::container(&mut f, "complex", &[("name", "SourceEvent")], vec![sn]);
    let c = Element::container(&mut f, "complex", &[], vec![src, tgt, se]);
    let ec = Element::container(&mut f, "array", &[("name", "EventConnections")], vec![c]);
    let lname = Element::text(&mut f, "field", &[("name", "Name")], "Levels/DEMO");
    let ld = Element::container(
        &mut f,
        "instance",
        &[
            ("guid", &g(0)),
            ("type", "GameSharedResources.LevelData"),
            ("id", "DEMO"),
        ],
        vec![lname, objs, ec],
    );
    let mut insts = vec![ld];
    let mut inst = |f: &mut DbxFile, k: u32, ty: &str, kids: Vec<Element>| {
        insts.push(Element::container(
            f,
            "instance",
            &[("guid", &g(k)), ("type", ty)],
            kids,
        ));
    };
    // the house and the crates
    for (k, at, bp, health) in [
        (1u32, [0.0f32, -1.0, 0.0], 0usize, 100.0f32),
        (2, [10.0, -1.0, 12.0], 1, 25.0),
        (3, [14.0, -1.0, 12.0], 1, 30.0),
    ] {
        let (path, guid) = LEVEL_BLUEPRINTS.get(bp).copied().unwrap_or_default();
        let en = Element::numbers(
            &mut f,
            "field",
            &[("name", "Enumeration")],
            4,
            &(k as i32).to_be_bytes(),
        );
        let name = Element::text(&mut f, "field", &[("name", "Name")], "");
        let t = tr(&mut f, at);
        let r = Element::text(
            &mut f,
            "field",
            &[("ref", &format!("{path}/{guid}")), ("name", "ReferencedObject")],
            "",
        );
        let h = Element::numbers(&mut f, "field", &[("name", "Health")], 4, &health.to_be_bytes());
        inst(&mut f, k, "Entity.ReferenceObjectData", vec![en, name, t, r, h]);
    }
    // the spawn
    let name = Element::text(&mut f, "field", &[("name", "Name")], "Spawn 1");
    let t = tr(&mut f, [-25.0, 0.0, 10.0]);
    let team = Element::text(&mut f, "field", &[("name", "Team")], "Team1");
    inst(
        &mut f,
        4,
        "GameSharedResources.AlternateSpawnEntityData",
        vec![name, t, team],
    );
    // the flag
    let name = Element::text(&mut f, "field", &[("name", "Name")], "Flag A");
    let t = tr(&mut f, [20.0, 0.0, -15.0]);
    let geo_item = Element::text(&mut f, "item", &[("ref", &lg(6))], "");
    let geo = Element::container(&mut f, "array", &[("name", "Geometry")], vec![geo_item]);
    let marker = Element::text(&mut f, "field", &[("ref", &lg(10)), ("name", "MapMarker")], "");
    let owner = Element::text(&mut f, "field", &[("name", "InitialOwnerTeam")], "TeamNeutral");
    inst(
        &mut f,
        5,
        "GameSharedResources.CapturePointEntityData",
        vec![name, t, geo, marker, owner],
    );
    // the capture area: a ring of four points, Next links
    for (k, at) in [
        (6u32, [28.0f32, 0.0, -15.0]),
        (7, [20.0, 0.0, -7.0]),
        (8, [12.0, 0.0, -15.0]),
        (9, [20.0, 0.0, -23.0]),
    ] {
        let t = tr(&mut f, at);
        let next = if k == 9 { 6 } else { k + 1 };
        let n = Element::text(&mut f, "field", &[("ref", &lg(next)), ("name", "Next")], "");
        let h = Element::numbers(&mut f, "field", &[("name", "Height")], 4, &5.0f32.to_be_bytes());
        inst(&mut f, k, "Entity.AreaGeometryEntityData", vec![t, n, h]);
    }
    let t = tr(&mut f, [20.0, 0.0, -15.0]);
    inst(&mut f, 10, "GameSharedResources.MapMarkerEntityData", vec![t]);
    let name = Element::text(&mut f, "field", &[("name", "Name")], "Tank");
    let t = tr(&mut f, [30.0, 0.0, 25.0]);
    let team = Element::text(&mut f, "field", &[("name", "Team")], "Team2");
    inst(
        &mut f,
        11,
        "GameSharedResources.VehicleSpawnEntityData",
        vec![name, t, team],
    );
    let part = Element::container(
        &mut f,
        "partition",
        &[("guid", "DE300000-FFFF-0000-0000-000000000000")],
        insts,
    );
    f.roots.push(part);
    dbx::write(&f).unwrap_or_default()
}

/// The demo level with its partition: [`level_partition`] written to `dir` and loaded as a level is (so its edits
/// go through levelwrite), with [`scene`]'s terrain and box collision for the house and the crates.
///
/// # Errors
/// The folder cannot be written, or the partition does not load.
pub fn level(dir: &std::path::Path) -> Result<Scene, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join("DEMO.dbx");
    std::fs::write(&file, level_partition()).map_err(|e| e.to_string())?;
    let mut s = bc2_scene::scene::load(&bc2_scene::scene::LoadOptions::new("DEMO", &file, Vec::new()))
        .map_err(|e| e.to_string())?;
    s.terrain = scene().terrain;
    let boxed = |x: f32, y: f32, z: f32| {
        let c = |i: usize| -> [f32; 3] {
            [
                if i & 4 == 0 { -x } else { x },
                if i & 2 == 0 { 0.0 } else { y },
                if i & 1 == 0 { -z } else { z },
            ]
        };
        let mut tris = Vec::new();
        for (a, b, cc, dd) in [
            (0, 1, 3, 2),
            (4, 6, 7, 5),
            (0, 4, 5, 1),
            (2, 3, 7, 6),
            (0, 2, 6, 4),
            (1, 5, 7, 3),
        ] {
            tris.push([c(a), c(b), c(cc)]);
            tris.push([c(a), c(cc), c(dd)]);
        }
        Some(Arc::new(tris))
    };
    let (house, crate_) = (LEVEL_BLUEPRINTS[0].0, LEVEL_BLUEPRINTS[1].0);
    let mut blueprints = HashMap::new();
    blueprints.insert(house.to_owned(), boxed(5.0, 6.0, 4.0));
    blueprints.insert(crate_.to_owned(), boxed(0.5, 1.0, 0.5));
    let mut meshes = Vec::new();
    for e in &mut s.entities {
        if let Some(bp) = e.blueprint.clone() {
            e.blueprint_type = Some("StaticModelEntityData".into());
            e.destructible = bp == house;
            meshes.push((e.index, bp));
        }
    }
    s.collision = Some(Collision {
        blueprints,
        meshes,
        missing: Vec::new(),
        parts: HashMap::new(),
    });
    Ok(s)
}
