//! The live draw list (round R5c) proven and timed against the per-frame draw list of R5b, on a prepared level:
//!
//! - per view: the old path ([`World::frame`] + [`Device::render`]) and the live path ([`Live::build`] +
//!   [`Device::render_live`]) with culling on and off, their HDR pictures, depths, draw classes and post-processed
//!   pictures compared bit for bit; `--job`: the same frame drawn by the C++ harness (`shaderview`) against the live
//!   picture;
//! - `--edit`: objects moved, rotated, removed, duplicated and added, replayed on the live list, against the world
//!   loaded again from the edited placements (bit for bit);
//! - `--pick`: picking at the projected centres of objects, each answer checked by the selection mask (the picked
//!   object lies at the frame's depth under the pixel), and the outline saved;
//! - timings per stage over `--frames N` frames (the camera turning a little each frame), old and live.
//!
//! `cargo run --release -p bc2-render --example live_view -- <level> <out folder> --view x,y,z,yaw,pitch,dist
//! [--view ...] [--size WxH] [--frames N] [--job] [--edit] [--pick] [--name stem]`
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::many_single_char_names,
    missing_docs
)]

mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bc2_render::d3d::Device;
use bc2_render::frame::{Camera, Rendered};
use bc2_render::live::{Live, Options};
use bc2_render::world::{Placed, World};
use bc2_render::{job, water};

fn camera(v: &[f64], size: [u32; 2]) -> (Camera, [f64; 3]) {
    let (x, y, z) = (v[0], v[1], v[2]);
    let (yaw, pitch, dist) = (v[3].to_radians(), v[4].to_radians(), v[5]);
    let eye = [
        x + dist * pitch.cos() * yaw.sin(),
        y + dist * pitch.sin(),
        z + dist * pitch.cos() * yaw.cos(),
    ];
    (
        Camera::look(eye, [x, y, z], 60f64.to_radians(), 0.5, 4000.0, f64::from(size[0]) / f64::from(size[1])),
        [x, y, z],
    )
}

/// Pixels whose colour bits, depth bits and class are all equal; and the post-processed bytes equal.
fn same(a: &Rendered, b: &Rendered) -> (usize, bool) {
    let n = a.depth.len();
    let mut k = 0;
    for i in 0..n {
        if a.hdr[4 * i..4 * i + 4]
            .iter()
            .zip(&b.hdr[4 * i..4 * i + 4])
            .all(|(x, y)| x.to_bits() == y.to_bits())
            && a.depth[i].to_bits() == b.depth[i].to_bits()
            && a.classes[i] == b.classes[i]
        {
            k += 1;
        }
    }
    (k, a.ldr == b.ldr)
}

struct Lv {
    world: World,
    live: Live,
}

fn live_frame(dev: &mut Device, lv: &mut Lv, cam: &Camera, target: [f64; 3], size: [u32; 2], cull: bool) -> Rendered {
    lv.live.set(lv.world.sources(target));
    lv.live.build(cam, size, Options { cull });
    let (depth, post) = lv.world.passes(cam, size, true).unwrap();
    let raw = lv.world.raw().clone();
    dev.render_live(&lv.live, size, lv.world.clear(), depth.as_ref(), &post, &lv.world.textures, &raw)
        .unwrap();
    dev.read_scene(post.last()).unwrap()
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0) * 1e3
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opfor = common::opfor();
    let (lvname, out) = (a[1].clone(), PathBuf::from(&a[2]));
    let opt = |k: &str| a.iter().position(|s| s == k).and_then(|i| a.get(i + 1)).cloned();
    let has = |k: &str| a.iter().any(|s| s == k);
    let size: Vec<u32> = opt("--size")
        .unwrap_or_else(|| "1600x950".into())
        .split('x')
        .map(|v| v.parse().unwrap())
        .collect();
    let size = [size[0], size[1]];
    let frames: usize = opt("--frames").map_or(10, |v| v.parse().unwrap());
    let stem = opt("--name").unwrap_or_else(|| lvname.clone());
    let views: Vec<Vec<f64>> = a
        .iter()
        .enumerate()
        .filter(|(_, s)| *s == "--view")
        .map(|(i, _)| a[i + 1].split(',').map(|v| v.parse().unwrap()).collect())
        .collect();
    std::fs::create_dir_all(&out).ok();
    let t = Instant::now();
    let (name, placed, weather) = common::placed(&opfor, &lvname).unwrap();
    let world = World::load(&opfor, &lvname, &name, &placed, weather.as_deref()).unwrap();
    println!("{lvname}: loaded in {:.2} s; {:?}", t.elapsed().as_secs_f64(), world.counts);
    let mut lv = Lv {
        live: Live::new(&world.light),
        world,
    };
    let mut dev = Device::new(has("--warp")).unwrap();
    println!(
        "device: {}; constants by offset: {}",
        if dev.warp { "WARP" } else { "GPU" },
        dev.ring_constants()
    );
    for (vi, v) in views.iter().enumerate() {
        let (cam, target) = camera(v, size);
        // the old path
        let mut old_t: Vec<[f64; 4]> = Vec::new();
        let mut old = Rendered::default();
        let mut old_frame = None;
        for f in 0..frames.max(1) {
            let mut c = cam;
            if f > 0 {
                let (c2, _) = camera(&[v[0], v[1], v[2], v[3] + 0.05 * f as f64, v[4], v[5]], size);
                c = c2;
            }
            let t0 = Instant::now();
            let frame = lv.world.frame(&c, target, size, true).unwrap();
            let build = t0.elapsed().as_secs_f64();
            let raw = lv.world.raw().clone();
            let r = dev.render(&frame, &lv.world.textures, &raw).unwrap();
            old_t.push([build, r.submit, r.post, r.readback]);
            if f == 0 {
                old = r;
                old_frame = Some(frame);
            }
        }
        // the live path, culling off then on: the same picture as the old path
        let t0 = Instant::now();
        let full = live_frame(&mut dev, &mut lv, &cam, target, size, false);
        let first = t0.elapsed().as_secs_f64();
        let culled = live_frame(&mut dev, &mut lv, &cam, target, size, true);
        let (s1, l1) = same(&old, &full);
        let (s2, l2) = same(&old, &culled);
        let n = old.depth.len();
        println!(
            "view {vi}: {} draws in the list; live without culling {s1} of {n} pixels identical to the old path \
             (colour, depth, class; post-processed picture identical: {l1}); with culling {s2} of {n} ({l2}), {} draws \
             drawn, {} instances culled; first live frame (materials made) {:.0} ms",
            lv.live.draw_count(),
            lv.live.built.steps.len(),
            lv.live.built.culled_instances,
            first * 1e3
        );
        common::save_png(&out.join(format!("{stem}_view{vi}.png")), size[0], size[1], &culled.ldr).unwrap();
        // the live path's costs
        let mut lt: Vec<[f64; 7]> = Vec::new();
        for f in 0..frames.max(1) {
            let (c, _) = camera(&[v[0], v[1], v[2], v[3] + 0.05 * f as f64, v[4], v[5]], size);
            let t0 = Instant::now();
            lv.live.build(&c, size, Options::default());
            let build = t0.elapsed().as_secs_f64();
            let (depth, post) = lv.world.passes(&c, size, true).unwrap();
            let raw = lv.world.raw().clone();
            let st = dev
                .render_live(&lv.live, size, lv.world.clear(), depth.as_ref(), &post, &lv.world.textures, &raw)
                .unwrap();
            let t1 = Instant::now();
            dev.finish().unwrap();
            let gpu = t1.elapsed().as_secs_f64();
            let t2 = Instant::now();
            let shared = dev.share(post.last().unwrap()).is_ok();
            let share = t2.elapsed().as_secs_f64();
            let t3 = Instant::now();
            let _ = dev.read_output(post.last().unwrap()).unwrap();
            let copy = t3.elapsed().as_secs_f64();
            assert!(shared, "sharing failed");
            lt.push([build, st.upload, st.submit, st.post, gpu, share, copy]);
        }
        let col = |k: usize| median(&mut old_t.iter().map(|r| r[k]).collect::<Vec<_>>());
        println!(
            "  old (median of {frames}): build {:.1} ms, submit {:.1}, post {:.1}, GPU + read back {:.1}; total {:.1}",
            col(0),
            col(1),
            col(2),
            col(3),
            col(0) + col(1) + col(2) + col(3)
        );
        let col = |k: usize| median(&mut lt.iter().map(|r| r[k]).collect::<Vec<_>>());
        println!(
            "  live (median of {frames}): build {:.1} ms, upload {:.1}, submit {:.1}, post {:.1}, GPU wait {:.1}, \
             share (copy on the GPU + wait) {:.1}; total {:.1} (the copy fallback's read back of the picture: {:.1})",
            col(0),
            col(1),
            col(2),
            col(3),
            col(4),
            col(5),
            col(0) + col(1) + col(2) + col(3) + col(4) + col(5),
            col(6)
        );
        if has("--job") {
            // the view drawn again: the scene's linear depth the job gives the harness is this view's
            let culled = live_frame(&mut dev, &mut lv, &cam, target, size, true);
            parity(&mut dev, &mut lv, old_frame.as_ref().unwrap(), &culled, &out, &format!("{stem}_view{vi}"));
        }
        if has("--pick") {
            pick(&mut dev, &mut lv, &cam, target, size, &out, &format!("{stem}_view{vi}"));
        }
    }
    if has("--edit")
        && let Some(v) = views.first()
    {
        edit(&mut dev, &mut lv, &opfor, &lvname, &name, &placed, weather.as_deref(), v, size);
    }
}

fn parity(dev: &mut Device, lv: &mut Lv, frame: &bc2_render::frame::Frame, live: &Rendered, out: &Path, stem: &str) {
    let Some(exe) = common::shaderview() else {
        println!("  --job: shaderview.exe not found");
        return;
    };
    let mut extra = HashMap::new();
    if let Some(l) = dev.read_pass(water::DEPTH).unwrap() {
        extra.insert(water::DEPTH.to_owned(), l);
    }
    let fwd = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let (raw_out, pack, jf) = (
        out.join(format!("{stem}_sv.raw")),
        out.join(format!("{stem}_job.pack")),
        out.join(format!("{stem}_job.txt")),
    );
    let j = job::write(frame, &lv.world.textures, lv.world.raw(), &extra, &fwd(&raw_out), &fwd(&pack));
    std::fs::write(&pack, &j.pack).unwrap();
    std::fs::write(&jf, &j.text).unwrap();
    let o = std::process::Command::new(exe).arg(&jf).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let theirs: Vec<u32> = std::fs::read(&raw_out)
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let stencil = std::fs::read(out.join(format!("{stem}_sv.raw.stencil"))).unwrap_or_default();
    let n = live.depth.len();
    let same = (0..n)
        .filter(|&k| {
            live.hdr[4 * k..4 * k + 4]
                .iter()
                .zip(&theirs[4 * k..4 * k + 4])
                .all(|(a, b)| a.to_bits() == *b)
                && stencil.get(k).is_none_or(|s| *s == live.classes[k])
        })
        .count();
    println!(
        "  parity with shaderview (the job of all {} draws; the live frame culled): {same} of {n} pixels identical",
        j.draws
    );
    for p in [raw_out, pack, jf, out.join(format!("{stem}_sv.raw.stencil"))] {
        std::fs::remove_file(p).ok();
    }
}

fn pick(dev: &mut Device, lv: &mut Lv, cam: &Camera, target: [f64; 3], size: [u32; 2], out: &Path, stem: &str) {
    // objects whose centre projects inside the view, a few spread through the list
    let vp = cam.view_proj();
    let mut tried = 0;
    let mut right = 0;
    let mut answers = Vec::new();
    let raw = lv.world.raw().clone();
    let (depth, post) = lv.world.passes(cam, size, true).unwrap();
    let objs: Vec<(usize, [f64; 3])> = lv
        .world
        .objects
        .iter()
        .enumerate()
        .map(|(k, o)| (k, [o.world[3], o.world[7], o.world[11]]))
        .collect();
    for (k, p) in objs.iter().step_by(37) {
        let c: Vec<f64> = (0..4)
            .map(|r| vp[r * 4] * p[0] + vp[r * 4 + 1] * p[1] + vp[r * 4 + 2] * p[2] + vp[r * 4 + 3])
            .collect();
        if c[3] <= 0.0 {
            continue;
        }
        let (x, y) = ((c[0] / c[3] + 1.0) / 2.0, (1.0 - c[1] / c[3]) / 2.0);
        if !(0.0..1.0).contains(&x) || !(0.0..1.0).contains(&y) {
            continue;
        }
        let at = [(x * f64::from(size[0])) as u32, (y * f64::from(size[1])) as u32];
        lv.live.set(lv.world.sources(target));
        lv.live.build(cam, size, Options::default());
        let id = dev.pick(&lv.live, size, at, depth.as_ref(), &lv.world.textures, &raw).unwrap();
        tried += 1;
        // the answer checked: the picked object's draws lie at the frame's depth under the pixel
        dev.render_live(&lv.live, size, lv.world.clear(), depth.as_ref(), &post, &lv.world.textures, &raw)
            .unwrap();
        let ok = if id == 0 {
            // nothing pickable: the pixel belongs to terrain, strips, water or the sky
            let r = dev.read_scene(None).unwrap();
            let i = at[1] as usize * size[0] as usize + at[0] as usize;
            r.classes[i] == 0 || {
                // drawn by a non-object: no object mask covers it
                let picks: Vec<u32> = lv.live.instances.iter().map(|i| i.source.pick).filter(|&p| p != 0).collect();
                dev.outline(&lv.live, &picks, post.last().unwrap(), [1.0; 4], [1.0; 4], &lv.world.textures, &raw)
                    .unwrap();
                dev.read_scene(None).unwrap().classes[i] & 0x80 == 0
            }
        } else {
            dev.outline(&lv.live, &[id], post.last().unwrap(), [1.0, 0.45, 0.0, 1.0], [1.0, 0.45, 0.0, 0.45], &lv.world.textures, &raw)
                .unwrap();
            let r = dev.read_scene(None).unwrap();
            r.classes[at[1] as usize * size[0] as usize + at[0] as usize] & 0x80 != 0
        };
        if ok {
            right += 1;
        }
        answers.push((*k, id, ok));
    }
    println!(
        "  picking at {tried} objects' centres: {right} answers confirmed by the selection mask; (object index, pick, \
         confirmed) {:?}",
        answers.iter().take(12).collect::<Vec<_>>()
    );
    // the outline of the first picked object, saved
    if let Some((_, id, _)) = answers.iter().find(|a| a.1 != 0) {
        lv.live.build(cam, size, Options::default());
        dev.render_live(&lv.live, size, lv.world.clear(), depth.as_ref(), &post, &lv.world.textures, &raw)
            .unwrap();
        dev.outline(&lv.live, &[*id], post.last().unwrap(), [1.0, 0.45, 0.0, 1.0], [1.0, 0.45, 0.0, 0.45], &lv.world.textures, &raw)
            .unwrap();
        let ldr = dev.read_output(post.last().unwrap()).unwrap();
        common::save_png(&out.join(format!("{stem}_selected.png")), size[0], size[1], &ldr).unwrap();
    }
}

#[allow(clippy::too_many_arguments, reason = "the level and the view")]
fn edit(
    dev: &mut Device,
    lv: &mut Lv,
    opfor: &Path,
    lvname: &str,
    name: &str,
    placed: &[Placed],
    weather: Option<&str>,
    v: &[f64],
    size: [u32; 2],
) {
    let (cam, target) = camera(v, size);
    // the objects nearest the target: moved, turned, removed, duplicated; and one added
    let mut near: Vec<(f64, usize)> = placed
        .iter()
        .enumerate()
        .filter(|(_, p)| p.blueprint.starts_with("objects/") && p.group.is_none())
        .map(|(k, p)| {
            let d = (p.rows[12] - target[0]).powi(2) + (p.rows[14] - target[2]).powi(2);
            (d, k)
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let ks: Vec<usize> = near.iter().take(4).map(|n| n.1).collect();
    let mut edited: Vec<Placed> = placed.to_vec();
    edited[ks[0]].rows[12] += 6.0; // moved
    edited[ks[0]].rows[13] += 1.5;
    let r = &mut edited[ks[1]].rows; // a quarter turn about y
    let (r0, r2) = ([r[0], r[1], r[2]], [r[8], r[9], r[10]]);
    r[0..3].copy_from_slice(&r2);
    r[8..11].copy_from_slice(&r0.map(|x| -x));
    let mut dup = edited[ks[3]].clone(); // duplicated beside itself
    dup.guid = "00000000-0000-0000-0000-00000000r5c1".into();
    dup.rows[14] += 8.0;
    let mut added = edited[ks[3]].clone(); // added again elsewhere (a new object of a blueprint already placed)
    added.guid = "00000000-0000-0000-0000-00000000r5c2".into();
    added.rows[12] -= 12.0;
    edited.remove(ks[2]); // removed
    edited.push(dup);
    edited.push(added);
    // replayed on the live list
    let t = Instant::now();
    lv.world.set_placed(&edited);
    let sources = lv.world.sources(target);
    let planned = lv.live.set(sources);
    let replay = t.elapsed().as_secs_f64();
    lv.live.build(&cam, size, Options::default());
    let (depth, post) = lv.world.passes(&cam, size, true).unwrap();
    let raw = lv.world.raw().clone();
    dev.render_live(&lv.live, size, lv.world.clear(), depth.as_ref(), &post, &lv.world.textures, &raw)
        .unwrap();
    let replayed = dev.read_scene(post.last()).unwrap();
    // from scratch: the world loaded with the edited placements, a new live list, a new device
    let fresh_world = World::load(opfor, lvname, name, &edited, weather).unwrap();
    let mut fresh = Lv {
        live: Live::new(&fresh_world.light),
        world: fresh_world,
    };
    let mut dev2 = Device::new(dev.warp).unwrap();
    let scratch = live_frame(&mut dev2, &mut fresh, &cam, target, size, true);
    let (s, l) = same(&replayed, &scratch);
    let unedited = live_frame(dev, &mut Lv {
        live: Live::new(&lv.world.light),
        world: World::load(opfor, lvname, name, placed, weather).unwrap(),
    }, &cam, target, size, true);
    let (s0, _) = same(&unedited, &scratch);
    println!(
        "edit (moved, turned, removed, duplicated, added near the first view's target): replayed in {:.1} ms ({planned} \
         instances planned again); the replayed frame against the level loaded again with the edits: {s} of {} pixels \
         identical (post-processed picture identical: {l}); the unedited frame differs in {} pixels",
        replay * 1e3,
        scratch.depth.len(),
        scratch.depth.len() - s0
    );
    let out = std::env::args().nth(2).map(PathBuf::from).unwrap();
    common::save_png(&out.join(format!("{lvname}_edited.png")), size[0], size[1], &replayed.ldr).unwrap();
    common::save_png(&out.join(format!("{lvname}_unedited.png")), size[0], size[1], &unedited.ldr).unwrap();
}
