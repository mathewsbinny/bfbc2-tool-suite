//! Times the editor's CPU ray queries on a real level (rust/BENCHMARKS.md, round HG1): the walk camera's
//! `ground_below` (a vertical ray), the view's picking rays (any direction) and `surface_below`, and prints a digest of
//! every answer so two builds can be compared (the same digest: the same hits, bit for bit).
//!
//! `cargo run --release -p bc2-editor --example ray_bench -- <OPFOR folder> <level> [rays]`
//! The level must be prepared (opened in the editor once); nothing is written.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    missing_docs
)]

use std::path::PathBuf;
use std::time::Instant;

use bc2_editor::{levels, query};
use bc2_scene::scene;

/// A small deterministic generator (the same rays in every build).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn digest(h: &mut sha1_smol::Sha1, hit: Option<&query::Hit>) {
    match hit {
        None => h.update(b"-"),
        Some(x) => {
            h.update(&x.t.to_bits().to_le_bytes());
            for v in x.point.iter().chain(&x.normal).chain(x.tri.iter().flatten()) {
                h.update(&v.to_bits().to_le_bytes());
            }
            h.update(&x.entity.map_or(u64::MAX, |e| e as u64).to_le_bytes());
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(&args[1]);
    let lv = args[2].to_lowercase();
    let rays: usize = args.get(3).map_or(200, |r| r.parse().unwrap());
    let t = Instant::now();
    let found = levels::load_options(&root, &lv).unwrap();
    let s = scene::load(&found.options).unwrap();
    println!("{lv}: loaded in {:.2} s", t.elapsed().as_secs_f64());
    let tris = s.terrain.as_ref().map_or(0, |t| t.1.len());
    let col = s.collision.as_ref().map_or(0, |c| c.meshes.len());
    println!("{lv}: {tris} terrain triangles, {col} placed collision meshes");
    let hf = s.heightfield.clone().expect("a height field");
    let (x0, z0) = (f64::from(hf.min[0]), f64::from(hf.min[2]));
    let (x1, z1) = (f64::from(hf.max[0]), f64::from(hf.max[2]));
    let mut rng = Lcg(0x4847_3100);
    let mut sha = sha1_smol::Sha1::new();
    // the walk's ray: straight down from 3 m above the height field
    let t = Instant::now();
    let mut hits = 0;
    let mut first = 0.0;
    for k in 0..rays {
        let x = x0 + (x1 - x0) * rng.next();
        let z = z0 + (z1 - z0) * rng.next();
        let y = hf.height(x, z) + 3.0;
        let h = query::ray_hit_except(&s, [x, y, z], [0.0, -1.0, 0.0], &|_| false);
        hits += usize::from(h.is_some());
        digest(&mut sha, h.as_ref());
        if k == 0 {
            first = t.elapsed().as_secs_f64();
        }
    }
    let down = t.elapsed().as_secs_f64();
    println!("{lv}: the first ray (the scene's ray structures built) {:.3} s", first);
    println!(
        "{lv}: {rays} vertical rays (ground_below) in {down:.3} s, {:.3} ms each, {hits} hits",
        down * 1000.0 / rays as f64
    );
    // picking: from a camera 30 - 230 m above the ground, in any downward direction
    let t = Instant::now();
    let mut hits = 0;
    for _ in 0..rays {
        let x = x0 + (x1 - x0) * rng.next();
        let z = z0 + (z1 - z0) * rng.next();
        let y = hf.height(x, z) + 30.0 + 200.0 * rng.next();
        let yaw = std::f64::consts::TAU * rng.next();
        let pitch = -0.05 - 1.4 * rng.next();
        let d = [pitch.cos() * yaw.cos(), pitch.sin(), pitch.cos() * yaw.sin()];
        let h = query::ray_hit(&s, [x, y, z], d);
        hits += usize::from(h.is_some());
        digest(&mut sha, h.as_ref());
    }
    let pick = t.elapsed().as_secs_f64();
    println!(
        "{lv}: {rays} picking rays in {pick:.3} s, {:.3} ms each, {hits} hits",
        pick * 1000.0 / rays as f64
    );
    // the placement's question: the surface below a point, from 100 km up
    let t = Instant::now();
    for _ in 0..rays {
        let x = x0 + (x1 - x0) * rng.next();
        let z = z0 + (z1 - z0) * rng.next();
        digest(&mut sha, query::surface_below(&s, x, z).as_ref());
    }
    let below = t.elapsed().as_secs_f64();
    println!(
        "{lv}: {rays} surface_below in {below:.3} s, {:.3} ms each",
        below * 1000.0 / rays as f64
    );
    println!("{lv}: digest {}", sha.digest());
}
