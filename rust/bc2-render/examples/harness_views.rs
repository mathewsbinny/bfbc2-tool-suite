//! Draws meshes together as `renderqa.py render --together` draws them (its three views: three-quarter, front, top;
//! its light and post-process; its background), side by side into one PNG, to compare with the C++ harness's picture.
//!
//! `cargo run -p bc2-render --example harness_views -- <opfor> <level> <out.png> <size> <mesh>...`
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

use bc2_render::d3d::Device;
use bc2_render::files::{Files, level_folders};
use bc2_render::frame::{Camera, Frame, IDENTITY, Instance, draw_calls};
use bc2_render::material::{Db, TextureFiles, prepare};
use bc2_render::{tonemap, weather};

const ENVMAP: &str = "Textures/Sky/Sky_Beach_Generic_RGBA16f";
const VIEWS: [(f64, f64); 3] = [(45.0, 28.0), (0.0, 8.0), (0.0, 89.0)];
const BACKGROUND: [u8; 3] = [82, 92, 107]; // renderqa.BACKGROUND (0.32, 0.36, 0.42) x 255, rounded

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (opfor, lv, out) = (PathBuf::from(&a[1]), a[2].clone(), PathBuf::from(&a[3]));
    let size: u32 = a[4].parse().unwrap();
    let files = Files::new(&level_folders(&opfor, &lv));
    let db_name = format!("levels/{lv}/shaders/dx11_single.res");
    let db = Db::parse(&files.read(&db_name).expect("database")).expect("parses");
    let w = weather::read(&files, &format!("weather/r_{}", lv.replace('_', ""))).expect("weather");
    println!("weather: {}", w.state);
    let (values, post) = weather::light(&w, weather::NOON_DIR);
    let mut cache = TextureFiles::new();
    let mut instances = Vec::new();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for m in &a[5..] {
        let p = prepare(&files, &db, m, ENVMAP, &mut cache);
        for n in &p.notes {
            println!("{m}: {n}");
        }
        if let Some((l, h)) = p.bounds {
            for k in 0..3 {
                lo[k] = lo[k].min(l[k]);
                hi[k] = hi[k].max(h[k]);
            }
        }
        println!("{m}: {} draws", p.draws.len());
        instances.push(Instance {
            prepared: Arc::new(p),
            world: IDENTITY,
        });
    }
    let mut dev = Device::new(false).expect("device");
    let mut sheet = vec![0u8; (size * size * 3 * 4) as usize];
    for (vi, (az, el)) in VIEWS.iter().enumerate() {
        let cam = Camera::orbit(lo, hi, *az, *el);
        let frame = Frame {
            width: size,
            height: size,
            clear: [0.0; 4],
            draws: draw_calls(&instances, &values, &cam, [size, size]),
            ..Frame::default()
        };
        let r = dev
            .render(&frame, &cache, &std::collections::HashMap::default())
            .expect("renders");
        let (ldr, e) = tonemap::tonemap(&r.hdr, &r.mask(), &post, BACKGROUND);
        println!(
            "view {vi}: {} draws, exposure {e:.3}, submit {:.1} ms, readback {:.1} ms",
            r.draws,
            r.submit * 1e3,
            r.readback * 1e3
        );
        for y in 0..size as usize {
            for x in 0..size as usize {
                let s = (y * size as usize + x) * 4;
                let d = (y * size as usize * 3 + vi * size as usize + x) * 4;
                sheet[d..d + 4].copy_from_slice(&ldr[s..s + 4]);
            }
        }
    }
    let f = std::fs::File::create(&out).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), size * 3, size);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&sheet).unwrap();
    println!("wrote {}", out.display());
}
