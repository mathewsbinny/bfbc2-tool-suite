//! The WW2 material grid's merge (round OR4, feature `levelbuild`) on built grids: Isla Inocentes' materials in
//! DICE's order and the new ones sorted after, each pair's relation from DICE's grids first, then BF1943's, then a
//! stand-in; the relations' data copied under guids of their own; the partition under the clone's name and guids;
//! the written grid read back; the redirect; a grid that does not hold together refused. Project OPFOR's rebuild
//! oracle holds the whole on the real grids (`material_grid_*`).

#![cfg(feature = "levelbuild")]
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test assertions on known fixtures"
)]

use std::collections::HashSet;

use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_install::levelbuild::materialgrid::{MergeSpec, merge, redirect};

/// A grid partition: materials `(guid, name)`, relations `(guid, m1 guid, m2 guid, [data guids])` with a damage
/// instance per data guid, under a `MaterialGridData` listing them.
fn grid(part: &str, grid_guid: &str, mats: &[(&str, &str)], rels: &[(&str, &str, &str, &[&str])]) -> Vec<u8> {
    let mut f = DbxFile::default();
    let mut insts = Vec::new();
    let item = |f: &mut DbxFile, r: &str| Element::text(f, "item", &[("ref", r)], "");
    let mats_items: Vec<Element> = mats.iter().map(|(g, _)| item(&mut f, g)).collect();
    let rel_items: Vec<Element> = rels.iter().map(|(g, ..)| item(&mut f, g)).collect();
    let name = Element::text(&mut f, "field", &[("name", "Name")], "levels/mp_005/materials_win32");
    let ma = Element::container(&mut f, "array", &[("name", "Materials")], mats_items);
    let ra = Element::container(&mut f, "array", &[("name", "MaterialRelations")], rel_items);
    insts.push(Element::container(
        &mut f,
        "instance",
        &[("guid", grid_guid), ("type", "Entity.MaterialGridData")],
        vec![name, ma, ra],
    ));
    for (g, n) in mats {
        let nm = Element::text(&mut f, "field", &[("name", "Name")], n);
        insts.push(Element::container(
            &mut f,
            "instance",
            &[("guid", g), ("type", "Entity.MaterialData")],
            vec![nm],
        ));
    }
    for (g, a, b, data) in rels {
        let m1 = Element::text(&mut f, "field", &[("name", "Material1"), ("ref", a)], "");
        let m2 = Element::text(&mut f, "field", &[("name", "Material2"), ("ref", b)], "");
        let items: Vec<Element> = data.iter().map(|d| item(&mut f, d)).collect();
        let da = Element::container(&mut f, "array", &[("name", "Data")], items);
        insts.push(Element::container(
            &mut f,
            "instance",
            &[("guid", g), ("type", "Entity.MaterialRelationData")],
            vec![m1, m2, da],
        ));
        for d in *data {
            let dm = Element::text(&mut f, "field", &[("name", "DamageMultiplier")], "1.5");
            insts.push(Element::container(
                &mut f,
                "instance",
                &[("guid", d), ("type", "Entity.MaterialRelationDamageData")],
                vec![dm],
            ));
        }
    }
    let p = Element::container(
        &mut f,
        "partition",
        &[
            ("guid", part),
            ("primaryInstance", grid_guid),
            ("name", "levels/mp_005/materials_win32"),
        ],
        insts,
    );
    f.roots.push(p);
    dbx::write(&f).unwrap()
}

fn g(n: u32) -> String {
    format!("{n:08X}-0000-0000-0000-000000000000")
}

#[test]
fn the_merge_takes_dice_then_bf1943_then_a_stand_in() {
    let (tank, sand, rock, wood) = (g(1), g(2), g(3), g(4));
    // Isla Inocentes: Tank, Sand; Tank x Sand with one damage item
    let base = grid(
        &g(100),
        &g(101),
        &[(&tank, "Tank"), (&sand, "Sand")],
        &[(&g(10), &tank, &sand, &[&g(20)])],
    );
    // another BFBC2 grid: Wood (named by the level) and Wood x Tank
    let other = grid(
        &g(200),
        &g(201),
        &[(&wood, "Wood"), (&tank, "Tank")],
        &[(&g(11), &wood, &tank, &[&g(21)])],
    );
    // BF1943's grid: Rock, Sand; Rock x Sand, and its own Tank x Sand (DICE's is taken)
    let bf = grid(
        &g(300),
        &g(301),
        &[(&rock, "Rock"), (&sand, "Sand"), (&g(5), "Medium_Tank")],
        &[(&g(12), &rock, &sand, &[&g(22)]), (&g(13), &g(5), &sand, &[&g(23)])],
    );
    let names: HashSet<Vec<u8>> = [b"Wood".to_vec()].into_iter().collect();
    let stand = vec![("Medium_Tank".to_owned(), "Tank".to_owned())];
    let spec = MergeSpec {
        base: ("levels/mp_005/materials_win32", &base),
        others: vec![("levels/mp_009/materials_win32", &other)],
        bf: ("levels/wake_island_s/materials_xenon", &bf),
        names_in: &names,
        stand_in: &stand,
        clone: "levels/wake_island_s/materials_xenon",
    };
    let m = merge(&spec).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        m.report[0],
        "materials: 5 (Isla Inocentes 2, BF1943 Wake 3, BFBC2 others Wake names 1)"
    );
    // in the pairs' name order: BF1943's Medium_Tank x Sand and Rock x Sand, DICE's Sand x Tank (BF1943's own left)
    // and Tank x Wood; the stand-in Medium_Tank x Wood (Tank's relation against Wood)
    assert_eq!(
        m.report[1],
        "relations: 5 - 2 from BF1943, 2 from DICE (a BFBC2 grid), 1 from stand-in"
    );
    assert!(m.report[2].starts_with("grid guid "), "{}", m.report[2]);
    assert!(
        m.report[2].contains("('name', 'levels/wake_island_s/materials_xenon')"),
        "{}",
        m.report[2]
    );
    let f = dbx::parse(&m.data).unwrap();
    let insts: Vec<&Element> = f.instances().collect();
    // the grid, 5 materials, 5 relations, 5 damage copies
    assert_eq!(insts.len(), 16);
    assert_eq!(f.attr(insts[0], "type"), Some("Entity.MaterialGridData"));
    let mat_names: Vec<&str> = insts[1..6]
        .iter()
        .map(|i| f.text(f.child(i, "Name").unwrap()))
        .collect();
    // DICE's order first, the new ones sorted
    assert_eq!(mat_names, ["Tank", "Sand", "Medium_Tank", "Rock", "Wood"]);
    assert_eq!(m.instances, 16);
    // the same merge gives the same bytes
    assert_eq!(merge(&spec).unwrap().data, m.data);
}

#[test]
fn a_relation_with_one_unknown_side_names_null_and_a_broken_grid_is_refused() {
    let (tank, sand) = (g(1), g(2));
    // BF1943's relation names a material its grid lacks: the pair (Sand, any) is kept, its side written null (DICE's
    // grid needs a relation: its array's first item is the new items' model, as the Python's)
    let base = grid(
        &g(100),
        &g(101),
        &[(&tank, "Tank"), (&sand, "Sand")],
        &[(&g(10), &tank, &tank, &[])],
    );
    let bf = grid(
        &g(300),
        &g(301),
        &[(&sand, "Sand")],
        &[(&g(12), &sand, &g(9), &[&g(22)])],
    );
    let names = HashSet::new();
    let spec = MergeSpec {
        base: ("levels/mp_005/materials_win32", &base),
        others: vec![],
        bf: ("x", &bf),
        names_in: &names,
        stand_in: &[],
        clone: "c",
    };
    let m = merge(&spec).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(m.report[1], "relations: 2 - 1 from BF1943, 1 from DICE (a BFBC2 grid)");
    let f = dbx::parse(&m.data).unwrap();
    let rel = f
        .instances()
        .find(|i| f.attr(i, "type") == Some("Entity.MaterialRelationData"))
        .unwrap();
    assert_eq!(f.attr(f.child(rel, "Material2").unwrap(), "ref"), Some("null"));
    // a grid that is not a partition
    let spec = MergeSpec {
        base: ("levels/mp_005/materials_win32", b"not a grid"),
        ..spec
    };
    assert!(merge(&spec).is_err());
}

#[test]
fn redirect_points_at_the_new_partition() {
    let mut f = DbxFile::default();
    let a = Element::text(
        &mut f,
        "field",
        &[("name", "Sound"), ("ref", "sound/old/gravel/Gravel/AAAA")],
        "",
    );
    let b = Element::text(&mut f, "field", &[("name", "Other"), ("ref", "sound/kept/x/BBBB")], "");
    let inst = Element::container(&mut f, "instance", &[("guid", &g(1)), ("type", "T")], vec![a, b]);
    let p = Element::container(&mut f, "partition", &[("guid", &g(2))], vec![inst]);
    f.roots.push(p);
    let data = dbx::write(&f).unwrap();
    let primary = vec![(
        "sound/old/gravel/gravel".to_owned(),
        "sound/new/gravel/new/cccc".to_owned(),
        "sound/new/gravel/new".to_owned(),
    )];
    let (out, lines) = redirect(&data, &primary).unwrap();
    assert_eq!(
        lines,
        ["redirected sound/old/gravel/gravel -> sound/new/gravel/new (neither game ships it) x1"]
    );
    let x = dbx::to_xml(&out).unwrap();
    assert!(
        x.contains("sound/new/gravel/new/cccc") && x.contains("sound/kept/x/BBBB"),
        "{x}"
    );
}
