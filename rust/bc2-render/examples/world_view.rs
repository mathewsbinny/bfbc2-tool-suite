//! Draws a prepared level as the editor's game-accurate view does (round R5b): its placed objects and model-group
//! members with their shader-set overrides, the terrain patches around a point, the strips, the water and DICE's
//! post-process - and proves it:
//!
//! - `--job`: the same frame written as a job for the C++ harness (`shaderview`), drawn by it, and its HDR picture held
//!   against the Rust device's, pixel for pixel (the scene's linear depth the water reads given to both);
//! - always: DICE's `psUnprojectDepthBuffer` against the projection's own inverse on every pixel, the exposure pass
//!   against the CPU's, and DICE's `psFilmicTonemap` (bloom off) against the harness's CPU tonemap.
//!
//! `cargo run -p bc2-render --example world_view -- <level> <out folder> <x> <y> <z> <yaw> <pitch> <distance>
//! [--size WxH] [--job] [--warp] [--name <file stem>]` (`--warp`: both drawn by the software rasterizer) (`OPFOR` names Project OPFOR's folder; `SHADERVIEW` the harness).
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    missing_docs
)]

mod common;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use bc2_render::d3d::Device;
use bc2_render::frame::Camera;
use bc2_render::world::World;
use bc2_render::{job, post, tonemap, water};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opfor = common::opfor();
    let (lv, out) = (a[1].clone(), PathBuf::from(&a[2]));
    let f = |i: usize| a[i].parse::<f64>().unwrap();
    let (x, y, z, yaw, pitch, dist) = (f(3), f(4), f(5), f(6), f(7), f(8));
    let opt = |k: &str| a.iter().position(|s| s == k).and_then(|i| a.get(i + 1)).cloned();
    let size: Vec<u32> = opt("--size")
        .unwrap_or_else(|| "1280x720".into())
        .split('x')
        .map(|v| v.parse().unwrap())
        .collect();
    let (wd, ht) = (size[0], size[1]);
    let stem = opt("--name").unwrap_or_else(|| lv.clone());
    std::fs::create_dir_all(&out).ok();
    let t = Instant::now();
    let (name, placed, weather) = common::placed(&opfor, &lv).unwrap();
    let scene_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let mut world = World::load(&opfor, &lv, &name, &placed, weather.as_deref()).unwrap();
    println!(
        "{lv}: scene {scene_s:.2} s, world {:.2} s",
        t.elapsed().as_secs_f64()
    );
    for n in &world.notes {
        println!("  note: {n}");
    }
    // --mesh <text>: how the prepared meshes whose name has it are drawn
    if let Some(m) = opt("--mesh") {
        let mut seen = std::collections::HashSet::new();
        for i in &world.objects {
            let p = &i.prepared;
            if p.mesh.to_lowercase().contains(&m.to_lowercase()) && seen.insert(p.mesh.clone()) {
                println!("{} ({}): bounds {:?}", p.mesh, p.shader_set, p.bounds);
                for d in &p.draws {
                    println!(
                        "  {} program {} kind {} class {} indices {} bones {:?}",
                        d.part,
                        d.program,
                        d.kind,
                        d.class,
                        d.index_count,
                        d.bones.as_ref().map(|b| b.iter().take(3).collect::<Vec<_>>())
                    );
                }
            }
        }
    }
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    let eye = [
        x + dist * pitch.cos() * yaw.sin(),
        y + dist * pitch.sin(),
        z + dist * pitch.cos() * yaw.cos(),
    ];
    let cam = Camera::look(
        eye,
        [x, y, z],
        60f64.to_radians(),
        0.5,
        4000.0,
        f64::from(wd) / f64::from(ht),
    );
    let warp = a.iter().any(|s| s == "--warp");
    let mut dev = Device::new(warp).unwrap();
    println!("device: {}", if dev.warp { "WARP" } else { "GPU" });
    let mut last = None;
    for _ in 0..3 {
        let t = Instant::now();
        let mut frame = world.frame(&cam, [x, y, z], [wd, ht], true).unwrap();
        // --stride N / --not-stride N: only (or all but) the draws of that vertex size (20 patches, 28 strips, 36 water)
        if let Some(s) = opt("--stride").map(|v| v.parse::<u32>().unwrap()) {
            frame.draws.retain(|d| d.stride == s);
        }
        if let Some(s) = opt("--not-stride").map(|v| v.parse::<u32>().unwrap()) {
            frame.draws.retain(|d| d.stride != s);
        }
        let build = t.elapsed().as_secs_f64();
        let raw = world.raw().clone();
        let r = dev.render(&frame, &world.textures, &raw).unwrap();
        println!(
            "{} draws: build {:.1} ms, submit {:.1} ms, post {:.1} ms, GPU + read back {:.1} ms",
            r.draws,
            build * 1e3,
            r.submit * 1e3,
            r.post * 1e3,
            r.readback * 1e3
        );
        last = Some((frame, r));
    }
    let (frame, r) = last.unwrap();
    common::save_png(&out.join(format!("{stem}.png")), wd, ht, &r.ldr).unwrap();
    let bg = tonemap::map(world.background, 1.0, &world.post).map(|v| (v * 255.0).round() as u8);
    let (cpu, e_cpu) = tonemap::tonemap(&r.hdr, &r.mask(), &world.post, bg);
    common::save_png(&out.join(format!("{stem}_harness_tonemap.png")), wd, ht, &cpu).unwrap();

    // psUnprojectDepthBuffer against the projection's inverse
    let lin = dev.read_pass(water::DEPTH).unwrap();
    if let Some((_, _, px)) = &lin {
        let q = cam.far / (cam.far - cam.near);
        let mut worst = 0f64;
        // where an opaque or alpha-tested draw kept the pixel: the depth then is the one the pass read (the water and
        // other blended draws come later, and the water writes depth)
        for (k, d) in r.depth.iter().enumerate() {
            if *d >= 1.0 || !(1..=2).contains(&r.classes[k]) {
                continue;
            }
            // the shader's own steps in single precision: z / q, 1 - that, n / that
            let want = f64::from(cam.near as f32 / (1.0 - *d / q as f32));
            let got = f64::from(px[4 * k]);
            worst = worst.max((got - want).abs() / want);
        }
        println!("psUnprojectDepthBuffer: worst relative difference from n / (1 - z / q) in singles {worst:.2e}");
    } else {
        println!("psUnprojectDepthBuffer: not drawn (no draw reads DepthBuffer)");
    }
    // the exposure against the CPU's (every pixel; the GPU's is a tree of 2 x 2 means)
    if let Some((_, _, e)) = dev.read_pass("post:exposure").unwrap() {
        let all = vec![true; r.depth.len()];
        let e_all = tonemap::exposure(&r.hdr, &all, &world.post);
        println!(
            "psExposure: {:.6}; the CPU's over every pixel {e_all:.6} (over the drawn ones {e_cpu:.6}); MiddleGray {}, \
             exposure range {} .. {}",
            e[0], world.post.middle_gray, world.post.min_exposure, world.post.max_exposure
        );
    }
    // psFilmicTonemap with the bloom off against the harness's CPU curve
    if let Some(s) = &world.post_shaders {
        let mut p = world.post;
        p.bloom_scale = [0.0; 3];
        let mut f2 = frame.clone();
        f2.post = post::chain(s, &p, [wd, ht]).unwrap();
        // the tonemap written as floats (not RGBA8) to see its values before they are rounded to 8 bits
        if let Some(last) = f2.post.last_mut() {
            last.format = post::RGBA32F;
        }
        let raw = world.raw().clone();
        let r2 = dev.render(&f2, &world.textures, &raw).unwrap();
        let e = dev.read_pass("post:exposure").unwrap().map_or(1.0, |v| v.2[0]);
        let ldr = dev.read_pass("post:ldr").unwrap().expect("the tonemap as floats").2;
        let (mut worst, mut steps) = (0f32, 0usize);
        for (k, px) in r2.hdr.chunks_exact(4).enumerate() {
            let want = tonemap::map([px[0], px[1], px[2]], e, &p);
            for c in 0..3 {
                let got = ldr[4 * k + c];
                worst = worst.max((got - want[c]).abs());
                if (got * 255.0).round() != (want[c] * 255.0).round() {
                    steps += 1;
                }
            }
        }
        println!(
            "psFilmicTonemap (bloom off, written as floats) against the CPU curve: worst difference {worst:.2e} over \
             {} pixels ({steps} channel values on the other side of an 8-bit step)",
            r2.hdr.len() / 4
        );
        // the 8-bit picture against the float one: how the target rounds
        if let Some(last) = f2.post.last_mut() {
            last.format = post::RGBA8;
        }
        let r3 = dev.render(&f2, &world.textures, &raw).unwrap();
        let (mut q_same, mut q_n) = (0usize, 0usize);
        for (k, v) in ldr.iter().enumerate() {
            if k % 4 == 3 {
                continue;
            }
            q_n += 1;
            if f32::from(r3.ldr[k]) == (v.clamp(0.0, 1.0) * 255.0).round() {
                q_same += 1;
            }
        }
        println!("  the RGBA8 target holds round(v x 255) for {q_same} of {q_n} channel values");
    }
    if a.iter().any(|s| s == "--job") {
        let Some(exe) = common::shaderview() else {
            println!("--job: shaderview.exe not found (SHADERVIEW)");
            return;
        };
        let mut extra = HashMap::new();
        if let Some(l) = lin {
            extra.insert(water::DEPTH.to_owned(), l);
        }
        let fwd = |p: &std::path::Path| p.to_string_lossy().replace('\\', "/");
        let raw_out = out.join(format!("{stem}_shaderview.raw"));
        let pack = out.join(format!("{stem}_job.pack"));
        let j = job::write(&frame, &world.textures, world.raw(), &extra, &fwd(&raw_out), &fwd(&pack));
        std::fs::write(&pack, &j.pack).unwrap();
        let jf = out.join(format!("{stem}_job.txt"));
        std::fs::write(&jf, &j.text).unwrap();
        for s in &j.skipped {
            println!("  job: {s}");
        }
        let t = Instant::now();
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg(&jf);
        if warp {
            cmd.arg("-warp");
        }
        let o = cmd.output().unwrap();
        println!(
            "shaderview: {} ({:.1} s): {}{}",
            o.status,
            t.elapsed().as_secs_f64(),
            String::from_utf8_lossy(&o.stdout).trim(),
            String::from_utf8_lossy(&o.stderr).trim()
        );
        let theirs: Vec<f32> = std::fs::read(&raw_out)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        let stencil = std::fs::read(out.join(format!("{stem}_shaderview.raw.stencil"))).unwrap_or_default();
        let (mut same, mut worst) = (0usize, 0f32);
        let mut by_class = [0usize; 4];
        let mut class_only = 0;
        let mut diff = vec![0u8; r.ldr.len()];
        for (k, (a, b)) in r.hdr.chunks_exact(4).zip(theirs.chunks_exact(4)).enumerate() {
            let d = a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0f32, f32::max);
            let bits = a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits());
            let class = stencil.get(k).is_none_or(|s| *s == r.classes[k]);
            if bits && class {
                same += 1;
            } else {
                by_class[usize::from(r.classes[k].min(3))] += 1;
                if bits {
                    class_only += 1;
                }
                diff[4 * k] = 255;
                diff[4 * k + 3] = 255;
            }
            worst = worst.max(d);
        }
        println!("  differing pixels by our draw class (none, opaque, tested, blended): {by_class:?}; {class_only} differ in class only");
        common::save_png(&out.join(format!("{stem}_diff.png")), wd, ht, &diff).unwrap();
        println!(
            "parity with shaderview: {} draws in the job (of {}), {same} of {} pixels identical (colour bits and draw \
             class), worst colour difference {worst:e}",
            j.draws,
            frame.draws.len(),
            r.hdr.len() / 4
        );
        for p in [raw_out.clone(), pack, jf] {
            std::fs::remove_file(p).ok();
        }
        std::fs::remove_file(out.join(format!("{stem}_shaderview.raw.stencil"))).ok();
    }
}
