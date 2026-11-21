//! Round R5e, the WW2 levels' deep sea, proven on the game's own data:
//!
//! - a program's material registers are its block header's window (RNDR-DRAW-8): on every program of DICE's MP_005 /
//!   SP_01 and OPFOR's WW2 levels whose pixel shader has a `constants` array, the header's `[0x19, 0x18)` is that
//!   array's registers; where a pixel shader has none, the window holds every block constant and covers only the
//!   material's own variables: OPFOR's translated BF1943 water's `waterMaterial` array, or the WW2 terrain shaders'
//!   constants declared one by one at their records' registers;
//! - Wake Island's water plans its `g_depthScaleFactors` (the frame's projection, written each frame), so the open
//!   sea past the reefs is drawn: R5d's 600 m view was 53.5 % near black, the black floor seen through a water of
//!   alpha 0.
//!
//! Skipped (with a note) without OPFOR's levels, the game copy or Direct3D 11.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    missing_docs
)]

use crate::common;

use std::collections::HashMap;
use std::path::Path;

use bc2_formats::shaderdb::query;
use bc2_render::d3d::Device;
use bc2_render::dxbc;
use bc2_render::files::{self, Files};
use bc2_render::frame::Camera;
use bc2_render::live::{Dyn, Key, Live, Options};
use bc2_render::material::Db;

/// A level's Direct3D 11 shader database, from its prepared folders or the game copy; `None` when absent.
fn database(opfor: &Path, lv: &str) -> Option<Db> {
    let files = if opfor.join("work").join("levels").join(lv).is_dir() {
        Files::new(&files::level_folders(opfor, lv))
    } else {
        Files::bundles(&files::package_stack(&opfor.join("game"), lv)?).ok()?
    };
    let name = format!("levels/{lv}/shaders/dx11_single.res");
    let data = files.read(&name).or_else(|| {
        files.fetch(&files.missed()).ok()?;
        files.read(&name)
    })?;
    Db::parse(&data).ok()
}

// spec: RNDR-DRAW-8
#[test]
#[ignore = "game tier: six levels' shader databases; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn block_windows_are_the_material_registers() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    let mut levels = 0;
    for lv in [
        "mp_005",
        "sp_01",
        "wake_island_s",
        "guadal_canal",
        "iwo_jima_s",
        "coral_sea",
    ] {
        let Some(db) = database(&opfor, lv) else {
            println!("{lv}: skipped, no shader database");
            continue;
        };
        let entry_of: HashMap<usize, String> = db
            .entries
            .iter()
            .flat_map(|(name, g)| {
                let name = String::from_utf8_lossy(name).into_owned();
                db.groups.get(g).into_iter().flatten().map(move |&k| (k, name.clone()))
            })
            .collect();
        let mut reflected: HashMap<usize, dxbc::Reflection> = HashMap::new();
        let (mut same, mut dropped, mut other) = (0, 0, Vec::new());
        for (k, p) in db.programs.iter().enumerate() {
            let block = db.param(p.block2);
            let Some(window) = query::block_registers(block) else {
                continue;
            };
            let r = reflected
                .entry(p.ps)
                .or_insert_with(|| dxbc::reflect(&db.ps[p.ps].code));
            if let Some(v) = r.var("constants") {
                let regs = [v.offset / 16, (v.offset + v.size) / 16].map(|x| u8::try_from(x).unwrap());
                assert_eq!(window, regs, "{lv}: program {k}'s window against its `constants`");
                same += 1;
                continue;
            }
            let constants = query::block_constants(block, false);
            if constants.is_empty() {
                continue;
            }
            for c in &constants {
                assert!(
                    (window[0]..window[1]).contains(&c.register),
                    "{lv}: program {k}: register {} outside {window:?}",
                    c.register
                );
            }
            // the $Globals variables the window covers: none where DICE's compiler dropped an unread material; else
            // each is the material's own array (`waterMaterial`) or one of the block's constants declared alone, at
            // the record's register (a terrain strip's engine-named values): the material is the window's, never an
            // engine value's
            let (lo, hi) = (u32::from(window[0]), u32::from(window[1]));
            let covered: Vec<&dxbc::Var> = r
                .globals
                .iter()
                .filter(|v| v.offset / 16 < hi && (v.offset + v.size).div_ceil(16) > lo)
                .collect();
            if covered.is_empty() {
                dropped += 1;
                continue;
            }
            for v in &covered {
                let own = constants
                    .iter()
                    .any(|c| c.name == v.name.as_bytes() && u32::from(c.register) == v.offset / 16);
                assert!(
                    own || v.name == "waterMaterial",
                    "{lv}: program {k}: {} in the window {window:?}",
                    v.name
                );
            }
            let kind = if covered.iter().any(|v| v.name == "waterMaterial") {
                "waterMaterial"
            } else {
                "by name"
            };
            other.push(format!("{} ({kind})", entry_of.get(&k).map_or("?", String::as_str)));
        }
        other.sort();
        other.dedup();
        let by_name = other.iter().filter(|e| e.ends_with("(by name)")).count();
        let arrays: Vec<&String> = other.iter().filter(|e| !e.ends_with("(by name)")).collect();
        println!(
            "{lv}: {same} programs' windows are their `constants`, {dropped} read none of their block constants; \
             {by_name} entries read them by name, in `waterMaterial`: {arrays:?}"
        );
        assert!(same > 0, "{lv}: no program checked");
        if lv == "wake_island_s" {
            assert!(
                other.iter().any(|e| e.contains("wake_water (waterMaterial)")),
                "Wake Island's water reads `waterMaterial`"
            );
        }
        levels += 1;
    }
    if levels == 0 {
        println!("skipped: no level");
    }
}

// spec: RNDR-DRAW-8, RNDR-WATER-3
#[test]
#[ignore = "game tier: Wake Island and Guadalcanal drawn; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn the_ww2_open_sea_is_drawn() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    let mut checked = 0;
    // (level, the view: target x, y, z, yaw, pitch, distance; near-black share before R5e)
    let views: [(&str, [f64; 6], f64); 2] = [
        ("wake_island_s", [-200.0, 215.0, -100.0, 225.0, 25.0, 600.0], 0.535),
        ("guadal_canal", [0.0, 190.0, 0.0, 135.0, 30.0, 1200.0], 0.0),
    ];
    for (lv, v, before) in views {
        let Some(mut w) = crate::r5d::world(&opfor, lv) else {
            continue;
        };
        let size = [640u32, 360];
        let (yaw, pitch, d) = (v[3].to_radians(), v[4].to_radians(), v[5]);
        let eye = [
            v[0] + d * pitch.cos() * yaw.sin(),
            v[1] + d * pitch.sin(),
            v[2] + d * pitch.cos() * yaw.cos(),
        ];
        let target = [v[0], v[1], v[2]];
        let cam = Camera::look(eye, target, 60f64.to_radians(), 0.5, 4000.0, 640.0 / 360.0);
        let mut live = Live::new(&w.light);
        live.set(w.sources(target));
        live.build(&cam, size, Options::exact(true));
        // the water's pixel shader is given the frame's depth factors in its material registers
        let water: Vec<_> = live
            .instances
            .iter()
            .filter(|i| i.source.key == Key::Water)
            .flat_map(|i| &i.draws)
            .collect();
        assert!(!water.is_empty(), "{lv}: no water drawn");
        for d in &water {
            assert!(
                d.ps.holes.iter().any(|h| h.key == Dyn::DepthFactors),
                "{lv}: the water's g_depthScaleFactors is not written"
            );
        }
        let mut dev = match Device::new(false) {
            Ok(d) => d,
            Err(e) => {
                println!("skipped: {e}");
                return;
            }
        };
        let (depth, post) = w.passes(&cam, size, true).unwrap();
        let raw = w.raw().clone();
        dev.render_live(&live, size, w.clear(), depth.as_ref(), &post, &w.textures, &raw)
            .unwrap();
        let r = dev.read_scene(post.last()).unwrap();
        let n = r.depth.len();
        let dark = |t: u8| {
            (0..n)
                .filter(|&i| r.ldr[4 * i] < t && r.ldr[4 * i + 1] < t && r.ldr[4 * i + 2] < t)
                .count() as f64
                / n as f64
        };
        let (black, dim) = (dark(8), dark(40));
        // the sea is blue: its blended pixels' mean blue above their red
        let (mut red, mut blue, mut sea) = (0.0, 0.0, 0.0);
        for i in 0..n {
            if r.classes[i] == bc2_render::material::BLENDED {
                red += f64::from(r.hdr[4 * i]);
                blue += f64::from(r.hdr[4 * i + 2]);
                sea += 1.0;
            }
        }
        println!(
            "{lv}: near-black {:.1} % (before R5e {:.1} %), dark {:.1} %, sea mean red {:.3} blue {:.3}",
            black * 100.0,
            before * 100.0,
            dim * 100.0,
            red / sea,
            blue / sea
        );
        assert!(
            black < 0.01 && dim < 0.05,
            "{lv}: {:.1} % near black, {:.1} % dark",
            black * 100.0,
            dim * 100.0
        );
        assert!(blue > red, "{lv}: the sea is not blue");
        checked += 1;
    }
    if checked == 0 {
        println!("skipped: no WW2 level");
    }
}
