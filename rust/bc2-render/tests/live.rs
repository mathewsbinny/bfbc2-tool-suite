//! The live draw list (round R5c) on MP_005: the same frame as the per-frame draw list of R5b, bit for bit, with
//! culling on and off; an edit replayed against the level loaded again with it; and picking checked by the
//! selection mask.
//!
//! Skipped (with a note) without OPFOR's prepared MP_005 or Direct3D 11.
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

use bc2_render::d3d::Device;
use bc2_render::frame::{Camera, Rendered};
use bc2_render::live::{Live, Options};
use bc2_render::world::World;

fn same(a: &Rendered, b: &Rendered) -> usize {
    (0..a.depth.len())
        .filter(|&i| {
            a.hdr[4 * i..4 * i + 4]
                .iter()
                .zip(&b.hdr[4 * i..4 * i + 4])
                .all(|(x, y)| x.to_bits() == y.to_bits())
                && a.depth[i].to_bits() == b.depth[i].to_bits()
                && a.classes[i] == b.classes[i]
        })
        .count()
}

fn live(
    dev: &mut Device,
    world: &mut World,
    live: &mut Live,
    cam: &Camera,
    target: [f64; 3],
    size: [u32; 2],
    cull: bool,
) -> Rendered {
    live.set(world.sources(target));
    live.build(cam, size, Options { cull });
    let (depth, post) = world.passes(cam, size, true).unwrap();
    let raw = world.raw().clone();
    dev.render_live(live, size, world.clear(), depth.as_ref(), &post, &world.textures, &raw)
        .unwrap();
    dev.read_scene(post.last()).unwrap()
}

#[test]
fn mp_005_live_list_is_the_frame_edits_and_picks() {
    let _gate = crate::gate();
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
    let target = [159.7, 66.0, 353.9];
    let (yaw, pitch, dist) = (206f64.to_radians(), 18f64.to_radians(), 75.0);
    let eye = [
        target[0] + dist * pitch.cos() * yaw.sin(),
        target[1] + dist * pitch.sin(),
        target[2] + dist * pitch.cos() * yaw.cos(),
    ];
    let size = [640u32, 360u32];
    let cam = Camera::look(eye, target, 60f64.to_radians(), 0.5, 4000.0, 640.0 / 360.0);
    // the old path
    let frame = world.frame(&cam, target, size, true).unwrap();
    let raw = world.raw().clone();
    let old = dev.render(&frame, &world.textures, &raw).unwrap();
    let n = old.depth.len();
    let mut l = Live::new(&world.light);
    let full = live(&mut dev, &mut world, &mut l, &cam, target, size, false);
    let culled = live(&mut dev, &mut world, &mut l, &cam, target, size, true);
    println!(
        "{} draws listed, {} drawn after culling",
        l.draw_count(),
        l.built.steps.len()
    );
    assert_eq!(same(&old, &full), n);
    assert_eq!(same(&old, &culled), n);
    assert_eq!(old.ldr, culled.ldr);
    assert!(l.built.steps.len() < l.draw_count(), "culling leaves draws out");
    // picking at the view's centre: the warehouse, confirmed by its mask under the pixel
    let at = [size[0] / 2, size[1] / 2];
    let (depth, post) = world.passes(&cam, size, true).unwrap();
    let id = dev.pick(&l, size, at, depth.as_ref(), &world.textures, &raw).unwrap();
    assert!(id > 0, "something placed is at the centre");
    assert!(
        placed[id as usize - 1].blueprint.contains("warehouse"),
        "{}",
        placed[id as usize - 1].blueprint
    );
    dev.render_live(&l, size, world.clear(), depth.as_ref(), &post, &world.textures, &raw)
        .unwrap();
    dev.outline(
        &l,
        &[id],
        post.last().unwrap(),
        [1.0; 4],
        [1.0; 4],
        &world.textures,
        &raw,
    )
    .unwrap();
    let r = dev.read_scene(None).unwrap();
    assert!(r.classes[(at[1] * size[0] + at[0]) as usize] & 0x80 != 0);
    // an edit: the warehouse moved; replayed, then against the level loaded again with it
    let mut edited = placed.clone();
    edited[id as usize - 1].rows[12] += 6.0;
    world.set_placed(&edited);
    let replayed = live(&mut dev, &mut world, &mut l, &cam, target, size, true);
    let mut fresh = World::load(&opfor, "mp_005", &name, &edited, weather.as_deref()).unwrap();
    let mut l2 = Live::new(&fresh.light);
    let scratch = live(&mut dev, &mut fresh, &mut l2, &cam, target, size, true);
    assert_eq!(same(&replayed, &scratch), n);
    assert_ne!(same(&replayed, &culled), n, "the edit shows");
}
