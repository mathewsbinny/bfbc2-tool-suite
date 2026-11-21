//! MP_005's `IND_Warehouse` with its material-mask mesh drawn as `renderqa.py render --together` draws it (the
//! three-quarter view at 512 x 512, its light and tonemap). On 2026-11-18 this picture was identical, pixel for pixel,
//! to the C++ harness's (`shaderview.exe`) in all three of its views (`examples/harness_views.rs`); the test keeps
//! what that picture measured: its draws, coverage and mean colour.
//!
//! Skipped (with a note) without Project OPFOR BFBC2's prepared MP_005 or a Direct3D 11 device.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::indexing_slicing,
    missing_docs
)]

use std::path::PathBuf;
use std::sync::Arc;

use bc2_render::d3d::Device;
use bc2_render::files::{Files, level_folders};
use bc2_render::frame::{Camera, Frame, IDENTITY, Instance, draw_calls};
use bc2_render::material::{Db, TextureFiles, prepare};
use bc2_render::{tonemap, weather};

fn opfor() -> Option<PathBuf> {
    let env = std::env::var_os("OPFOR_ROOT").map(PathBuf::from);
    let up = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|d| d.join("project-opfor-bfbc2"))
        .collect::<Vec<_>>();
    env.into_iter()
        .chain(up)
        .find(|d| d.join("work").join("levels").join("mp_005").join("pkg").is_dir())
}

#[test]
fn warehouse_as_the_harness_draws_it() {
    let _gate = crate::gate();
    let Some(root) = opfor() else {
        println!("skipped: MP_005 is not prepared in Project OPFOR BFBC2");
        return;
    };
    let mut dev = match Device::new(false) {
        Ok(d) => d,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let files = Files::new(&level_folders(&root, "mp_005"));
    let db = Db::parse(&files.read("levels/mp_005/shaders/dx11_single.res").unwrap()).unwrap();
    let w = weather::read(&files, "weather/r_mp005").unwrap();
    assert_eq!(w.state, "weather/r_mp005/states/default");
    let (values, post) = weather::light(&w, weather::NOON_DIR);
    let mut cache = TextureFiles::new();
    let mut instances = Vec::new();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for m in [
        "Objects/Buildings/IND_Warehouse/IND_Warehouse_Mesh",
        "Objects/Buildings/IND_Warehouse/MatMask/IND_Warehouse_MatMask_MatMaskMesh",
    ] {
        let p = prepare(&files, &db, m, "Textures/Sky/Sky_Beach_Generic_RGBA16f", &mut cache);
        assert!(p.notes.is_empty(), "{m}: {:?}", p.notes);
        let (l, h) = p.bounds.unwrap();
        for k in 0..3 {
            lo[k] = lo[k].min(l[k]);
            hi[k] = hi[k].max(h[k]);
        }
        instances.push(Instance {
            prepared: Arc::new(p),
            world: IDENTITY,
        });
    }
    let draws: Vec<usize> = instances.iter().map(|i| i.prepared.draws.len()).collect();
    assert_eq!(
        draws,
        [7, 1],
        "renderqa's parts: 7 of the building, the material-mask overlay"
    );
    let cam = Camera::orbit(lo, hi, 45.0, 28.0);
    let frame = Frame {
        width: 512,
        height: 512,
        clear: [0.0; 4],
        draws: draw_calls(&instances, &values, &cam, [512, 512]),
        ..Frame::default()
    };
    let r = dev.render(&frame, &cache, &std::collections::HashMap::new()).unwrap();
    let mask = r.mask();
    let (ldr, exposure) = tonemap::tonemap(&r.hdr, &mask, &post, [82, 92, 107]);
    let covered = mask.iter().filter(|&&m| m).count();
    let mut sum = [0.0f64; 3];
    for (px, _) in ldr.as_chunks::<4>().0.iter().zip(&mask).filter(|(_, m)| **m) {
        for (s, v) in sum.iter_mut().zip(px) {
            *s += f64::from(*v);
        }
    }
    let mean = sum.map(|s| s / covered as f64);
    println!("covered {covered}, exposure {exposure}, mean {mean:?}");
    assert!(
        (exposure - 1.0).abs() < 1e-6,
        "MP_005's default state: exposure fixed at 1"
    );
    // the harness's picture (identical pixel for pixel on 2026-11-18): pixels covered and their mean colour
    let want = [COVERED, MEAN[0], MEAN[1], MEAN[2]];
    let got = [covered as f64, mean[0], mean[1], mean[2]];
    assert!(
        (got[0] - want[0]).abs() <= want[0] * 0.002,
        "coverage {got:?} against {want:?}"
    );
    for k in 1..4 {
        assert!((got[k] - want[k]).abs() <= 1.0, "mean colour {got:?} against {want:?}");
    }
}

/// The harness's picture: pixels covered, and their mean colour (8-bit).
const COVERED: f64 = 73_711.0;
const MEAN: [f64; 3] = [63.759_873, 57.211_271, 48.768_447];
