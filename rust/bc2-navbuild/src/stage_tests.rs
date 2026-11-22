//! Round NK5's stage and build: Python's number formats, the settings, the `.npy` reader and a whole synthetic build
//! (a flat ground of two sectors with a step; the regression levels' byte-for-byte parity is
//! `rust/parity_nk5_build.py`'s).

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]

use std::collections::BTreeSet;

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
