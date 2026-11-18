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
