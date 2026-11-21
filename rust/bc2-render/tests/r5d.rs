//! Round R5d, the game-accurate view's visual gaps, proven on the game's own data:
//!
//! - the mask atlas's tiles span `flag` x `flag` cells (TERR-VT-7): on DICE's MP_009 / MP_005 and OPFOR's Wake Island
//!   every tile a patch map names lies in one aligned block, and the patches drawn read one continuous mask across
//!   the cells of a block;
//! - the height map's world scale is the height field's range (RNDR-TERR-5) and the water reads the height map
//!   (RNDR-WATER-2): Wake Island's beach and lagoon no longer draw black;
//! - the sky dome (RNDR-SKY-1): MP_005's background is drawn, blue above the horizon;
//! - mesh levels of detail from the mesh sets' LOD settings (RNDR-LOD-1) and the patches' levels (RNDR-VIEW-2): fewer
//!   indices drawn on a wide view;
//! - the Package bundles read through the bundle cache give the bytes a plain read gives.
//!
//! Skipped (with a note) without OPFOR's prepared levels, the game copy or Direct3D 11.
#![allow(
    clippy::unwrap_used,
    clippy::many_single_char_names,
    clippy::naive_bytecount,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    missing_docs
)]

use crate::common;

use std::collections::HashMap;
use std::path::Path;

use bc2_render::d3d::Device;
use bc2_render::files::{self, Files};
use bc2_render::frame::{Camera, Rendered};
use bc2_render::live::{Key, Live, Options};
use bc2_render::terrain::{HEIGHTMAP, PATCH_STRIDE};
use bc2_render::world::World;

/// A level of the work folder (prepared) or of the game copy (Package), loaded; `None` (with a note) when absent.
fn world(opfor: &Path, lv: &str) -> Option<World> {
    let prepared = opfor.join("work").join("levels").join(lv).join("dist").is_dir();
    let package = files::package_stack(&opfor.join("game"), lv).is_some()
        && opfor.join("work").join("editor").join("levels").join(lv).is_dir();
    if !prepared && !package {
        println!("{lv}: skipped, neither prepared nor in the game copy");
        return None;
    }
    let (name, placed, weather) = common::placed(opfor, lv).ok()?;
    let w = if prepared {
        World::load(opfor, lv, &name, &placed, weather.as_deref())
    } else {
        World::load_package(opfor, lv, &name, &placed, weather.as_deref())
    };
    w.ok()
}

fn camera(v: [f64; 6]) -> (Camera, [f64; 3]) {
    let (yaw, pitch, d) = (v[3].to_radians(), v[4].to_radians(), v[5]);
    let eye = [
        v[0] + d * pitch.cos() * yaw.sin(),
        v[1] + d * pitch.sin(),
        v[2] + d * pitch.cos() * yaw.cos(),
    ];
    let target = [v[0], v[1], v[2]];
    (
        Camera::look(eye, target, 60f64.to_radians(), 0.5, 4000.0, 640.0 / 360.0),
        target,
    )
}

const SIZE: [u32; 2] = [640, 360];

fn render(dev: &mut Device, world: &mut World, v: [f64; 6], opts: Options) -> (Rendered, Live) {
    let (cam, target) = camera(v);
    let mut live = Live::new(&world.light);
    live.set(world.sources(target));
    live.build(&cam, SIZE, opts);
    let (depth, post) = world.passes(&cam, SIZE, true).unwrap();
    let raw = world.raw().clone();
    dev.render_live(&live, SIZE, world.clear(), depth.as_ref(), &post, &world.textures, &raw)
        .unwrap();
    (dev.read_scene(post.last()).unwrap(), live)
}

fn near_black(r: &Rendered) -> f64 {
    let n = r.depth.len();
    let k = (0..n)
        .filter(|&i| r.ldr[4 * i] < 8 && r.ldr[4 * i + 1] < 8 && r.ldr[4 * i + 2] < 8)
        .count();
    k as f64 / n as f64
}

// spec: TERR-VT-7
#[test]
#[ignore = "game tier: a level's terrain masks; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn mask_tiles_span_aligned_blocks_and_read_on_across_cells() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    let mut checked = 0;
    for lv in ["mp_009cq", "mp_005", "wake_island_s"] {
        let Some(w) = world(&opfor, lv) else { continue };
        let t = w.terrain.as_ref().expect("terrain");
        let m = &t.visual.maps[0];
        let side = m.side();
        // every tile lies in one block of flag x flag cells, aligned to the flag
        let mut blocks: HashMap<u16, (usize, usize, u8)> = HashMap::new();
        let (mut cells, mut shared) = (0, 0);
        for c in 0..side * side {
            let (px, pz) = (c % side, c / side);
            for (tile, f) in m.tiles[c].iter().zip(m.flags[c]) {
                if f == 0 {
                    continue;
                }
                cells += 1;
                assert!(f == 4 || f == 8, "{lv}: flag {f}");
                let b = (px / usize::from(f), pz / usize::from(f), f);
                let seen = *blocks.entry(*tile).or_insert(b);
                assert_eq!(
                    seen, b,
                    "{lv}: tile {tile} named outside its block at cell ({px}, {pz})"
                );
                shared += 1;
            }
        }
        println!("{lv}: {cells} tiled masks in {} tiles' blocks", blocks.len());
        assert!(shared > blocks.len(), "{lv}: tiles are shared by the cells of a block");
        // the drawn patches: two neighbours of one tile read the same atlas coordinates along their shared edge
        let q = t.samples_per_patch();
        let mut edges = 0;
        for c in 0..side * side {
            let (px, pz) = (c % side, c / side);
            if px + 1 >= side {
                continue;
            }
            let r = c + 1;
            let Some(k) = (0..4)
                .find(|&k| m.flags[c][k] != 0 && m.flags[c][k] == m.flags[r][k] && m.tiles[c][k] == m.tiles[r][k])
            else {
                continue;
            };
            let (Some(a), Some(b)) = (t.patch(px, pz), t.patch(px + 1, pz)) else {
                continue;
            };
            let coord = |p: &bc2_render::material::Prepared, i: usize, j: usize| {
                let at = (j * (q + 1) + i) * PATCH_STRIDE as usize + 4 + 4 * k;
                [
                    i16::from_le_bytes([p.vertices[at], p.vertices[at + 1]]),
                    i16::from_le_bytes([p.vertices[at + 2], p.vertices[at + 3]]),
                ]
            };
            for j in 0..=q {
                assert_eq!(
                    coord(&a, q, j),
                    coord(&b, 0, j),
                    "{lv}: cells ({px}, {pz}) and the next, row {j}"
                );
            }
            edges += 1;
        }
        println!("{lv}: {edges} shared cell edges read on");
        assert!(edges > 0);
        checked += 1;
    }
    if checked == 0 {
        println!("skipped: no level");
    }
}

// spec: RNDR-TERR-5, RNDR-WATER-2
#[test]
#[ignore = "game tier: Wake Island drawn; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn wake_island_draws_its_beach_and_lagoon() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    let mut checked = 0;
    // (level, the view: looking at the island from its lagoon, 90 m back; near-black share before R5d)
    for (lv, v, before) in [
        ("wake_island_s", [155.0, 214.5, 129.8, 225.0, 20.0, 90.0], 0.528),
        ("coral_sea", [-40.2, 120.7, -61.4, 225.0, 20.0, 90.0], 0.102),
    ] {
        let Some(mut w) = world(&opfor, lv) else { continue };
        let t = w.terrain.as_ref().expect("terrain");
        let range = t.hf.max[1] - t.hf.min[1];
        // the water reads the height map with the terrain's world scale
        let water = w.water.as_ref().expect("water");
        for d in &water.draws {
            if d.ps_reflect.resources.iter().any(|r| r.name == "texture_heightmap") {
                assert!(
                    d.textures.iter().any(|(_, n)| n == HEIGHTMAP),
                    "{lv}: {} binds the height map",
                    d.entry
                );
                assert!(
                    d.ps_extra.iter().any(|(_, v)| (v[0] - range).abs() < 1e-3),
                    "{lv}: {} reads heightmapWorldScaleY = {range}",
                    d.entry
                );
            }
        }
        let mut dev = match Device::new(false) {
            Ok(d) => d,
            Err(e) => {
                println!("skipped: {e}");
                return;
            }
        };
        let (r, _) = render(&mut dev, &mut w, v, Options::exact(true));
        let black = near_black(&r);
        println!(
            "{lv}: near-black {:.1} % of the view (before R5d: {:.1} %)",
            black * 100.0,
            before * 100.0
        );
        assert!(black < 0.02, "{lv}: {:.1} % near black", black * 100.0);
        checked += 1;
    }
    if checked == 0 {
        println!("skipped: no WW2 level");
    }
}

// spec: RNDR-SKY-1
#[test]
#[ignore = "game tier: MP_005 drawn; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn the_sky_dome_is_drawn_behind_the_level() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    let Some(mut w) = world(&opfor, "mp_005") else { return };
    let mut dev = match Device::new(false) {
        Ok(d) => d,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let v = [-90.0, 60.0, 300.0, 270.0, 15.0, 300.0];
    let (off, _) = render(&mut dev, &mut w, v, Options::exact(true));
    w.sky_pass = true;
    let (on, _) = render(&mut dev, &mut w, v, Options::exact(true));
    let bare = off.classes.iter().filter(|&&c| c == 0).count();
    let bare_on = on.classes.iter().filter(|&&c| c == 0).count();
    // the top row: sky only
    let row: Vec<&[f32]> = (0..SIZE[0] as usize).map(|x| &on.hdr[4 * x..4 * x + 3]).collect();
    let mean = |k: usize| row.iter().map(|p| f64::from(p[k])).sum::<f64>() / row.len() as f64;
    println!(
        "mp_005: background pixels {bare} without the sky, {bare_on} with it; the top row's mean ({:.3}, {:.3}, {:.3})",
        mean(0),
        mean(1),
        mean(2)
    );
    assert!(bare > 10_000, "the view sees the sky");
    assert_eq!(bare_on, 0, "the dome covers every background pixel");
    assert!(mean(2) > mean(0) * 1.2, "the sky above the horizon is blue");
    // the level itself is drawn as before: the sky lies behind it
    let level = (0..off.depth.len()).filter(|&i| off.classes[i] != 0).count();
    let same = (0..off.depth.len())
        .filter(|&i| off.classes[i] != 0 && off.depth[i].to_bits() == on.depth[i].to_bits())
        .count();
    assert_eq!(same, level, "the level's depth is unchanged");
}

// spec: RNDR-LOD-1, RNDR-VIEW-2
#[test]
#[ignore = "game tier: MP_005 drawn; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn levels_of_detail_draw_fewer_indices() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    let Some(mut w) = world(&opfor, "mp_005") else { return };
    let mut dev = match Device::new(false) {
        Ok(d) => d,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let with_lods = w.object_lods.iter().filter(|l| l.is_some()).count();
    assert!(with_lods > 100, "objects with levels of detail: {with_lods}");
    let v = [-90.0, 60.0, 300.0, 270.0, 15.0, 300.0];
    let indices = |live: &Live| -> (u64, u64) {
        let (mut patches, mut objects) = (0u64, 0u64);
        for s in &live.built.steps {
            let inst = &live.instances[s.inst as usize];
            let n = s
                .range
                .map_or(inst.source.prepared.draws[usize::from(s.draw)].index_count, |r| r.1);
            match inst.source.key {
                Key::Patch(..) => patches += u64::from(n),
                _ => objects += u64::from(n),
            }
        }
        (patches, objects)
    };
    let (full, lf) = render(&mut dev, &mut w, v, Options::exact(true));
    let (lod, ll) = render(&mut dev, &mut w, v, Options::default());
    let (fp, fo) = indices(&lf);
    let (lp, lo) = indices(&ll);
    let differ = (0..full.depth.len())
        .filter(|&i| full.ldr[4 * i..4 * i + 3] != lod.ldr[4 * i..4 * i + 3])
        .count();
    println!(
        "mp_005: terrain indices {fp} -> {lp}, object indices {fo} -> {lo}; {differ} of {} pixels differ",
        full.depth.len()
    );
    assert!(lp * 2 < fp, "the far patches are drawn coarser");
    assert!(lo < fo, "far objects are drawn at their further levels");
    assert!(differ * 5 < full.depth.len(), "the picture stays close");
}

#[test]
fn package_reads_through_the_bundle_cache_give_the_plain_bytes() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    let Some(stack) = files::package_stack(&opfor.join("game"), "coral_sea") else {
        println!("skipped: Coral Sea is not in the game copy");
        return;
    };
    let names = [
        "levels/coral_sea/shaders/dx11_single.res",
        "levels/coral_sea.dbx",
        "terrains/coral_sea/coral_sea.visual.res",
    ];
    let files = Files::bundles(&stack).unwrap();
    for n in names {
        assert!(files.read(n).is_none(), "{n} is not read before a fetch");
    }
    files.fetch(&names.map(str::to_owned)).unwrap();
    for n in names {
        let cached = files.read(n).expect(n);
        let k = stack
            .iter()
            .find(|p| {
                bc2_formats::fbrb::entries_path(p)
                    .is_ok_and(|h| h.entries.iter().any(|e| e.name.eq_ignore_ascii_case(n)))
            })
            .unwrap();
        let plain = bc2_formats::fbrb::read_path_uncached(k, &[n]).unwrap().remove(0);
        assert_eq!(cached, plain, "{n}");
    }
}
