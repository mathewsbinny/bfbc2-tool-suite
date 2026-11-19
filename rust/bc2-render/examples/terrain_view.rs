//! Draws the terrain patches around a point with their `PatchMaskShaders` entries, from a camera looking at it.
//!
//! `cargo run -p bc2-render --example terrain_view -- <opfor> <level> <out.png> <x> <z> <patches> <yaw> <pitch> <distance>`
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    missing_docs
)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use bc2_render::d3d::Device;
use bc2_render::files::{Files, level_folders};
use bc2_render::frame::{Camera, Frame, IDENTITY, Instance, draw_calls};
use bc2_render::material::{Db, TextureFiles};
use bc2_render::terrain::Terrain;
use bc2_render::{tonemap, weather};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (opfor, lv, out) = (PathBuf::from(&a[1]), a[2].clone(), PathBuf::from(&a[3]));
    let f = |i: usize| a[i].parse::<f64>().unwrap();
    let (x, z, n, yaw, pitch, dist) = (f(4), f(5), f(6) as usize, f(7), f(8), f(9));
    let files = Files::new(&level_folders(&opfor, &lv));
    let db = Db::parse(&files.read(&format!("levels/{lv}/shaders/dx11_single.res")).unwrap()).unwrap();
    let w = weather::read(&files, &format!("weather/r_{}", lv.replace('_', ""))).unwrap();
    let (values, post) = weather::light(&w, weather::NOON_DIR);
    let mut cache = TextureFiles::new();
    let t = Instant::now();
    let name = lv.to_uppercase();
    let terrain = Terrain::load(&files, &db, &name, &mut cache).unwrap();
    println!("terrain loaded in {:.2} s", t.elapsed().as_secs_f64());
    for (k, d) in terrain.draws.iter().enumerate() {
        if let Err(e) = d {
            println!("patch shader {k}: {e}");
        }
    }
    let (px, pz) = terrain.patch_at(x, z).unwrap();
    let mut instances = Vec::new();
    for j in pz.saturating_sub(n)..=pz + n {
        for i in px.saturating_sub(n)..=px + n {
            if let Some(p) = terrain.patch(i, j) {
                instances.push(Instance {
                    prepared: Arc::new(p),
                    world: IDENTITY,
                });
            }
        }
    }
    let y = terrain.hf.height(x, z);
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    let eye = [
        x + dist * pitch.cos() * yaw.sin(),
        y + dist * pitch.sin(),
        z + dist * pitch.cos() * yaw.cos(),
    ];
    let (wd, ht) = (1280u32, 720u32);
    let cam = Camera::look(
        eye,
        [x, y, z],
        60f64.to_radians(),
        0.5,
        4000.0,
        f64::from(wd) / f64::from(ht),
    );
    let frame = Frame {
        width: wd,
        height: ht,
        clear: [0.0; 4],
        draws: draw_calls(&instances, &values, &cam, [wd, ht]),
    };
    let mut dev = Device::new(false).unwrap();
    for _ in 0..3 {
        let r = dev.render(&frame, &cache, &terrain.raw).unwrap();
        let t = Instant::now();
        let (ldr, e) = tonemap::tonemap(&r.hdr, &r.mask(), &post, [82, 92, 107]);
        println!(
            "{} draws: submit {:.1} ms, readback {:.1} ms, tonemap {:.1} ms, exposure {e:.2}",
            r.draws,
            r.submit * 1e3,
            r.readback * 1e3,
            t.elapsed().as_secs_f64() * 1e3
        );
        let file = std::fs::File::create(&out).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), wd, ht);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&ldr).unwrap();
    }
    println!("wrote {}", out.display());
}
