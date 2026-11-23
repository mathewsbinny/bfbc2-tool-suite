//! Round NK5's stage and build: Python's number formats, the settings, the `.npy` reader and a whole synthetic build
//! (a flat ground of two sectors with a step; the regression levels' byte-for-byte parity is
//! `rust/parity_nk5_build.py`'s).

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::*;
use crate::build::{self, grid_ground};
use crate::walk::WalkSettings;

#[test]
fn python_number_formats() {
    assert_eq!(py_repr(1.5), "1.5");
    assert_eq!(py_repr(2.0), "2.0");
    assert_eq!(py_repr(0.2), "0.2");
    assert_eq!(py_repr(1e-5), "1e-05");
    assert_eq!(py_repr(1e16), "1e+16");
    assert_eq!(py_repr(1_795_336_519.055_013_2), "1795336519.0550132");
    assert_eq!(py_g(0.8), "0.8");
    assert_eq!(py_g(0.2), "0.2");
    assert_eq!(py_g(1.0), "1");
    assert_eq!(py_g(8.0), "8");
    assert_eq!(py_g(1_234_567.0), "1.23457e+06");
    assert_eq!(py_g(0.000_012_5), "1.25e-05");
}

#[test]
fn walk_settings_constants_are_pythons() {
    // math.cos(math.radians(55.0)), cos(radians(15)), cos(radians(40)) - 0.01, tan(radians(40)) as CPython gives them
    let s = WalkSettings::navkit(40.0);
    assert_eq!(s.steep_max_cos.to_bits(), 0x3fe2_5abc_f87c_4978);
    assert_eq!(s.lip_flat_cos.unwrap().to_bits(), 0x3fee_e8dd_4748_bf15);
    assert_eq!(s.slope_cos.to_bits(), 0x3fe8_3184_1db0_97e7);
    assert_eq!(s.slope_tan.to_bits(), 0x3fea_d9e7_783f_bf1d);
}

#[test]
fn detailed_settings() {
    let d = BuildSettings::default().detailed();
    assert_eq!((d.sector_samples, d.window, d.tile, d.tiles), (500, (-1, 502), 100, 6));
    assert_eq!((d.resolution, d.link_step), (0.05, DETAILED_LINK_STEP));
    assert!(d.is_detailed() && !BuildSettings::default().is_detailed());
    assert_eq!(sector_id(9, 3), 0x0903);
}

#[test]
fn npy_triangles_as_levelobjects_writes_them() {
    let dir = std::env::temp_dir().join(format!("nk5_npy_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, descr: &str, n: usize, body: &[u8]| {
        let mut h = format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': ({n}, 3, 3), }}");
        let pad = 64 - (10 + h.len() + 1) % 64;
        h.push_str(&" ".repeat(pad));
        h.push('\n');
        let mut out = b"\x93NUMPY\x01\x00".to_vec();
        out.extend_from_slice(&u16::try_from(h.len()).unwrap().to_le_bytes());
        out.extend_from_slice(h.as_bytes());
        out.extend_from_slice(body);
        let p = dir.join(name);
        std::fs::write(&p, out).unwrap();
        p
    };
    let vals: Vec<u8> = (0..9).flat_map(|v| (v as f32 * 0.5).to_le_bytes()).collect();
    let t = build::read_tris_npy(&write("a.npy", "<f4", 1, &vals)).unwrap().unwrap();
    assert!(t.from_f32);
    assert_eq!(t.tris, vec![[[0.0, 0.5, 1.0], [1.5, 2.0, 2.5], [3.0, 3.5, 4.0]]]);
    assert!(build::read_tris_npy(&write("b.npy", "<f8", 0, &[])).unwrap().is_none());
    assert!(build::read_tris_npy(&write("c.npy", "<i4", 1, &[0; 36])).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A flat ground over x in 0 .. 200 m, z in 0 .. 100 m at pitch 0.8 (one surface per column, every one kept), a 1 m step along
/// x = 50 m: two sectors side by side.
fn flat(cfg: &BuildSettings) -> Ground {
    let (nx, nz) = (250usize, 125usize);
    let n = nx * nz;
    let start: Vec<usize> = (0..=n).collect();
    let y: Vec<f64> = (0..n)
        .map(|c| if (c % nx) as f64 * 0.8 >= 50.0 { 11.0 } else { 10.0 })
        .collect();
    grid_ground(
        GridInfo {
            nx,
            nz,
            min_x: 0.0,
            min_z: 0.0,
            pitch: 0.8,
        },
        start,
        y,
        vec![true; n],
        crate::Floats::F64(vec![0.0; n]),
        crate::Floats::F64(vec![0.0; n]),
        vec![false; n],
        vec![true; n],
        0.28,
        cfg,
    )
    .unwrap()
}

struct Lines(Vec<String>);

impl Log for Lines {
    fn line(&mut self, text: &str) {
        self.0.push(text.to_owned());
    }
    fn seconds(&self) -> f64 {
        0.0
    }
}

#[test]
fn a_whole_stage_on_a_synthetic_ground() {
    let cfg = BuildSettings::default();
    let sw = Switches::default();
    let mut stats = BuildStats::default();
    let mut log = Lines(Vec::new());
    let args = StageArgs {
        id_start: 1,
        ..StageArgs::default()
    };
    let st = stage(flat(&cfg), &args, &cfg, &sw, &mut stats, &mut log).unwrap();
    assert_eq!(st.sector_set(), BTreeSet::from([(0, 0), (1, 0)]));
    assert_eq!(log.0[0], "31250 surfaces, 31250 walkable kept (0 s)");
    assert_eq!(log.0[1], "2 sectors (pitch 0.8)");
    // the 1 m step at 50 m is no link: the areas there end at it (outline), no portal across it
    assert!(st.sectors[0].cells.len() > st.sectors[1].cells.len());
    // border nodes on the seam, shared by both sectors with the same ids
    let a: BTreeSet<u32> = st.border_nodes[&(0, 0)].iter().map(|b| b.0).collect();
    let b: BTreeSet<u32> = st.border_nodes[&(1, 0)].iter().map(|b| b.0).collect();
    assert!(!a.is_empty() && a == b && a.iter().all(|id| id & 0x8000_0000 != 0));
    // every sector links to the other across the seam, and the file reads back
    for g in &st.graphs {
        assert_eq!(g.mesh.links.len(), 1);
        assert!(!g.nodes.is_empty() && !g.pairs.is_empty());
    }
    let mut report = build::Report::default();
    let data = build::file_bytes(
        &st.graphs,
        None,
        &cfg,
        &sw,
        (261_022, 12_000_000),
        &mut report,
        &mut log,
    )
    .unwrap();
    let f = bc2_formats::hpd::parse(&data).unwrap();
    assert_eq!(f.sectors.len(), 2);
    assert_eq!(report.sectors, 2);
    assert!(report.nodes > 0 && report.edges > 0);
}

#[test]
fn no_ground_is_the_references_refusal() {
    let cfg = BuildSettings::default();
    let mut g = flat(&cfg);
    g.level = g.level.with_keep(vec![false; g.level.len()]).unwrap();
    let mut log = Lines(Vec::new());
    let e = stage(
        g,
        &StageArgs::default(),
        &cfg,
        &Switches::default(),
        &mut BuildStats::default(),
        &mut log,
    )
    .unwrap_err();
    assert!(e.to_string().starts_with("no walkable ground was kept"));
}

// ------------------------------------------------------------------------------------------------ round NK5-rest:
// the checks of navbuild.py's tests (test_column_sectors, test_area_link, test_portal_symmetry, test_bridge_union,
// the detail pass of test_detail_features, the settings of test_tool_edges), on the Rust stage's own pieces now that
// navbuild.py and its twins are retired

/// A level from columns of surface heights (`columns[j * nx + i]`), gradients as given (flat: zeros), every surface
/// kept; terrain everywhere unless `terrain` says.
fn level_of(
    columns: &[Vec<f64>],
    nx: usize,
    nz: usize,
    grads: Option<(Vec<f64>, Vec<f64>)>,
    terrain: Option<Vec<bool>>,
) -> Level {
    let mut start = vec![0];
    let mut y = Vec::new();
    for c in 0..nx * nz {
        y.extend(columns.get(c).cloned().unwrap_or_default());
        start.push(y.len());
    }
    let n = y.len();
    let (gx, gz) = grads.unwrap_or_else(|| (vec![0.0; n], vec![0.0; n]));
    Level::new(crate::LevelData {
        nx,
        nz,
        start,
        keep: vec![true; n],
        y,
        gx: crate::Floats::F64(gx),
        gz: crate::Floats::F64(gz),
        stair: vec![false; n],
        terrain: terrain.unwrap_or_else(|| vec![true; n]),
        pitch: 0.8,
        step: 0.28,
        link_step: 0.6,
    })
    .unwrap()
}

/// One surface per column, heights `h[j][i]`, gradients as `np.gradient` gives them (the normals of a plane).
fn heights(h: &[Vec<f64>], terrain: Option<Vec<bool>>) -> Level {
    let (nz, nx) = (h.len(), h[0].len());
    let grad = |a: &dyn Fn(usize) -> f64, n: usize, at: usize| -> f64 {
        if n < 2 {
            0.0
        } else if at == 0 {
            (a(1) - a(0)) / 0.8
        } else if at == n - 1 {
            (a(n - 1) - a(n - 2)) / 0.8
        } else {
            (a(at + 1) - a(at - 1)) / 1.6
        }
    };
    let (mut gx, mut gz) = (Vec::new(), Vec::new());
    for j in 0..nz {
        for i in 0..nx {
            gx.push(grad(&|q| h[j][q], nx, i));
            gz.push(grad(&|q| h[q][i], nz, j));
        }
    }
    let cols: Vec<Vec<f64>> = h.iter().flat_map(|r| r.iter().map(|&v| vec![v])).collect();
    level_of(&cols, nx, nz, Some((gx, gz)), terrain)
}

#[test]
fn column_sectors_as_dice_places_them() {
    // the grid's minimum as navsample.cpp's Begin() aligns it (float32, a multiple of the pitch)
    let grid = |lo: f64, pitch: f64| -> GridInfo {
        let p = pitch as f32;
        let mn = wide(((lo as f32) / p).floor() * p);
        let n = (((lo + 260.0) as f32 - mn as f32) / p).ceil() as usize + 1;
        GridInfo {
            nx: n,
            nz: n,
            min_x: mn,
            min_z: mn,
            pitch,
        }
    };
    fn wide(v: f32) -> f64 {
        f64::from(v)
    }
    for (pitch, minima) in [
        (0.8, (-300..=300).map(|v| f64::from(v) * 10.0).collect::<Vec<_>>()),
        (0.2, (-300..=300).map(|v| f64::from(v) * 10.0).collect()),
        (0.8, vec![-910.4, -812.3, -799.9, -0.4, 123.45, 1999.99]),
    ] {
        for lo in minima {
            let g = grid(lo, pitch);
            let off = (lo / 100.0).floor() as i64 - 1;
            for gi in 0..g.nx {
                let x = g.min_x + gi as f64 * pitch;
                assert_eq!(
                    column_sector(&g, gi, off, off, pitch).0,
                    (x / 100.0).floor() as i64 - off
                );
                assert_eq!(
                    column_sector(&g, gi * g.nx, off, off, pitch).1,
                    (x / 100.0).floor() as i64 - off
                );
            }
        }
    }
    // SP_008 (box minimum x -910.4, XOffset -11): the x = -900 column lies a hair below the line (the float32 minimum
    // -910.4000244), in sector X = 1 as DICE's sectors 257-262 hold it; a grid starting on a line starts its own
    let g = grid(-910.4, 0.8);
    assert!(g.min_x == wide(-910.4_f32) && g.min_x < -910.4);
    let xs: Vec<i64> = [12, 13, 14]
        .iter()
        .map(|&c| column_sector(&g, c, -11, -1, 0.8).0)
        .collect();
    assert_eq!(xs, [1, 1, 2]);
    let g = grid(-1000.0, 0.8);
    assert_eq!(g.min_x, -1000.0);
    let xs: Vec<i64> = [124, 125, 126]
        .iter()
        .map(|&c| column_sector(&g, c, -11, -1, 0.8).0)
        .collect();
    assert_eq!(xs, [1, 2, 2]);
}

#[test]
fn the_default_settings_are_the_normal_grid() {
    let d = BuildSettings::default();
    assert_eq!(
        (
            d.pitch,
            d.sector_samples,
            d.window,
            d.tile,
            d.tiles,
            d.resolution,
            d.link_step
        ),
        (0.8, 125, (-1, 127), 50, 3, 0.025, 0.6)
    );
    assert!(!d.is_detailed() && d.detail_features && d.order_nodes && d.clip_to_reference);
    assert_eq!(d.one_way_portals, OneWayRule::Drop);
    // the detailed variant changes the grid only, and back again
    assert_eq!(d.detailed().at_pitch(0.8), d);
    let sw = Switches::default();
    assert_eq!((sw.area_link, sw.link_rule), (Rule::Terrain, Rule::Slope));
    assert!(sw.symmetric_portals && sw.diagonal_links && sw.node_bounds && sw.owner_edges);
    assert_eq!(sw.gabriel, Gabriel::Detailed);
    assert_eq!(sw.spanner, Some(SPANNER_STRETCH));
}

#[test]
fn area_link_rule() {
    let lv = heights(&[vec![0.0, 0.55, 1.2]], None);
    assert!(lv.connected(0, 1, 1, 0, Rule::Raw), "straight step 0.55 m links");
    assert!(!lv.connected(1, 2, 1, 0, Rule::Raw), "straight step 0.65 m does not");
    let plane = heights(&[vec![0.0, 0.66, 1.32, 1.98]], None); // 39.5 deg: walkable, steps of 0.66 m
    assert!(
        plane.connected(0, 1, 1, 0, Rule::Slope),
        "the slope rule joins a steep plane"
    );
    assert!(!plane.connected(0, 1, 1, 0, Rule::Raw), "the raw rule cuts it");
    let members: Vec<(usize, i64, i64)> = (0..4).map(|k| (k, k as i64, 0)).collect();
    assert_eq!(
        plane.components(&members, 0, 0, Switches::default().area_link).len(),
        4,
        "one area per cell"
    );
    // diagonal (0,0) -> (1,1), step 0.9: through side (1,0) at 0.45 (both steps 0.45) links, even unwalkable
    let mut d = heights(&[vec![0.0, 0.45], vec![5.0, 0.9]], None);
    d = d.with_keep(vec![true, false, false, true]).unwrap();
    assert!(
        d.connected(0, 3, 1, 1, Rule::Raw),
        "diagonal through an unwalkable side cell"
    );
    let d2 = heights(&[vec![0.0, 5.0], vec![5.0, 0.9]], None);
    assert!(!d2.connected(0, 3, 1, 1, Rule::Raw), "no side route, no link");
    let d3 = heights(&[vec![0.0, 0.0], vec![0.0, 0.0]], None);
    assert!(d3.connected(0, 3, 1, 1, Rule::Raw), "flat diagonal links");
    let obj = heights(&[vec![0.0, 0.66, 1.32]], Some(vec![true, false, false]));
    assert!(
        obj.connected(1, 2, 1, 0, Rule::Terrain),
        "'terrain': two object surfaces use the slope rule"
    );
    assert!(
        obj.connected(0, 1, 1, 0, Rule::Terrain),
        "'terrain': terrain next to an object uses the slope rule"
    );
    let ter = heights(&[vec![0.0, 0.66, 1.32]], Some(vec![true, true, true]));
    assert!(
        !ter.connected(0, 1, 1, 0, Rule::Terrain),
        "'terrain': two terrain surfaces use the raw rule"
    );
}

fn tcfg() -> TraceCfg {
    TraceCfg {
        gi0: 0,
        gj0: 0,
        window: (-1, 127),
        y_offset: 0.95,
        rule: Rule::Slope,
    }
}

/// Per area: (target, unit edge midpoint x, z in centimetres) -> count, for every 0.8 m piece of its portals.
fn unit_portals(em: &[Emitted]) -> Vec<BTreeMap<(usize, i64, i64), usize>> {
    em.iter()
        .map(|e| {
            let mut c = BTreeMap::new();
            for &(p0, p1, t) in &e.portals {
                let n = (((p1.0 - p0.0).abs().max((p1.2 - p0.2).abs())) / 0.8).round().max(1.0) as i64;
                for q in 0..n {
                    let f = (q as f64 + 0.5) / n as f64;
                    let x = ((p0.0 + (p1.0 - p0.0) * f) * 100.0).round() as i64;
                    let z = ((p0.2 + (p1.2 - p0.2) * f) * 100.0).round() as i64;
                    *c.entry((t, x, z)).or_insert(0) += 1;
                }
            }
            c
        })
        .collect()
}

/// `trace_edges`, one-way candidates not in `keep` dropped, `trace_emit`: one sector's tracing.
fn traced(lv: &Level, cells: &[Vec<Cell>], keep: &BTreeSet<usize>) -> (Vec<Emitted>, usize) {
    let aof = trace::area_of(cells);
    let (mut rows, ow) = trace::trace_edges(lv, cells, &aof, tcfg(), true).unwrap();
    drop_one_way(&mut rows, &ow, keep).unwrap();
    (trace::trace_emit(lv, cells, rows, tcfg(), 0.0, 0.0, 0.8), ow.len())
}

fn sector_of(xy: (i64, i64), cells: Vec<Vec<Cell>>) -> SectorInfo {
    let n = cells.len();
    SectorInfo {
        xy,
        area_of: trace::area_of(&cells),
        cells,
        tiles: vec![[0, 0, 0]; n],
        origin: (0, 0, 0.0, 0.0),
        start: (-1, -1),
        ylims: vec![(0.0, 0.0); n],
        borders: vec![Vec::new(); n],
    }
}

#[test]
fn portal_symmetry_and_one_way_pieces() {
    // two areas side by side on flat ground: columns i 0..3, j 0..1; A = i 0..1, B = i 2..3
    let lv = level_of(&vec![vec![0.0]; 8], 4, 2, None, None);
    let k = |i: i64, j: i64| (j * 4 + i) as usize;
    let side = |is: [i64; 2]| -> Vec<Cell> { is.iter().flat_map(|&i| (0..2).map(move |j| (i, j, k(i, j)))).collect() };
    let (em, _) = traced(&lv, &[side([0, 1]), side([2, 3])], &BTreeSet::new());
    let up = unit_portals(&em);
    let to_b: BTreeSet<(i64, i64)> = up[0].keys().filter(|q| q.0 == 1).map(|q| (q.1, q.2)).collect();
    let to_a: BTreeSet<(i64, i64)> = up[1].keys().filter(|q| q.0 == 0).map(|q| (q.1, q.2)).collect();
    assert!(
        to_b.len() == 2 && to_b == to_a,
        "side by side: two portal pieces each way on the same edge"
    );
    assert!(
        up.iter().all(|c| c.values().all(|&v| v == 1)),
        "no edge carries two portal pieces"
    );

    // stacked: column 0 holds A's surface (y 0) and T's (y 0.2); column 1 holds T's (y 0.1)
    let lv2 = level_of(&[vec![0.0, 0.2], vec![0.1]], 2, 1, None, None);
    let areas2 = vec![vec![(0, 0, 0)], vec![(0, 0, 1), (1, 0, 2)]];
    let (em_keep, n) = traced(&lv2, &areas2, &BTreeSet::from([0]));
    let up_keep = unit_portals(&em_keep);
    assert_eq!(n, 1);
    assert!(
        up_keep[0].keys().any(|q| q.0 == 1) && !up_keep[1].keys().any(|q| q.0 == 0),
        "'keep': one-way"
    );
    let (em_drop, _) = traced(&lv2, &areas2, &BTreeSet::new());
    let up_drop = unit_portals(&em_drop);
    assert!(
        up_drop[0].is_empty() && up_drop[1].is_empty(),
        "'drop': no portal either way"
    );
    assert!(
        em_drop[0]
            .outline
            .iter()
            .any(|(p0, p1)| (p0.0 - 0.4).abs() < 1e-6 && (p1.0 - 0.4).abs() < 1e-6),
        "'drop': A's side at x 0.4 is outline instead"
    );

    // 'restore': a one-way candidate stays only when its two areas are joined no other way
    let restored =
        |lv: &Level, cells: Vec<Vec<Cell>>, others: Vec<SectorInfo>| -> (usize, BTreeSet<usize>, Vec<Emitted>) {
            let aof = trace::area_of(&cells);
            let (mut rows, ow) = trace::trace_edges(lv, &cells, &aof, tcfg(), true).unwrap();
            let mut infos = vec![sector_of((0, 0), cells.clone())];
            let mut edges = vec![(rows.clone(), ow.clone())];
            for o in others {
                edges.push((vec![EdgeRows::default(); o.cells.len()], Vec::new()));
                infos.push(o);
            }
            let keep = restore_one_way(&infos, &edges).swap_remove(0);
            drop_one_way(&mut rows, &ow, &keep).unwrap();
            (
                ow.len(),
                keep,
                trace::trace_emit(lv, &cells, rows, tcfg(), 0.0, 0.0, 0.8),
            )
        };
    let (n, kept, em) = restored(&lv2, areas2.clone(), Vec::new());
    assert!(
        n == 1 && kept == BTreeSet::from([0]) && unit_portals(&em)[0].keys().any(|q| q.0 == 1),
        "restore keeps"
    );
    // the same stacked column, but A and T also meet on flat ground next door: columns (0,1) A, (1,1) T at y 0
    let lv3 = level_of(&[vec![0.0, 0.2], vec![0.1], vec![0.0], vec![0.0]], 2, 2, None, None);
    let areas3 = vec![vec![(0, 0, 0), (0, 1, 3)], vec![(0, 0, 1), (1, 0, 2), (1, 1, 4)]];
    let (n3, kept3, em3) = restored(&lv3, areas3, Vec::new());
    let up3 = unit_portals(&em3);
    let a_b: BTreeSet<(i64, i64)> = up3[0].keys().filter(|q| q.0 == 1).map(|q| (q.1, q.2)).collect();
    let b_a: BTreeSet<(i64, i64)> = up3[1].keys().filter(|q| q.0 == 0).map(|q| (q.1, q.2)).collect();
    let two_way: BTreeSet<_> = a_b.intersection(&b_a).copied().collect();
    assert!(
        n3 >= 1 && kept3.is_empty() && !two_way.is_empty(),
        "restore: joined by a two-way portal drops"
    );
    assert!(
        a_b.iter().chain(&b_a).all(|p| two_way.contains(p)),
        "restore: no one-way piece left"
    );
    // areas joined through a shared surface across sectors (the same surface in another sector's area)
    let other = sector_of((1, 0), vec![vec![(0, 0, 0), (0, 0, 1)]]);
    let (_, kept_sh, _) = restored(&lv2, areas2, vec![other]);
    assert!(
        kept_sh.is_empty(),
        "restore: a shared surface across sectors drops the one-way piece"
    );
    assert_eq!(BuildSettings::default().one_way_portals, OneWayRule::Drop);
    assert!(Switches::default().symmetric_portals);
}

/// A small random generator for the patterns (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[(self.next() % v.len() as u64) as usize]
    }
}

/// `trace_emit`'s outline of one area holding `cells` on a 10 x 10 grid (one surface in each of their columns, none
/// elsewhere), in local sample units: [((x, z, y), (x, z, y))].
fn bridge_outline(
    cells: &BTreeSet<(i64, i64)>,
    hts: &BTreeMap<(i64, i64), f64>,
) -> Vec<((f64, f64, f64), (f64, f64, f64))> {
    const N: i64 = 10;
    let mut cols = vec![Vec::new(); (N * N) as usize];
    let mut area = Vec::new();
    let mut k = 0;
    for j in 0..N {
        for i in 0..N {
            if cells.contains(&(i, j)) {
                cols[(j * N + i) as usize].push(hts.get(&(i, j)).copied().unwrap_or(0.0));
                area.push((i, j, k));
                k += 1;
            }
        }
    }
    let lv = level_of(&cols, N as usize, N as usize, None, None);
    let (em, _) = traced(&lv, &[area], &BTreeSet::new());
    em[0]
        .outline
        .iter()
        .map(|&(a, b)| ((a.0 / 0.8, a.2 / 0.8, a.1), (b.0 / 0.8, b.2 / 0.8, b.1)))
        .collect()
}

#[test]
fn bridge_outlines_are_closed() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let (mut bad, mut with_bridges, mut overlapping) = (0, 0, 0);
    for _ in 0..600 {
        let n = rng.pick(&[4, 6, 8]);
        let mut cells = BTreeSet::new();
        for i in 0..n {
            for j in 0..n {
                let p = rng.pick(&[0.35, 0.5, 0.65]);
                if rng.unit() < p {
                    cells.insert((i, j));
                }
            }
        }
        let mut corners: BTreeMap<(i64, i64), usize> = BTreeMap::new();
        for &(i, j) in &cells {
            for (di, dj) in [(1, 1), (1, -1)] {
                if cells.contains(&(i + di, j + dj)) && !cells.contains(&(i + di, j)) && !cells.contains(&(i, j + dj)) {
                    *corners.entry((i + di, j)).or_insert(0) += 1;
                    *corners.entry((i, j + dj)).or_insert(0) += 1;
                }
            }
        }
        if corners.is_empty() {
            continue;
        }
        with_bridges += 1;
        overlapping += usize::from(corners.values().any(|&v| v > 1));
        let mut deg: BTreeMap<(i64, i64), usize> = BTreeMap::new();
        for (p, q) in bridge_outline(&cells, &BTreeMap::new()) {
            *deg.entry(((p.0 * 2.0).round() as i64, (p.1 * 2.0).round() as i64))
                .or_insert(0) += 1;
            *deg.entry(((q.0 * 2.0).round() as i64, (q.1 * 2.0).round() as i64))
                .or_insert(0) += 1;
        }
        bad += usize::from(deg.values().any(|v| v % 2 == 1));
    }
    assert!(
        with_bridges > 300 && overlapping > 50,
        "{with_bridges} with bridges, {overlapping} overlapping"
    );
    assert_eq!(bad, 0, "every outline closed");

    // one bridge alone: its two diagonals from A's corners (A's height) to B's (B's height); the four sides at its
    // corner covered, each cell keeping its two outer sides
    let cells = BTreeSet::from([(0, 0), (1, 1)]);
    let lines = bridge_outline(&cells, &BTreeMap::from([((0, 0), 1.0), ((1, 1), 2.0)]));
    let r = |v: f64| (v * 1e6).round() as i64;
    let y = 0.95;
    let mut diag: Vec<_> = lines
        .iter()
        .filter(|(p, q)| p.0 != q.0 && p.1 != q.1)
        .map(|(p, q)| {
            let (a, b) = ((r(p.0), r(p.1), r(p.2)), (r(q.0), r(q.1), r(q.2)));
            if a <= b { (a, b) } else { (b, a) }
        })
        .collect();
    diag.sort_unstable();
    assert_eq!(
        diag,
        vec![
            ((r(-0.5), r(0.5), r(1.0 + y)), (r(0.5), r(1.5), r(2.0 + y))),
            ((r(0.5), r(-0.5), r(1.0 + y)), (r(1.5), r(0.5), r(2.0 + y))),
        ]
    );
    let mut sides: Vec<_> = lines
        .iter()
        .filter(|(p, q)| p.0 == q.0 || p.1 == q.1)
        .map(|(p, q)| {
            let (a, b) = ((r(p.0), r(p.1)), (r(q.0), r(q.1)));
            if a <= b { (a, b) } else { (b, a) }
        })
        .collect();
    sides.sort_unstable();
    let mut want: Vec<_> = [
        ((-0.5, -0.5), (-0.5, 0.5)),
        ((-0.5, -0.5), (0.5, -0.5)),
        ((1.5, 0.5), (1.5, 1.5)),
        ((0.5, 1.5), (1.5, 1.5)),
    ]
    .iter()
    .map(|&((a, b), (c, d))| ((r(a), r(b)), (r(c), r(d))))
    .collect();
    want.sort_unstable();
    assert_eq!(sides, want);
}

fn outline_key(x_axis: bool, line2: i64, dir: i64) -> EdgeKey {
    EdgeKey {
        kind: Kind::Outline,
        x_axis,
        line2,
        dir,
    }
}

#[test]
fn the_detail_pass_moves_outline_sides_in_pairs() {
    // cells (0,0) (1,0) (0,1) at 0, (1,1) at 0.7: surface k = j * 2 + i
    let lv = level_of(&[vec![0.0], vec![0.0], vec![0.0], vec![0.7]], 2, 2, None, None);
    let cells = vec![vec![(0, 0, 0), (1, 0, 1), (0, 1, 2), (1, 1, 3)]];
    let rows = || {
        let mut e = EdgeRows::default();
        e.insert(outline_key(true, 1, 1), vec![(1, 0.95)]); // (0,1)|(1,1): outline on both cells
        e.insert(outline_key(true, 1, -1), vec![(1, 1.65)]);
        e.insert(outline_key(false, 1, 1), vec![(1, 0.95)]); // (1,0)|(1,1): on one cell only
        e.insert(outline_key(true, -1, -1), vec![(0, 0.95), (1, 0.95)]); // the area's own west edge
        vec![e]
    };
    let mut r = rows();
    let cfg = BuildSettings::default();
    let det = detail_pass(&lv, &cells, &mut r, (0, 0, 0.0, 0.0), &cfg, &Switches::default());
    let e = &r[0];
    assert_eq!(e.get(&outline_key(true, 1, 1)), Some(&vec![]));
    assert_eq!(e.get(&outline_key(true, 1, -1)), Some(&vec![]));
    assert_eq!(
        e.get(&outline_key(false, 1, 1)),
        Some(&vec![(1, 0.95)]),
        "a one-sided side kept"
    );
    assert_eq!(
        e.get(&outline_key(true, -1, -1)),
        Some(&vec![(0, 0.95), (1, 0.95)]),
        "the rest untouched"
    );
    assert_eq!(det.features[0].0.len(), 2, "two L-shaped D lines");
    assert_eq!((det.one_sided, det.moved), (1, 2));
    // the detailed settings (pitch 0.2): no D / F there (detail_in_detailed off), the rows untouched
    let mut r = rows();
    let det = detail_pass(
        &lv,
        &cells,
        &mut r,
        (0, 0, 0.0, 0.0),
        &cfg.detailed(),
        &Switches::default(),
    );
    assert!(det.features[0].0.is_empty() && det.features[0].1.is_empty());
    assert_eq!(r[0].get(&outline_key(true, 1, 1)), Some(&vec![(1, 0.95)]));
}

#[test]
fn route_lines_do_not_step_across_a_d_side() {
    // a flat 3 x 3 grid, surface k = j * 3 + i
    let lv = level_of(&vec![vec![0.0]; 9], 3, 3, None, None);
    let walk = |blocked: &[(usize, usize)], b: (i64, i64), k1: usize| {
        let set: HashSet<(usize, usize)> = blocked.iter().copied().collect();
        lv.line_walkable(0, (0, 0), b, 0, 0, k1, &set, Rule::Slope)
    };
    assert!(walk(&[], (2, 1), 5));
    assert!(
        !walk(&[(1, 4), (4, 1)], (2, 1), 5),
        "a D side on the line: (1, 0) | (1, 1)"
    );
    assert!(
        walk(&[(2, 5), (5, 2)], (2, 1), 5),
        "a D side beside the line does not block"
    );
    assert!(!walk(&[(0, 1), (1, 0)], (2, 0), 2), "a straight step across a D side");
}
