//! Times the asset preview's orbit picture (rust/BENCHMARKS.md, "PR1b-0"): a blueprint and a soldier read from the
//! game copy, then the orbit drawn at a panel's size as a drag turns it, and prints a digest of every picture so two
//! builds can be compared.
//!
//! `cargo run --release -p bc2-editor --example preview_bench -- [width] [height] [frames]`
//! Reads the OPFOR folder's game copy; nothing is written.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    missing_docs
)]

use std::time::{Duration, Instant};

use bc2_editor::preview::{self, Camera, Preview, Subject};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let w: usize = args.get(1).map_or(1200, |a| a.parse().unwrap());
    let h: usize = args.get(2).map_or(900, |a| a.parse().unwrap());
    let frames: usize = args.get(3).map_or(30, |a| a.parse().unwrap());
    let root = bc2_editor::levels::find_opfor(None).expect("an OPFOR folder");
    for subject in [
        Subject::Blueprint("objects/buildings/guardtower/guardtower".into()),
        Subject::Mesh("Characters/Beach/3P/Us_Infantry/Us_Infantry_mesh_Mesh".into()),
    ] {
        let name = format!("{subject:?}");
        let mut pv = Preview::default();
        pv.show(&root, subject);
        pv.wait(Duration::from_mins(10));
        if pv.models.is_empty() {
            println!("{name}: not read ({})", pv.status);
            continue;
        }
        let tris: usize = pv
            .models
            .iter()
            .flat_map(|m| &m.read.parts)
            .map(|p| p.indices.len())
            .sum();
        pv.camera = Camera::orbit();
        let mut sha = sha1_smol::Sha1::new();
        let mut each = Vec::new();
        for k in 0..frames {
            if let Camera::Orbit { yaw, .. } = &mut pv.camera {
                *yaw += 0.05;
            }
            let t = Instant::now();
            let (img, owner) = pv.draw(w, h, preview::Colours::default());
            each.push(t.elapsed().as_secs_f64() * 1000.0);
            if k % 10 == 0 {
                sha.update(&img.rgb);
                sha.update(&owner.iter().flat_map(|o| o.to_le_bytes()).collect::<Vec<u8>>());
            }
        }
        each.sort_by(f64::total_cmp);
        println!(
            "{name}: {tris} triangles, {frames} orbit pictures at {w} x {h}: median {:.1} ms, max {:.1} ms; digest {}",
            each[each.len() / 2],
            each[each.len() - 1],
            sha.digest()
        );
    }
}
