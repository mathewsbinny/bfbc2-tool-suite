//! The offline sampler on geometry built here (no game data): the reference's cases
//! (`bfbc2-auto-navkit/tests/test_navsample_offline.py`) - the cast's layers, the 0.1 m gap and the step through
//! thin solids, nothing under the terrain, convex pieces solid, mesh triangles hit from both sides, the sphere
//! touching an edge, the capsule against walls, ceilings and steep terrain, the file, the grid - and the vehicle's
//! cylinder.

#![allow(clippy::unwrap_used, clippy::float_cmp, clippy::indexing_slicing, reason = "tests")]

use super::geom::{self, Tri};
use super::{
    BOTTOM, Body, Geometry, Grid, KIND_CONVEX, KIND_MESH, KIND_TERRAIN, RAY_RADIUS, RAY_SHAPE_VTABLE, Samples,
    VEHICLE_RADIUS, VTABLE_OF_KIND, capsule_axis, level, quantise, sample,
};

/// A height field over `[lo, hi]^2` with heights `f(x, z)`.
fn field(f: impl Fn(f64, f64) -> f64, lo: f64, hi: f64, size: usize) -> level::Terrain {
    let cell = (hi - lo) / size as f64;
    let mut h = Vec::with_capacity(size * size);
    for j in 0..size {
        for i in 0..size {
            h.push(f(lo + i as f64 * cell, lo + j as f64 * cell) as f32);
        }
    }
    level::Terrain {
        heights: h,
        size,
        min: (lo, lo),
        cell: (cell, cell),
    }
}

fn flat() -> level::Terrain {
    field(|_, _| 0.0, -20.0, 20.0, 64)
}

/// A closed box as 12 triangles, wound outward.
fn slab(x0: f64, z0: f64, x1: f64, z1: f64, y0: f64, y1: f64) -> Vec<Tri> {
    let mut c = Vec::new();
    for x in [x0, x1] {
        for y in [y0, y1] {
            for z in [z0, z1] {
                c.push([x, y, z]);
            }
        }
    }
    let centre = [f64::midpoint(x0, x1), f64::midpoint(y0, y1), f64::midpoint(z0, z1)];
    let faces = [
        (0, 1, 3),
        (0, 3, 2),
        (4, 6, 7),
        (4, 7, 5),
        (0, 4, 5),
        (0, 5, 1),
        (2, 3, 7),
        (2, 7, 6),
        (0, 2, 6),
        (0, 6, 4),
        (1, 5, 7),
        (1, 7, 3),
    ];
    faces
        .iter()
        .map(|&(a, b, d)| {
            let t = [c[a], c[b], c[d]];
            let n = geom::tri_cross(&t);
            let m = [
                (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                (t[0][1] + t[1][1] + t[2][1]) / 3.0,
                (t[0][2] + t[1][2] + t[2][2]) / 3.0,
            ];
            let out = n[0] * (m[0] - centre[0]) + n[1] * (m[1] - centre[1]) + n[2] * (m[2] - centre[2]);
            if out < 0.0 { [t[0], t[2], t[1]] } else { t }
        })
        .collect()
}

fn geometry(pieces: &[(Vec<Tri>, u8)]) -> Geometry {
    let mut g = Geometry::default();
    for (k, (t, kind)) in pieces.iter().enumerate() {
        g.tris.extend(t.iter().copied());
        g.kind.extend(std::iter::repeat_n(*kind, t.len()));
        g.solid.extend(std::iter::repeat_n(
            if *kind == KIND_CONVEX { k as i32 } else { -1 },
            t.len(),
        ));
        g.vt.extend(std::iter::repeat_n(*kind, t.len()));
    }
    g
}

fn grid() -> Grid {
    Grid {
        pitch: 0.8,
        min_x: -4.0,
        min_z: -4.0,
        nx: 11,
        nz: 11,
        y_bottom: -10.0,
        y_top: 50.0,
        mask: None,
    }
}

fn small(y_bottom: f64, y_top: f64) -> Grid {
    Grid {
        pitch: 0.8,
        min_x: -1.6,
        min_z: -1.6,
        nx: 5,
        nz: 5,
        y_bottom,
        y_top,
        mask: None,
    }
}

/// A column's surfaces: (y to 3 places, normal y to 2, flags, kind).
fn column(s: &Samples, x: f64, z: f64) -> Vec<(f64, f64, u8, u8)> {
    let i = ((x - s.header.min_x) / s.header.pitch).round() as usize;
    let j = ((z - s.header.min_z) / s.header.pitch).round() as usize;
    let c = j * s.header.nx as usize + i;
    let start: usize = s.counts[..c].iter().map(|&n| usize::from(n)).sum();
    (start..start + usize::from(s.counts[c]))
        .map(|k| {
            let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
            let r2 = |v: f64| (v * 100.0).round() / 100.0;
            (
                r3(f64::from(s.y[k])),
                r2(f64::from(s.normal[k][1]) / 127.0),
                s.flags[k],
                s.kind[k],
            )
        })
        .collect()
}

fn run(g: &Grid, geo: &Geometry, t: Option<&level::Terrain>, bodies: &[Body]) -> Vec<Samples> {
    let f = t.map(level::Terrain::field);
    sample(g, geo, f.as_ref(), bodies).unwrap().0
}

fn heights(c: &[(f64, f64, u8, u8)]) -> Vec<f64> {
    c.iter().map(|s| s.0).collect()
}

#[test]
fn the_cast_layers() {
    let (g, t) = (grid(), flat());
    let soldier = [Body::soldier(BOTTOM)];
    // a deck 0.3 m thick at 3 m: its top, its underside (hit from below, normal facing the ray), the terrain
    let s = &run(
        &g,
        &geometry(&[(slab(-2.1, -2.1, 2.1, 2.1, 2.7, 3.0), KIND_MESH)]),
        Some(&t),
        &soldier,
    )[0];
    let col = column(s, 0.0, 0.0);
    assert_eq!(heights(&col), [3.0, 2.7, 0.0], "{col:?}");
    assert!(col.iter().all(|c| c.1 > 0.99));
    assert_eq!((col[0].3, col[2].3), (KIND_MESH, KIND_TERRAIN));
    assert_eq!((col[1].2, col[2].2), (0, 0), "the capsule fits under a 2.7 m underside");
    assert_eq!(column(s, 3.2, 3.2), [(0.0, 1.0, 0, KIND_TERRAIN)]);
    // a thin slab: its underside is hit at once from 0.1 under the top and stepped through
    let s = &run(
        &g,
        &geometry(&[(slab(-2.1, -2.1, 2.1, 2.1, 2.93, 3.0), KIND_MESH)]),
        Some(&t),
        &soldier,
    )[0];
    assert_eq!(heights(&column(s, 0.0, 0.0)), [3.0, 0.0]);
    // nothing under the terrain
    let s = &run(
        &g,
        &geometry(&[(slab(-2.1, -2.1, 2.1, 2.1, -3.0, -2.0), KIND_MESH)]),
        Some(&t),
        &soldier,
    )[0];
    assert_eq!(heights(&column(s, 0.0, 0.0)), [0.0]);
    // a convex block is solid: its top only; the same block as a mesh is hollow
    let s = &run(
        &g,
        &geometry(&[(slab(-2.1, -2.1, 2.1, 2.1, 0.0, 3.0), KIND_CONVEX)]),
        Some(&t),
        &soldier,
    )[0];
    let col = column(s, 0.0, 0.0);
    assert_eq!(heights(&col), [3.0]);
    assert_eq!(col[0].3, KIND_CONVEX);
    let s = &run(
        &g,
        &geometry(&[(slab(-2.1, -2.1, 2.1, 2.1, 0.0, 3.0), KIND_MESH)]),
        Some(&t),
        &soldier,
    )[0];
    assert_eq!(heights(&column(s, 0.0, 0.0)), [3.0, 0.0]);
    // a thin vertical sheet 0.03 m beside a column: its top edge hit with an upward-ish normal
    let sheet = vec![
        [[0.03, 0.0, -2.0], [0.03, 1.0, -2.0], [0.03, 1.0, 2.0]],
        [[0.03, 0.0, -2.0], [0.03, 1.0, 2.0], [0.03, 0.0, 2.0]],
    ];
    let s = &run(&g, &geometry(&[(sheet, KIND_MESH)]), Some(&t), &soldier)[0];
    let col = column(s, 0.0, 0.0);
    assert!(col[0].0 == 1.0 && col[0].1 > 0.5, "{col:?}");
    assert_eq!(heights(&column(s, 0.8, 0.0)), [0.0]);
}

#[test]
fn the_capsule() {
    let (g, t) = (grid(), flat());
    let soldier = [Body::soldier(BOTTOM)];
    let s = &run(
        &g,
        &geometry(&[(slab(0.3, -4.0, 0.5, 4.0, -1.0, 5.0), KIND_MESH)]),
        Some(&t),
        &soldier,
    )[0];
    assert_eq!(column(s, 0.0, 0.0)[0].2, 1, "a wall 0.3 m away blocks");
    assert_eq!(column(s, -0.8, 0.0)[0].2, 0, "1.1 m away it does not");
    for (top, want) in [(1.7, 1), (2.6, 0)] {
        let s = &run(
            &g,
            &geometry(&[(slab(-4.0, -4.0, 4.0, 4.0, top, top + 0.3), KIND_MESH)]),
            Some(&t),
            &soldier,
        )[0];
        assert_eq!(column(s, 0.0, 0.0).last().unwrap().2, want, "ceiling {top} m up");
    }
    // steep terrain: the normal run is blocked from about 64 degrees
    for (deg, want) in [(30.0f64, 0), (55.0, 0), (72.0, 1)] {
        let tan = deg.to_radians().tan();
        let hf = field(|x, _| x * tan, -20.0, 20.0, 128);
        let s = &run(&small(-100.0, 100.0), &Geometry::default(), Some(&hf), &soldier)[0];
        let col = column(s, 0.0, 0.0);
        assert!(
            col.len() == 1 && col[0].2 == want && (col[0].1 - deg.to_radians().cos()).abs() < 0.02,
            "{deg}: {col:?}"
        );
    }
    // the terrain hit is lifted by RAY_RADIUS * (1 - ny) on a slope
    let hf = field(|x, _| x, -20.0, 20.0, 128);
    let s = &run(&small(-100.0, 100.0), &Geometry::default(), Some(&hf), &soldier)[0];
    let y = column(s, 0.0, 0.0)[0].0;
    let want = hf.field().height(0.0, 0.0) + RAY_RADIUS * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
    assert!((y - want).abs() < 0.002, "{y} {want}");
}

#[test]
fn the_vehicle_cylinder() {
    let (g, t) = (grid(), flat());
    let both = [Body::soldier(BOTTOM), Body::vehicle()];
    // a wall 2 m from the column: the soldier fits, the vehicle (radius 2.5) does not
    let s = run(
        &g,
        &geometry(&[(slab(2.0, -4.0, 2.2, 4.0, -1.0, 5.0), KIND_MESH)]),
        Some(&t),
        &both,
    );
    assert_eq!(column(&s[0], 0.0, 0.0)[0].2, 0);
    assert_eq!(column(&s[1], 0.0, 0.0)[0].2, 1);
    assert_eq!(s[1].header.radius, VEHICLE_RADIUS);
    // a kerb 0.3 m high passes under the vehicle's step (0.4), a 0.6 m block does not
    for (top, want) in [(0.3, 0), (0.6, 1)] {
        let s = run(
            &g,
            &geometry(&[(slab(1.0, -4.0, 3.0, 4.0, 0.0, top), KIND_MESH)]),
            Some(&t),
            &both,
        );
        assert_eq!(column(&s[1], -0.8, 0.0)[0].2, want, "block {top} m high");
    }
    // a 20 degree slope does not block it (the plane test), a 1.5 m bump 2 m away does
    let tan = 20f64.to_radians().tan();
    let hf = field(|x, _| x * tan, -20.0, 20.0, 128);
    let s = run(&small(-100.0, 100.0), &Geometry::default(), Some(&hf), &both);
    assert_eq!(column(&s[1], 0.0, 0.0)[0].2, 0);
    let hf = field(
        |x, z| {
            if (x - 2.0).abs() < 0.5 && z.abs() < 0.5 {
                1.5
            } else {
                0.0
            }
        },
        -20.0,
        20.0,
        128,
    );
    let s = run(&small(-100.0, 100.0), &Geometry::default(), Some(&hf), &both);
    assert_eq!(column(&s[1], 0.0, 0.0)[0].2, 1);
}

#[test]
fn the_file_is_write_v3s() {
    let (g, t) = (grid(), flat());
    let geo = geometry(&[(slab(-2.1, -2.1, 2.1, 2.1, 2.7, 3.0), KIND_MESH)]);
    let s = &run(&g, &geo, Some(&t), &[Body::soldier(0.6)])[0];
    let data = s.file_bytes().unwrap();
    let runs = crate::navsample::parse_v3(&data, 11, 11).unwrap();
    assert_eq!(runs.y, s.y);
    // write_v3 of the same arrays (the reference's writer) gives the same bytes
    let mut start = vec![0i64];
    for &c in &s.counts {
        start.push(start.last().unwrap() + i64::from(c));
    }
    let starts: Vec<u8> = start.iter().flat_map(|v| v.to_le_bytes()).collect();
    let ys: Vec<u8> = s.y.iter().flat_map(|v| v.to_le_bytes()).collect();
    let ns: Vec<u8> = s
        .normal
        .iter()
        .flat_map(|n| n.map(|c| f32::from(c) / 127.0))
        .flat_map(f32::to_le_bytes)
        .collect();
    let hits: Vec<u8> = s
        .kind
        .iter()
        .flat_map(|&k| [VTABLE_OF_KIND[usize::from(k)], RAY_SHAPE_VTABLE, 0])
        .flat_map(u32::to_le_bytes)
        .collect();
    let cols = crate::navsample::Columns {
        start: &starts,
        y: &ys,
        normal: &ns,
        flags: &s.flags,
        hit: Some(&hits),
    };
    assert_eq!(crate::navsample::write_v3(&s.header, &cols).unwrap(), data);
    let (h, counts) = level::header_and_counts(&data).unwrap();
    assert_eq!((h.cap_bottom as f32, counts), (0.6f32, s.counts.clone()));
}

#[test]
fn the_grid_and_its_mask() {
    let g = Grid::from_box(0.8, [-770.0, -910.0, 135.0, -555.0], [290.0, 680.0], None);
    assert!(
        (g.min_x + 770.4).abs() < 1e-3 && (g.min_z + 910.4).abs() < 1e-3 && g.nx == 1133 && g.nz == 446,
        "{g:?}"
    );
    let g = Grid::from_box(
        0.2,
        [-1.0, -1.0, 201.0, 101.0],
        [0.0, 10.0],
        Some(&[[-1.0, -1.0, 101.0, 101.0]]),
    );
    let m = g.mask.as_ref().unwrap();
    for j in 0..g.nz {
        assert!(m[j * g.nx..j * g.nx + 10].iter().all(|&b| b));
        assert!(!m[(j + 1) * g.nx - 10..(j + 1) * g.nx].iter().any(|&b| b));
    }
    assert_eq!(quantise(1.0), 127);
    assert_eq!(quantise(-0.5), -64);
    assert_eq!(capsule_axis(0.28, 0.4, 1.9), (0.95, 1.5));
    assert_eq!(capsule_axis(f64::from(0.6f32), 0.4, 1.9), (1.15, 1.5));
    assert_eq!(capsule_axis(0.3, 0.4, 1.9), (0.7, 1.5));
}

#[test]
fn destroyed_state_parts() {
    let dir = std::env::temp_dir().join(format!("bc2_sampler_parts_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bp = dir.join("House.xml");
    let hav = dir.join("House_Havok.xml");
    let comp = |g: &str, refs: [&str; 2]| {
        format!(
            "<instance guid=\"{g}\" type=\"GameSharedResources.PartComponentData\">\n<array name=\"HealthStates\">\n\
             <item ref=\"{}\" />\n<item ref=\"{}\" />\n</array>\n</instance>\n",
            refs[0], refs[1]
        )
    };
    let state = |g: &str, mesh: &str, hp: &str| {
        let m = if mesh.is_empty() {
            "<field name=\"MeshName\" />".to_owned()
        } else {
            format!("<field name=\"MeshName\">{mesh}</field>")
        };
        format!(
            "<instance guid=\"{g}\" type=\"GameSharedResources.HealthStateData\">\n{m}\n<field name=\"Health\">{hp}</field>\n</instance>\n"
        )
    };
    let xml = format!(
        "<partition>\n{}{}{}{}{}{}</partition>\n",
        comp("A", ["b", "c"]),
        state("B", "ruin_downed", "0"),
        state("C", "ruin", "100000"),
        comp("D", ["e", "f"]),
        state("E", "", "0"),
        state("F", "Part_01", "100")
    );
    std::fs::write(&bp, xml).unwrap();
    let names = ["base", "Part_01", "ruin", "ruin_downed"];
    let parts = names.iter().fold(String::new(), |s, n| {
        s + "<complex>\r\n<field name=\"Name\">" + n + "</field>\r\n</complex>\r\n"
    });
    std::fs::write(
        &hav,
        format!(
            "<partition><instance type=\"Physics.HavokAsset\"><array name=\"PartInfoDatas\">\r\n{parts}</array></instance></partition>\r\n"
        ),
    )
    .unwrap();
    let got = level::destroyed_parts(&bp.to_string_lossy()).unwrap();
    assert_eq!(got.into_iter().collect::<Vec<_>>(), ["ruin_downed"]);
    let res = dir.join("House_Havok.res").to_string_lossy().into_owned();
    assert_eq!(level::part_names(&res), ["base", "part_01", "ruin", "ruin_downed"]);
    assert!(
        level::destroyed_parts(&dir.join("missing.xml").to_string_lossy())
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
