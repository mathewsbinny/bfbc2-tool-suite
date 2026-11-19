//! Draws a prepared level's placed objects (NavKit's `objects.json`: blueprint and transform) and the terrain patches
//! around a point, from a camera looking at it - the editor's game-accurate view without the editor.
//!
//! `cargo run -p bc2-render --example level_view -- <opfor> <level> <out.png> <x> <y> <z> <yaw> <pitch> <distance>
//! [<only: text a mesh name has>] [--list]`
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    missing_docs
)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use bc2_render::d3d::Device;
use bc2_render::files::{Files, level_folders};
use bc2_render::frame::{self, Camera, Frame, Instance, draw_calls};
use bc2_render::material::{Db, TextureFiles, prepare};
use bc2_render::terrain::Terrain;
use bc2_render::{tonemap, weather};

fn mesh_of(files: &Files, blueprint: &str) -> Option<String> {
    let xml = bc2_formats::dbx::to_xml(&files.read(&format!("{blueprint}.dbx"))?).ok()?;
    let at = xml.find("name=\"Mesh\"")?;
    let head = &xml[..at];
    let r = &head[head.rfind("ref=\"")? + 5..];
    let r = &r[..r.find('"')?];
    (r != "null").then(|| r[..r.rfind('/').unwrap()].to_owned())
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (opfor, lv, out) = (PathBuf::from(&a[1]), a[2].clone(), PathBuf::from(&a[3]));
    let f = |i: usize| a[i].parse::<f64>().unwrap();
    let (x, y, z, yaw, pitch, dist) = (f(4), f(5), f(6), f(7), f(8), f(9));
    let only = a.get(10).filter(|s| !s.starts_with("--")).cloned();
    let list = a.iter().any(|s| s == "--list");
    let files = Files::new(&level_folders(&opfor, &lv));
    let db = Db::parse(&files.read(&format!("levels/{lv}/shaders/dx11_single.res")).unwrap()).unwrap();
    let w = weather::read(&files, &format!("weather/r_{}", lv.replace('_', ""))).unwrap();
    let (values, post) = weather::light(&w, weather::NOON_DIR);
    let mut cache = TextureFiles::new();
    let objects: serde_json_lite::Objects =
        serde_json_lite::read(&opfor.join("work/levels").join(&lv).join("objects.json"));
    let mut meshes: HashMap<String, Option<Arc<bc2_render::material::Prepared>>> = HashMap::new();
    let mut bps: HashMap<String, Option<String>> = HashMap::new();
    let mut instances = Vec::new();
    for (bp, m) in objects {
        if !bp.starts_with("objects/") {
            continue;
        }
        let Some(mesh) = bps.entry(bp.clone()).or_insert_with(|| mesh_of(&files, &bp)).clone() else {
            continue;
        };
        if only.as_ref().is_some_and(|o| !mesh.contains(o.as_str())) {
            continue;
        }
        let p = meshes
            .entry(mesh.clone())
            .or_insert_with(|| {
                let p = prepare(&files, &db, &mesh, "terrains/mp_005/textures/mp05_envmap", &mut cache);
                if list {
                    let kinds: Vec<u8> = p.draws.iter().map(|d| d.kind).collect();
                    println!(
                        "{mesh}: {} draws, kinds {kinds:?}, bounds {:?}",
                        p.draws.len(),
                        p.bounds
                    );
                }
                (!p.draws.is_empty()).then(|| Arc::new(p))
            })
            .clone();
        if let Some(p) = p {
            let mut rows = [0.0; 16];
            rows.copy_from_slice(&m);
            rows[15] = 1.0;
            instances.push(Instance {
                prepared: p,
                world: frame::from_rows(&rows),
            });
        }
    }
    println!("{} objects", instances.len());
    let terrain = Terrain::load(
        &files,
        &db,
        &lv.to_uppercase(),
        "terrains/mp_005/textures/mp05_envmap",
        &mut cache,
    )
    .unwrap();
    if only.is_none() {
        let (px, pz) = terrain.patch_at(x, z).unwrap();
        for j in pz.saturating_sub(24)..=pz + 24 {
            for i in px.saturating_sub(24)..=px + 24 {
                if let Some(p) = terrain.patch(i, j) {
                    instances.push(Instance {
                        prepared: Arc::new(p),
                        world: frame::IDENTITY,
                    });
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
    let (wd, ht) = (1280u32, 720u32);
    let cam = Camera::look(
        eye,
        [x, y, z],
        60f64.to_radians(),
        0.5,
        4000.0,
        f64::from(wd) / f64::from(ht),
    );
    let mut dev = Device::new(false).unwrap();
    for _ in 0..3 {
        let t = Instant::now();
        let frame = Frame {
            width: wd,
            height: ht,
            clear: [0.192, 0.349, 0.459, 0.0],
            draws: draw_calls(&instances, &values, &cam, [wd, ht]),
            ..Frame::default()
        };
        let build = t.elapsed().as_secs_f64();
        let r = dev.render(&frame, &cache, &terrain.raw).unwrap();
        let t = Instant::now();
        let bg = tonemap::map([0.192, 0.349, 0.459], 1.0, &post).map(|v| (v * 255.0).round() as u8);
        let (ldr, _) = tonemap::tonemap(&r.hdr, &r.mask(), &post, bg);
        println!(
            "{} draws: build {:.1} ms, submit {:.1} ms, readback {:.1} ms, tonemap {:.1} ms",
            r.draws,
            build * 1e3,
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
}

/// NavKit's `objects.json` read without a JSON library: `[{"bp": "...", ..., "m": [16 numbers]}, ...]`.
mod serde_json_lite {
    pub type Objects = Vec<(String, Vec<f64>)>;

    pub fn read(path: &std::path::Path) -> Objects {
        let text = std::fs::read_to_string(path).unwrap();
        let mut out = Vec::new();
        for item in text.split("{\"bp\": \"").skip(1) {
            let bp = item[..item.find('"').unwrap()].to_owned();
            let Some(at) = item.find("\"m\": [") else { continue };
            let nums = &item[at + 6..];
            let nums = &nums[..nums.find(']').unwrap()];
            let m: Vec<f64> = nums.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if m.len() == 16 {
                out.push((bp, m));
            }
        }
        out
    }
}
