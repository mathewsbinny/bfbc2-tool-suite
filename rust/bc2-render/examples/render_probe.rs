//! A level's game-accurate frame drawn with some of its sources left out (round R5d's diagnosis tool): per view,
//! the picture (PNG), the share of near-black pixels and, per draw class, the pixels it kept.
//!
//! `cargo run --release -p bc2-render --example render_probe -- <level> <out folder> --view x,y,z,yaw,pitch,dist
//! [--view ...] [--size WxH] [--drop water,strips,patches,objects,<mesh word>] [--name stem]`
#![allow(
    clippy::print_stdout,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::chunks_exact_to_as_chunks,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    missing_docs
)]

mod common;

use std::path::PathBuf;
use std::time::Instant;

use bc2_render::d3d::Device;
use bc2_render::frame::Camera;
use bc2_render::live::{Key, Live, Options};
use bc2_render::world::World;

fn camera(v: &[f64], size: [u32; 2]) -> (Camera, [f64; 3]) {
    let (x, y, z) = (v[0], v[1], v[2]);
    let (yaw, pitch, dist) = (v[3].to_radians(), v[4].to_radians(), v[5]);
    let eye = [
        x + dist * pitch.cos() * yaw.sin(),
        y + dist * pitch.sin(),
        z + dist * pitch.cos() * yaw.cos(),
    ];
    let far = v.get(6).copied().unwrap_or(4000.0);
    (
        Camera::look(
            eye,
            [x, y, z],
            60f64.to_radians(),
            0.5,
            far,
            f64::from(size[0]) / f64::from(size[1]),
        ),
        [x, y, z],
    )
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opfor = common::opfor();
    let (lv, out) = (a[1].clone(), PathBuf::from(&a[2]));
    let opt = |k: &str| a.iter().position(|s| s == k).and_then(|i| a.get(i + 1)).cloned();
    let size: Vec<u32> = opt("--size")
        .unwrap_or_else(|| "1280x720".into())
        .split('x')
        .map(|v| v.parse().unwrap())
        .collect();
    let size = [size[0], size[1]];
    let drop: Vec<String> = opt("--drop").map_or_else(Vec::new, |d| d.split(',').map(str::to_lowercase).collect());
    let stem = opt("--name").unwrap_or_else(|| lv.clone());
    let specs: Vec<String> = a
        .iter()
        .enumerate()
        .filter(|(_, s)| *s == "--view")
        .map(|(i, _)| a[i + 1].clone())
        .collect();
    std::fs::create_dir_all(&out).ok();
    let t = Instant::now();
    let (name, placed, weather) = common::placed(&opfor, &lv).unwrap();
    let package = !opfor.join("work").join("levels").join(lv.to_lowercase()).is_dir();
    let mut world = if package {
        World::load_package(&opfor, &lv, &name, &placed, weather.as_deref()).unwrap()
    } else {
        World::load(&opfor, &lv, &name, &placed, weather.as_deref()).unwrap()
    };
    println!("{lv}: loaded in {:.2} s", t.elapsed().as_secs_f64());
    world.sky_pass = a.iter().any(|s| s == "--sky");
    for n in world.notes.iter().filter(|n| n.starts_with("sky")) {
        println!("  note: {n}");
    }
    // `auto`: live_view's opening view (the placed object nearest the objects' median, from the south-west)
    let views: Vec<Vec<f64>> = specs
        .iter()
        .map(|v| {
            if v != "auto" {
                return v.split(',').map(|x| x.parse().unwrap()).collect();
            }
            let mut xs: Vec<[f64; 3]> = world
                .objects
                .iter()
                .map(|o| [o.world[3], o.world[7], o.world[11]])
                .collect();
            xs.sort_by(|a, b| a[0].total_cmp(&b[0]));
            let mx = xs[xs.len() / 2][0];
            xs.sort_by(|a, b| a[2].total_cmp(&b[2]));
            let mz = xs[xs.len() / 2][2];
            let c = xs
                .iter()
                .min_by(|a, b| {
                    let d = |p: &[f64; 3]| (p[0] - mx).powi(2) + (p[2] - mz).powi(2);
                    d(a).total_cmp(&d(b))
                })
                .copied()
                .unwrap();
            println!("auto view: {:.1},{:.1},{:.1},225,20,90", c[0], c[1], c[2]);
            vec![c[0], c[1], c[2], 225.0, 20.0, 90.0]
        })
        .collect();
    let mut dev = Device::new(false).unwrap();
    let mut live = Live::new(&world.light);
    for (vi, v) in views.iter().enumerate() {
        let (cam, target) = camera(v, size);
        let sources: Vec<_> = world
            .sources(target)
            .into_iter()
            .filter(|s| {
                let kind = match &s.key {
                    Key::Water => "water",
                    Key::Strip(_) => "strips",
                    Key::Patch(..) => "patches",
                    Key::Object(_) => "objects",
                    Key::Sky => "sky",
                };
                let mesh = s.prepared.mesh.to_lowercase();
                !drop
                    .iter()
                    .any(|d| d == kind || (d.len() > 3 && mesh.contains(d.as_str())))
            })
            .collect();
        live.set(sources);
        live.build(
            &cam,
            size,
            Options {
                cull: true,
                lod: a.iter().any(|s| s == "--lod"),
            },
        );
        if a.iter().any(|s| s == "--hash") {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            live.built.arena.hash(&mut h);
            for st in &live.built.steps {
                (st.inst, st.draw, st.vs, st.ps).hash(&mut h);
                let inst = &live.instances[st.inst as usize];
                let d = &inst.source.prepared.draws[usize::from(st.draw)];
                d.textures.hash(&mut h);
                d.samplers.hash(&mut h);
                d.program.hash(&mut h);
                d.first_index.hash(&mut h);
                d.index_count.hash(&mut h);
                inst.source.prepared.vertices.hash(&mut h);
                inst.source.prepared.indices.hash(&mut h);
            }
            live.built.depth_at.hash(&mut h);
            let mut tex: Vec<(&String, usize)> = world
                .textures
                .iter()
                .map(|(k, v)| {
                    (
                        k,
                        v.as_ref().map_or(0, |b| {
                            let mut hh = std::collections::hash_map::DefaultHasher::new();
                            b.hash(&mut hh);
                            hh.finish() as usize
                        }),
                    )
                })
                .collect();
            tex.sort();
            tex.hash(&mut h);
            println!("  cpu hash {:016x}", h.finish());
        }
        let (depth, post) = world.passes(&cam, size, true).unwrap();
        let raw = world.raw().clone();
        // `--frames N`: the frame drawn N more times, the camera turning 0.05 degrees a frame (live_view's), the
        // median of the build, the submit and the GPU's time (waited for); the draws made
        let frames: usize = a
            .iter()
            .position(|s| s == "--frames")
            .and_then(|i| a.get(i + 1))
            .map_or(0, |v| v.parse().unwrap());
        if frames > 0 {
            let lod = a.iter().any(|s| s == "--lod");
            let (mut tb, mut ts, mut tg) = (Vec::new(), Vec::new(), Vec::new());
            let mut draws = 0;
            for f in 0..frames + 2 {
                let mut w = v.clone();
                w[3] += 0.05 * f as f64;
                let (c, _) = camera(&w, size);
                let t0 = Instant::now();
                live.build(&c, size, Options { cull: true, lod });
                let t1 = Instant::now();
                let st = dev
                    .render_live(&live, size, world.clear(), depth.as_ref(), &post, &world.textures, &raw)
                    .unwrap();
                let t2 = Instant::now();
                dev.finish().unwrap();
                let t3 = Instant::now();
                draws = st.draws;
                if f >= 2 {
                    tb.push(t1.duration_since(t0).as_secs_f64() * 1e3);
                    ts.push(t2.duration_since(t1).as_secs_f64() * 1e3);
                    tg.push(t3.duration_since(t2).as_secs_f64() * 1e3);
                }
            }
            let med = |v: &mut Vec<f64>| {
                v.sort_by(f64::total_cmp);
                v[v.len() / 2]
            };
            println!(
                "  frames ({}): {draws} draws, {} listed; build {:.2} ms, submit {:.2} ms, GPU {:.2} ms (medians of {frames})",
                if lod { "LOD" } else { "full" },
                live.draw_count(),
                med(&mut tb),
                med(&mut ts),
                med(&mut tg)
            );
            let mut td = Vec::new();
            for _ in 0..5 {
                let t0 = Instant::now();
                let _ = dev.read_depth().unwrap();
                td.push(t0.elapsed().as_secs_f64() * 1e3);
            }
            println!(
                "  depth read back (the labels' test): {:.2} ms (median of 5)",
                med(&mut td)
            );
            live.build(&cam, size, Options { cull: true, lod });
        }
        let t = Instant::now();
        dev.render_live(&live, size, world.clear(), depth.as_ref(), &post, &world.textures, &raw)
            .unwrap();
        let r = dev.read_scene(post.last()).unwrap();
        let n = r.depth.len();
        let black = (0..n)
            .filter(|&i| r.ldr[4 * i] < 8 && r.ldr[4 * i + 1] < 8 && r.ldr[4 * i + 2] < 8)
            .count();
        let mut classes = [0usize; 8];
        for &c in &r.classes {
            classes[usize::from(c).min(7)] += 1;
        }
        println!(
            "view {vi}: {:.1} ms, near-black {black} of {n} ({:.1} %), classes {classes:?}",
            t.elapsed().as_secs_f64() * 1e3,
            100.0 * black as f64 / n as f64
        );
        let mut sums = [[0f64; 5]; 8];
        for i in 0..n {
            let c = usize::from(r.classes[i]).min(7);
            for k in 0..4 {
                sums[c][k] += f64::from(r.hdr[4 * i + k]);
            }
            sums[c][4] += 1.0;
        }
        for (c, s) in sums.iter().enumerate().filter(|(_, s)| s[4] > 0.0) {
            println!(
                "  class {c}: {} px, mean HDR rgba ({:.3}, {:.3}, {:.3}, {:.3})",
                s[4],
                s[0] / s[4],
                s[1] / s[4],
                s[2] / s[4],
                s[3] / s[4]
            );
        }
        println!("  ldr bytes {} for {n} pixels", r.ldr.len());
        for p in a
            .iter()
            .enumerate()
            .filter(|(_, s)| *s == "--pixel")
            .map(|(i, _)| &a[i + 1])
        {
            let xy: Vec<usize> = p.split(',').map(|v| v.parse().unwrap()).collect();
            let i = xy[1] * size[0] as usize + xy[0];
            println!(
                "  pixel {p}: hdr {:?} depth {} class {} ldr {:?}",
                &r.hdr[4 * i..4 * i + 4],
                r.depth[i],
                r.classes[i],
                &r.ldr[4 * i..4 * i + 4]
            );
        }
        if a.iter().any(|s| s == "--dump") {
            let bytes: Vec<u8> = r.hdr.iter().flat_map(|v| v.to_le_bytes()).collect();
            std::fs::write(out.join(format!("{stem}_v{vi}.hdr")), bytes).unwrap();
        }
        // the tonemap's alpha is not coverage: saved opaque, as the editor shows the frame
        let mut ldr = r.ldr.clone();
        for p in ldr.chunks_exact_mut(4) {
            p[3] = 255;
        }
        common::save_png(&out.join(format!("{stem}_v{vi}.png")), size[0], size[1], &ldr).unwrap();
    }
}
