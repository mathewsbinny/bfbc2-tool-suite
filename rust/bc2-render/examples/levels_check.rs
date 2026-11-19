//! Every prepared stock level drawn the game's way, with no harness flag (round R5b): each folder of `work\levels`
//! loaded as the editor loads it, its world (objects, group members, terrain, strips, water, post) drawn once from
//! above its middle, and what is missing logged per level - the load's notes counted by kind (`no-entry`,
//! `missing-texture`, `layout-fallback`, `shader-set`, strip and water problems ...), the counts drawn and any error.
//!
//! `cargo run --release -p bc2-render --example levels_check -- <out folder> [<level>...]` (`OPFOR` names Project
//! OPFOR's folder): writes `<level>.png` and `levels_check.txt` there.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    missing_docs
)]

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use bc2_render::d3d::Device;
use bc2_render::frame::Camera;
use bc2_render::world::World;

/// A note's kind: the text before its first colon after the mesh name (`missing-texture`, `no-entry` ...).
fn kind(note: &str) -> String {
    for k in [
        "no-mesh",
        "no-layout",
        "no-entry",
        "no-program",
        "layout-fallback",
        "bound-empty",
        "missing-texture",
        "dimension",
        "shader-set",
        "strip:",
        "water:",
        "patch shader",
        "terrain:",
        "post-process:",
        "placements:",
    ] {
        if note.contains(k) {
            return k.trim_end_matches(':').to_owned();
        }
    }
    "other".into()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let out = PathBuf::from(&a[1]);
    std::fs::create_dir_all(&out).ok();
    let opfor = common::opfor();
    let mut levels: Vec<String> = a[2..].to_vec();
    if levels.is_empty() {
        let mut v: Vec<String> = std::fs::read_dir(opfor.join("work").join("levels"))
            .unwrap()
            .flatten()
            .filter(|e| e.path().join("dist").is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        levels = v;
    }
    let mut dev = Device::new(false).unwrap();
    let mut log = String::new();
    let mut ok = 0;
    for lv in &levels {
        let t = Instant::now();
        let r = (|| -> Result<String, String> {
            let (name, placed, weather) = common::placed(&opfor, lv)?;
            let mut w = World::load(&opfor, lv, &name, &placed, weather.as_deref())?;
            let load = t.elapsed().as_secs_f64();
            // the middle of the terrain (else of the objects), from the south-west, above
            let target = if let Some(tr) = &w.terrain {
                let (x, z) = (
                    f64::from(tr.hf.min[0] + tr.hf.max[0]) / 2.0,
                    f64::from(tr.hf.min[2] + tr.hf.max[2]) / 2.0,
                );
                [x, tr.hf.height(x, z), z]
            } else {
                let n = w.objects.len().max(1) as f64;
                let s = w.objects.iter().fold([0.0; 3], |s, i| {
                    [s[0] + i.world[3], s[1] + i.world[7], s[2] + i.world[11]]
                });
                s.map(|v| v / n)
            };
            let (yaw, pitch, dist) = (225f64.to_radians(), 35f64.to_radians(), 350.0);
            let eye = [
                target[0] + dist * pitch.cos() * yaw.sin(),
                target[1] + dist * pitch.sin(),
                target[2] + dist * pitch.cos() * yaw.cos(),
            ];
            let (wd, ht) = (960u32, 540u32);
            let cam = Camera::look(eye, target, 60f64.to_radians(), 0.5, 4000.0, f64::from(wd) / f64::from(ht));
            let t2 = Instant::now();
            let f = w.frame(&cam, target, [wd, ht], true)?;
            let raw = w.raw().clone();
            let r = dev.render(&f, &w.textures, &raw)?;
            let frame_s = t2.elapsed().as_secs_f64();
            if !r.ldr.is_empty() {
                common::save_png(&out.join(format!("{lv}.png")), wd, ht, &r.ldr)?;
            }
            let nan = r.hdr.iter().filter(|v| !v.is_finite()).count();
            let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
            // the load's own summary and weather lines are not findings
            for n in w.notes.iter().filter(|n| !n.contains(" group members drawn") && !n.starts_with("weather: ")) {
                *kinds.entry(kind(n)).or_default() += 1;
            }
            kinds.remove("other");
            let c = w.counts;
            let mut s = format!(
                "{lv}: ok - load {load:.1} s, frame {frame_s:.2} s ({} draws{}); {} objects, {} group members, {} meshes, \
                 {} overridden, {} strip draws, {} water draws, terrain {}, post {}{}",
                r.draws,
                if nan > 0 { format!(", {nan} non-finite values") } else { String::new() },
                c.objects,
                c.members,
                c.meshes,
                c.overridden,
                c.strips,
                c.water,
                if w.terrain.is_some() { "drawn" } else { "none" },
                if r.ldr.is_empty() { "CPU tonemap" } else { "DICE's" },
                if kinds.is_empty() {
                    String::new()
                } else {
                    format!(
                        "\n    missing / notes: {}",
                        kinds.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", ")
                    )
                }
            );
            let mut shown = 0;
            for n in w.notes.iter().filter(|n| !n.contains(" group members drawn") && !n.starts_with("weather: ")) {
                shown += 1;
                if shown > 12 {
                    let _ = write!(s, "\n    ... ({} more)", w.notes.len() - 2 - 12);
                    break;
                }
                let _ = write!(s, "\n    {n}");
            }
            Ok(s)
        })();
        let line = match r {
            Ok(s) => {
                ok += 1;
                s
            }
            Err(e) => format!("{lv}: FAILED - {e}"),
        };
        println!("{line}");
        log.push_str(&line);
        log.push('\n');
        dev.forget();
    }
    let _ = writeln!(log, "{ok} of {} levels drawn", levels.len());
    println!("{ok} of {} levels drawn", levels.len());
    std::fs::write(out.join("levels_check.txt"), log).unwrap();
}
