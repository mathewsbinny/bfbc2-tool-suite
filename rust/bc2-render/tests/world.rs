//! MP_005's whole world (round R5b): the placed objects and group members with their shader-set overrides, the
//! strips, the water and DICE's post-process, drawn at the harbour; then the same frame drawn by the C++ harness
//! (`shaderview`) from a job and held against the Rust device's picture pixel for pixel.
//!
//! Skipped (with a note) without OPFOR's prepared MP_005, Direct3D 11 or (the parity part) `shaderview.exe`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    missing_docs
)]

#[path = "../examples/common/mod.rs"]
mod common;

use std::collections::HashMap;

use bc2_render::d3d::Device;
use bc2_render::frame::Camera;
use bc2_render::world::World;
use bc2_render::{job, water};

#[test]
fn mp_005_world_draws_and_matches_the_harness() {
    let opfor = common::opfor();
    if !opfor.join("work").join("levels").join("mp_005").join("dist").is_dir() {
        println!("skipped: MP_005 not prepared");
        return;
    }
    let mut dev = match Device::new(false) {
        Ok(d) => d,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let (name, placed, weather) = common::placed(&opfor, "mp_005").unwrap();
    let mut world = World::load(&opfor, "mp_005", &name, &placed, weather.as_deref()).unwrap();
    let c = world.counts;
    println!("{c:?}");
    // the harbour's water and wakes, the strips, the group members and the overrides the partition names
    assert_eq!(c.water, 2);
    assert!(c.strips > 0);
    assert!(c.members > 2000, "{c:?}");
    assert!(c.overridden > 200, "{c:?}");
    assert!(world.post_shaders.is_some(), "{:?}", world.notes);
    // the harbour: the warehouses and the water beyond them
    let target = [159.7, 66.0, 353.9];
    let (yaw, pitch, dist) = (206f64.to_radians(), 18f64.to_radians(), 75.0);
    let eye = [
        target[0] + dist * pitch.cos() * yaw.sin(),
        target[1] + dist * pitch.sin(),
        target[2] + dist * pitch.cos() * yaw.cos(),
    ];
    let (w, h) = (640u32, 360u32);
    let cam = Camera::look(
        eye,
        target,
        60f64.to_radians(),
        0.5,
        4000.0,
        f64::from(w) / f64::from(h),
    );
    let frame = world.frame(&cam, target, [w, h], true).unwrap();
    let raw = world.raw().clone();
    let r = dev.render(&frame, &world.textures, &raw).unwrap();
    assert_eq!(r.ldr.len(), (w * h * 4) as usize);
    let covered = r.classes.iter().filter(|&&k| k > 0).count();
    assert!(covered > (w * h / 2) as usize, "{covered}");
    let depth = dev
        .read_pass(water::DEPTH)
        .unwrap()
        .expect("the water read the scene's depth");
    let Some(exe) = common::shaderview() else {
        println!("parity skipped: shaderview.exe not found (SHADERVIEW)");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bc2render_world_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fwd = |p: &std::path::Path| p.to_string_lossy().replace('\\', "/");
    let (out, pack, txt) = (dir.join("sv.raw"), dir.join("job.pack"), dir.join("job.txt"));
    let mut extra = HashMap::new();
    extra.insert(water::DEPTH.to_owned(), depth);
    let j = job::write(&frame, &world.textures, &raw, &extra, &fwd(&out), &fwd(&pack));
    assert!(j.skipped.is_empty(), "{:?}", j.skipped);
    std::fs::write(&pack, &j.pack).unwrap();
    std::fs::write(&txt, &j.text).unwrap();
    let o = std::process::Command::new(exe).arg(&txt).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let theirs: Vec<u32> = std::fs::read(&out)
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let stencil = std::fs::read(dir.join("sv.raw.stencil")).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let mut same = 0;
    for k in 0..(w * h) as usize {
        let ours = &r.hdr[4 * k..4 * k + 4];
        if ours
            .iter()
            .zip(&theirs[4 * k..4 * k + 4])
            .all(|(a, b)| a.to_bits() == *b)
            && stencil[k] == r.classes[k]
        {
            same += 1;
        }
    }
    println!(
        "{same} of {} pixels identical to shaderview's ({} draws)",
        w * h,
        j.draws
    );
    assert_eq!(same, (w * h) as usize);
}
