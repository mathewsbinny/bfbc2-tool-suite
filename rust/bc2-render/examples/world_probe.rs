//! A level's world as the game-accurate view loads it: every note, and the prepared meshes whose name holds one of
//! the given words with their draws (entry, program, state, bounds). Round R5d's diagnosis tool.
//!
//! `cargo run --release -p bc2-render --example world_probe -- <level> [word ...]`
#![allow(
    clippy::print_stdout,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::chunks_exact_to_as_chunks,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

mod common;

use std::time::Instant;

use bc2_render::world::World;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opfor = common::opfor();
    let lv = a[1].clone();
    let words: Vec<String> = a[2..].iter().map(|w| w.to_lowercase()).collect();
    let t = Instant::now();
    let (name, placed, weather) = common::placed(&opfor, &lv).unwrap();
    let package = !opfor.join("work").join("levels").join(lv.to_lowercase()).is_dir();
    let world = if package {
        World::load_package(&opfor, &lv, &name, &placed, weather.as_deref()).unwrap()
    } else {
        World::load(&opfor, &lv, &name, &placed, weather.as_deref()).unwrap()
    };
    println!("{lv}: loaded in {:.2} s", t.elapsed().as_secs_f64());
    let (w, _) = bc2_render::world::resolve_weather(&world.files, &name, &placed, weather.as_deref());
    for (what, f) in [
        ("sun", &w.sun),
        ("sky", &w.sky),
        ("fog", &w.fog),
        ("light", &w.outdoor_light),
    ] {
        let mut keys: Vec<_> = f.iter().collect();
        keys.sort_by(|a, b| a.0.cmp(b.0));
        println!("  {what}: {keys:?}");
    }
    for n in &world.notes {
        println!("  note: {n}");
    }
    if std::env::var_os("PROBE_LODS").is_some() {
        use bc2_formats::meshdata::{Kind, parse_set};
        let mut by: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
        let mut meshes: Vec<String> = world.objects.iter().map(|o| o.prepared.mesh.to_lowercase()).collect();
        meshes.sort();
        meshes.dedup();
        for m in &meshes {
            let name = format!("{m}.res");
            let (Some(b), Some(k)) = (world.files.read(&name), world.files.kind(&name).and_then(Kind::of)) else {
                continue;
            };
            if let Ok(set) = parse_set(&b, k, false, None) {
                let e = by
                    .entry(String::from_utf8_lossy(&set.lod_settings).into_owned())
                    .or_default();
                e.0 += 1;
                e.1 += set.lods.len();
            }
        }
        for (k, (n, l)) in by {
            println!("  lod settings {k:?}: {n} meshes, {l} lods");
        }
    }
    let mut seen_l = std::collections::HashSet::new();
    for (o, l) in world.objects.iter().zip(&world.object_lods) {
        let m = o.prepared.mesh.to_lowercase();
        if words.iter().any(|w| m.contains(w)) && seen_l.insert(m.clone()) {
            match l {
                Some(l) => println!(
                    "  lods of {m}: {} levels, settings {} {:?} enabled {:?}; bands {:?}; level draws {:?} notes {:?}",
                    l.settings.levels,
                    l.settings.name,
                    l.settings.distances,
                    l.settings.level_enabled,
                    (0..l.settings.levels).map(|k| l.settings.band(k)).collect::<Vec<_>>(),
                    l.levels
                        .iter()
                        .map(|p| p
                            .as_ref()
                            .map(|p| (p.draws.len(), p.draws.iter().map(|d| d.index_count).sum::<u32>())))
                        .collect::<Vec<_>>(),
                    l.levels
                        .iter()
                        .flatten()
                        .flat_map(|p| p.notes.clone())
                        .collect::<Vec<_>>()
                ),
                None => println!("  lods of {m}: none"),
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    for o in &world.objects {
        let m = o.prepared.mesh.to_lowercase();
        if !words.iter().any(|w| m.contains(w)) || !seen.insert(m.clone()) {
            continue;
        }
        println!(
            "  mesh {} at {:?} bounds {:?} notes {:?}",
            o.prepared.mesh,
            [o.world[3], o.world[7], o.world[11]],
            o.prepared.bounds,
            o.prepared.notes
        );
        for d in &o.prepared.draws {
            println!(
                "    draw {} program {} indices {} class {} state {:?} textures {:?}",
                d.entry,
                d.program,
                d.index_count,
                d.class,
                d.state.map(|s| s.word),
                d.textures
            );
        }
    }
    for p in placed
        .iter()
        .filter(|p| words.iter().any(|w| p.blueprint.to_lowercase().contains(w)))
        .take(20)
    {
        println!("  placed {} at {:?}", p.blueprint, [p.rows[12], p.rows[13], p.rows[14]]);
    }
    if let Some(w) = &world.water {
        println!("  water bounds {:?} draws {}", w.bounds, w.draws.len());
        for (k, d) in w.draws.iter().enumerate() {
            println!(
                "    {} first {} count {} program {} textures {:?} named {:?} material {:?} state {:?}",
                d.entry, d.first_index, d.index_count, d.program, d.textures, d.named, d.material, d.state
            );
            println!(
                "    ps resources {:?}",
                d.ps_reflect
                    .resources
                    .iter()
                    .map(|r| (r.name.clone(), r.kind, r.point))
                    .collect::<Vec<_>>()
            );
            if let Some(dir) = std::env::var_os("PROBE_DUMP") {
                let dir = std::path::PathBuf::from(dir);
                std::fs::write(dir.join(format!("{lv}_water{k}_ps.cso")), d.ps.as_slice()).unwrap();
                std::fs::write(dir.join(format!("{lv}_water{k}_vs.cso")), d.vs.as_slice()).unwrap();
            }
        }
    }
    if let Some(t) = &world.terrain {
        println!(
            "  terrain hf size {} min {:?} max {:?} cell {:?}",
            t.hf.size,
            t.hf.min,
            t.hf.max,
            t.hf.cell()
        );
    }
}
