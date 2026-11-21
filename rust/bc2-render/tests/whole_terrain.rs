//! The game-accurate view draws the whole terrain (round RV1): on SP_01 (Operation Aurora, the campaign level the
//! user saw it on) and MP_005, from the map editor's opening overview, from above and from low across the map, every
//! sampled pixel whose ray meets the terrain's height field is drawn by the terrain patches, at the height field's
//! height. Until RV1 only the 49 x 49 patches around the camera's target were drawn (784 m of a 2,048 m map): the
//! rest of the terrain was invisible.
//!
//! The reference is the height field itself: each sampled pixel's ray marched over it on the CPU. Only the terrain
//! patches are drawn (objects, strips and water would hide terrain the ray meets). Each level has its own device (a
//! device keeps the engine textures it was given by name, the height map among them).
//!
//! Skipped (with a note) without OPFOR's prepared levels or Direct3D 11.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    missing_docs
)]

use crate::common;

use bc2_formats::terrain::Heightfield;
use bc2_render::d3d::Device;
use bc2_render::frame::Camera;
use bc2_render::live::{Key, Live, Options};
use bc2_render::world::World;

const SIZE: [u32; 2] = [640, 360];
/// Every `STEP`th pixel's ray is marched.
const STEP: u32 = 10;

fn add(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Where the ray from `eye` along `dir` first meets the height field (inside its square, before `far`).
fn meets(hf: &Heightfield, eye: [f64; 3], dir: [f64; 3], near: f64, far: f64) -> Option<[f64; 3]> {
    let (cx, cz) = hf.cell();
    let lo = [f64::from(hf.min[0]), f64::from(hf.min[1]), f64::from(hf.min[2])];
    let hi = [
        lo[0] + cx * f64::from(hf.size - 1),
        f64::from(hf.max[1]),
        lo[2] + cz * f64::from(hf.size - 1),
    ];
    // the ray's span inside the height field's box
    let (mut t0, mut t1) = (near, far);
    for k in 0..3 {
        if dir[k].abs() < 1e-12 {
            if eye[k] < lo[k] || eye[k] > hi[k] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[k] - eye[k]) / dir[k], (hi[k] - eye[k]) / dir[k]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    let mut t = t0;
    while t <= t1 {
        let p = add(eye, dir, t);
        if p[1] <= hf.height(p[0], p[2]) {
            return Some(p);
        }
        t += 0.5;
    }
    None
}

/// Of the sampled pixels whose ray meets the terrain: how many, how many the terrain patches drew, and how many of
/// those lie within 2 m of the height field's surface (the frame's depth taken back to a point).
fn coverage(dev: &mut Device, world: &mut World, cam: &Camera, target: [f64; 3]) -> (usize, usize, usize) {
    let mut live = Live::new(&world.light);
    let patches: Vec<_> = world
        .sources(target)
        .into_iter()
        .filter(|s| matches!(s.key, Key::Patch(..)))
        .collect();
    live.set(patches);
    live.build(cam, SIZE, Options { cull: true });
    let (depth, post) = world.passes(cam, SIZE, false).unwrap();
    let raw = world.raw().clone();
    dev.render_live(&live, SIZE, world.clear(), depth.as_ref(), &post, &world.textures, &raw)
        .unwrap();
    let frame = dev.read_scene(None).unwrap();
    let hf = &world.terrain.as_ref().unwrap().hf;
    let half = (cam.fov / 2.0).tan();
    let (mut hits, mut drawn, mut on) = (0, 0, 0);
    for y in (STEP / 2..SIZE[1]).step_by(STEP as usize) {
        for x in (STEP / 2..SIZE[0]).step_by(STEP as usize) {
            let sx = (2.0 * (f64::from(x) + 0.5) / f64::from(SIZE[0]) - 1.0) * half * cam.aspect;
            let sy = (1.0 - 2.0 * (f64::from(y) + 0.5) / f64::from(SIZE[1])) * half;
            let d = add(add(cam.forward, cam.right, sx), cam.up, sy);
            let n = dist(d, [0.0; 3]);
            let dir = d.map(|v| v / n);
            // the far plane is a depth along forward: the ray's length at it is n times that
            if meets(hf, cam.eye, dir, cam.near, cam.far * n).is_some() {
                hits += 1;
                let dz = f64::from(frame.depth[(y * SIZE[0] + x) as usize]);
                if dz < 1.0 {
                    drawn += 1;
                    let view_z = cam.near * cam.far / (cam.far - dz * (cam.far - cam.near));
                    let p = add(cam.eye, dir, view_z * n);
                    if (p[1] - hf.height(p[0], p[2])).abs() < 2.0 {
                        on += 1;
                    }
                }
            }
        }
    }
    (hits, drawn, on)
}

fn aspect() -> f64 {
    f64::from(SIZE[0]) / f64::from(SIZE[1])
}

/// Looking at `target` from `dist` away, `yaw` degrees round from +z and `pitch` degrees up.
fn orbit(target: [f64; 3], yaw: f64, pitch: f64, dist: f64) -> Camera {
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    let eye = [
        target[0] + dist * pitch.cos() * yaw.sin(),
        target[1] + dist * pitch.sin(),
        target[2] + dist * pitch.cos() * yaw.cos(),
    ];
    Camera::look(eye, target, 60f64.to_radians(), 0.5, 4000.0, aspect())
}

#[test]
fn the_whole_terrain_is_drawn() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    let mut checked = 0;
    // the map editor's opening targets (about 1,700 m back, 34 degrees up), from above, and low across the map
    for (lv, target, back) in [
        ("sp_01", [-3.6, 205.5, -15.0], 1672.0),
        ("mp_005", [-24.0, 100.1, 23.1], 1706.0),
    ] {
        if !opfor.join("work").join("levels").join(lv).join("dist").is_dir() {
            println!("{lv}: skipped, not prepared");
            continue;
        }
        let mut dev = match Device::new(false) {
            Ok(d) => d,
            Err(e) => {
                println!("skipped: {e}");
                return;
            }
        };
        let (name, placed, weather) = common::placed(&opfor, lv).unwrap();
        let mut world = World::load(&opfor, lv, &name, &placed, weather.as_deref()).unwrap();
        assert!(world.terrain.is_some(), "{lv}: {:?}", world.notes);
        let low = [target[0], 60.0, target[2]];
        let above = Camera::look([0.0, 3000.0, 1.0], [0.0; 3], 60f64.to_radians(), 0.5, 4000.0, aspect());
        for (view, cam, at) in [
            ("overview", orbit(target, 34.4, 34.4, back), target),
            ("above", above, [0.0; 3]),
            ("across", orbit(low, 200.0, 12.0, 700.0), low),
        ] {
            let (hits, drawn, on) = coverage(&mut dev, &mut world, &cam, at);
            println!(
                "{lv} {view}: of {hits} sampled pixels on the terrain, {drawn} drawn ({:.1}%), {on} of them on the \
                 height field",
                drawn as f64 * 100.0 / hits.max(1) as f64
            );
            assert!(
                hits > 300,
                "{lv} {view}: the view sees the terrain ({hits} sampled pixels)"
            );
            assert!(
                drawn * 100 >= hits * 99,
                "{lv} {view}: the terrain is drawn wherever the view meets it: {drawn} of {hits} sampled pixels"
            );
            assert!(
                on * 100 >= drawn * 98,
                "{lv} {view}: the terrain is drawn at the height field's height: {on} of {drawn} sampled pixels"
            );
        }
        checked += 1;
    }
    if checked == 0 {
        println!("skipped: neither SP_01 nor MP_005 is prepared");
    }
}
