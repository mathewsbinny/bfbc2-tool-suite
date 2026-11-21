//! Times the Rust scene alone (rust/BENCHMARKS.md): the collision index, the entities only, the whole load, and the
//! readers on their own, best of N runs.
//!
//! `cargo run --release -p bc2-scene --example scene_bench -- <OPFOR folder> <level> [runs]`
//! (`RAYON_NUM_THREADS=1` for one thread). The level must be prepared (`level.load` once); nothing is written.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::map_unwrap_or,
    clippy::too_many_lines,
    missing_docs
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bc2_formats::{havok, terrain, water};
use bc2_scene::{collision, scene, walk};

/// The first file under `dir` named `name` (any case), in walk order.
fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    walk::find(dir, &[&name.to_lowercase()])
        .into_iter()
        .flatten()
        .find(|f| f.rel.rsplit('/').next() == Some(name.to_lowercase().as_str()))
        .map(|f| f.path)
}

fn files_in(dir: &Path, f: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| f(&p.file_name().unwrap().to_string_lossy().to_lowercase()))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn best<T>(runs: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut out = None;
    let mut best = f64::MAX;
    for _ in 0..runs {
        let t = Instant::now();
        let r = f();
        best = best.min(t.elapsed().as_secs_f64());
        out = Some(r);
    }
    (best, out.unwrap())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(&args[1]);
    let lv = args[2].to_lowercase();
    let runs: usize = args.get(3).map_or(3, |r| r.parse().unwrap());
    let base = root.join("work").join("levels").join(&lv);
    let mut bases = vec![base.join("pkg"), base.join("dist")];
    bases.extend(files_in(&root.join("work").join("navkit").join("common"), |_| true));
    let partition = bases[..2]
        .iter()
        .flat_map(|b| files_in(&b.join("levels"), |n| n == format!("{lv}.dbx")))
        .next()
        .expect("level partition");
    let name = partition.file_stem().unwrap().to_string_lossy().into_owned();
    let mut groups = Vec::new();
    for b in &bases[..2] {
        for g in files_in(&b.join("levels").join(&lv), |n| n.ends_with("_havokasset.dbx")) {
            if !groups.iter().any(|x: &PathBuf| x.file_name() == g.file_name()) {
                groups.push(g);
            }
        }
    }
    let tname = scene::terrain_name(&partition).unwrap().unwrap_or_else(|| lv.clone());
    let tdir = base.join("terrain");
    let heightfield = find(&tdir, &format!("{tname}.heightfield-000.res"));
    let wmesh = find(&tdir, &format!("{tname}.water.mesh.res"));
    let wvis = find(&tdir, &format!("{tname}.water.res"));
    let options = scene::LoadOptions {
        groups,
        geometry: true,
        heightfield: heightfield.clone(),
        terrain_stride: 2,
        water: wmesh.clone().map(|m| (m, wvis.clone())),
        ..scene::LoadOptions::new(name, partition, bases.clone())
    };
    println!("{lv}: {} threads, best of {runs}", rayon::current_num_threads());
    let (t, found) = best(runs, || {
        bases
            .iter()
            .map(|b| walk::find(b, &["_havok.res", ".xml", ".dbx"]))
            .collect::<Vec<_>>()
    });
    let xml: Vec<&walk::Found> = found.iter().flat_map(|f| &f[1]).collect();
    let dbx: Vec<&walk::Found> = found.iter().flat_map(|f| &f[2]).collect();
    println!(
        "  the walk alone                     {t:7.3} s  {} XML files, {} .dbx",
        xml.len(),
        dbx.len()
    );
    for (what, files) in [("XML files", &xml), (".dbx files", &dbx)] {
        let (t, bytes) = best(runs, || {
            use rayon::prelude::*;
            files
                .par_iter()
                .map(|f| fs::read(&f.path).map_or(0, |d| d.len()))
                .sum::<usize>()
        });
        println!("  reading the {what:<22} {t:7.3} s  {} MB", bytes >> 20);
    }
    let (t, hv) = best(runs, || collision::havok_index(&bases));
    println!("  havok_index alone (no aliases)     {t:7.3} s  {} packfiles", hv.len());
    let (t, idx) = best(runs, || collision::index(&bases));
    println!(
        "  collision index (walk + aliases)   {t:7.3} s  {} packfiles",
        idx.havok.len()
    );
    // the mesh-alias cache: built once (cold), then reused (warm), in a temp folder removed after
    let cache = std::env::temp_dir().join(format!("bc2_scene_bench_cache_{}", std::process::id()));
    let _ = fs::remove_dir_all(&cache);
    let t = Instant::now();
    let _ = collision::index_cached(&bases, Some(&cache));
    let cold = t.elapsed().as_secs_f64();
    let (warm, _) = best(runs, || collision::index_cached(&bases, Some(&cache)));
    println!("  collision index, cache cold / warm {cold:7.3} s / {warm:.3} s");
    let _ = fs::remove_dir_all(&cache);
    let light = scene::LoadOptions {
        geometry: false,
        ..options.clone()
    };
    let (t, s) = best(runs, || scene::load(&light).unwrap());
    println!(
        "  entities only                      {t:7.3} s  {} entities",
        s.entities.len()
    );
    let (t, s) = best(runs, || scene::load(&options).unwrap());
    println!(
        "  whole load                         {t:7.3} s  {} entities, {} meshes",
        s.entities.len(),
        s.collision.as_ref().map_or(0, |c| c.meshes.len())
    );
    let packfiles: Vec<Vec<u8>> = s
        .collision
        .as_ref()
        .map(|c| {
            let havok = idx.resolved().unwrap();
            c.blueprints
                .keys()
                .filter_map(|bp| havok.get(bp).map(|p| fs::read(p).unwrap()))
                .collect()
        })
        .unwrap_or_default();
    let (t, _) = best(runs, || {
        packfiles.iter().filter(|d| havok::triangles(d).is_ok()).count()
    });
    println!(
        "  Havok triangles, one thread        {t:7.3} s  {} packfiles (from memory)",
        packfiles.len()
    );
    // round VW1: the per-part split the scene now reads beside the triangles
    let (t, split) = best(runs, || {
        packfiles
            .iter()
            .filter(|d| havok::parts(d).ok().flatten().is_some_and(|p| p.split()))
            .count()
    });
    println!("  Havok parts, one thread            {t:7.3} s  {split} packfiles split per part");
    if let Some(h) = heightfield {
        let data = fs::read(h).unwrap();
        let (t, hf) = best(runs, || terrain::parse(&data).unwrap());
        let (tm, _) = best(runs, || hf.mesh(2));
        println!(
            "  terrain parse + heights / mesh     {t:7.3} s / {tm:.3} s  size {}",
            hf.size
        );
    }
    if let Some(m) = wmesh {
        let data = fs::read(m).unwrap();
        let (t, w) = best(runs, || water::parse_mesh(&data).unwrap());
        println!(
            "  water mesh                         {t:7.3} s  {} triangles",
            w.triangles.len()
        );
    }
}
