//! Round E5-rest's terrain layers on DICE's data (game tier; `run_tests.py game` runs them):
//!
//! - `dice_paint_layers`: every DICE visual terrain of the game copy (each once) split into its layers
//!   (`terrainpaint::LayerModel`); on the terrains whose layers have one kind each, every aligned block of a patch map
//!   names one atlas tile per layer when a cell's tile slots are its tiled layers in the name's order, and not
//!   reversed (TEDIT-VT-2); the atlas tiles' border texels against their nearest inside texel (TEDIT-ATL-1); each
//!   level's material map names against its grid's terrain materials (MAT-GRID-2); MP_005's visual / material pairing
//!   (TEDIT-MM-2).
//! - `mp_005_paint_written`: on MP_005's own visual terrain, atlas and Dx9 database, every tiled layer painted over
//!   every patch shader's cells is classified (its combination the terrain's own, drawn by a shader with more tiled
//!   layers, cloned with its textures retargeted, or refused); three paints written and read back
//!   (`check_visual`), every byte outside the main patch map and the atlas tiles DICE's.
//! - `dice_colour_maps_written_in_place`: DICE's DXT1 / L8 colour / occlusion maps: nothing painted is DICE's bytes; a
//!   64 x 64 texel shade rewrites only the blocks under it and the mip texels below them; a re-shade after a 20 m pit
//!   darkens the texels around the pit only.
//! - `dice_rings_stitched_after_a_sculpt`: DICE's single-ring terrains: nothing edited gives DICE's main block and 12
//!   ring blocks byte for byte; a hill on the main block's edge and a dug ring block leave the ring stitched as DICE
//!   stitches it (TERR-HF-12); a widened range keeps the ground within a stored unit (TEDIT-HF-2).
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); skipped without one. Nothing is written but to the
//! process's memory, and nothing is loaded in game.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    missing_docs
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bc2_formats::terrain_codec::{MainBlock, MaterialMap, Region, SubBlock};
use bc2_formats::visual_terrain::VisualTerrain;
use bc2_formats::{fbrb, itexture};
use bc2_levelwrite::delta::Raster;
use bc2_levelwrite::terrainedit::{self, check_heights, rescale, write_heights_in, write_ring};
use bc2_levelwrite::terrainpaint::{
    self, Atlas, LayerModel, PaintLayer, check_colour, check_visual, colour_raster, write_colour,
};

const DICE_BEFORE: u64 = 1_356_998_400;

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

fn dice(p: &Path) -> bool {
    p.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|d| d.as_secs() < DICE_BEFORE)
}

fn bundles(game: &Path, name: &str) -> Vec<PathBuf> {
    let root = game.join("Dist").join("win32").join("levels");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.path().join(name))
        .filter(|p| p.is_file() && dice(p))
        .collect();
    out.sort();
    out
}

/// Every resource of DICE's `bundle`s whose name `keep` says, each name once: (bundle, name, data).
fn corpus(game: &Path, bundle: &str, keep: impl Fn(&str) -> bool) -> Vec<(PathBuf, String, Vec<u8>)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for b in bundles(game, bundle) {
        let h = fbrb::entries_path(&b).unwrap();
        let want: Vec<String> = h
            .entries
            .iter()
            .filter(|e| !e.is_deleted() && keep(&e.name.to_ascii_lowercase()))
            .filter(|e| seen.insert(e.name.to_ascii_lowercase()))
            .map(|e| e.name.clone())
            .collect();
        if want.is_empty() {
            continue;
        }
        for (n, d) in want.iter().zip(fbrb::read_path_uncached(&b, &want).unwrap()) {
            out.push((b.clone(), n.clone(), d));
        }
    }
    out
}

fn read(bundle: &Path, name: &str) -> Option<Vec<u8>> {
    let h = fbrb::entries_path(bundle).ok()?;
    let e = h
        .entries
        .iter()
        .find(|e| !e.is_deleted() && e.name.eq_ignore_ascii_case(name))?;
    fbrb::read_path_uncached(bundle, std::slice::from_ref(&e.name))
        .ok()?
        .pop()
}

#[test]
#[ignore = "game tier: every DICE visual terrain, atlas, material map and grid of the game copy"]
// spec: TEDIT-VT-2, TEDIT-ATL-1, TEDIT-MM-2, MAT-GRID-2
fn dice_paint_layers() {
    let _gate = crate::gate();
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let vts = corpus(&game, "terrain-00.fbrb", |n| n.ends_with(".visual.res"));
    assert!(vts.len() >= 20, "{}", vts.len());
    let (mut one_kind, mut slot_ok, mut slot_all, mut rev_ok, mut rev_all) = (0, 0, 0, 0, 0);
    let (mut border_same, mut border_all) = (0usize, 0usize);
    let mut refused = Vec::new();
    for (bundle, name, data) in &vts {
        let vt = VisualTerrain::decode(data).unwrap();
        let m = match LayerModel::of(&vt) {
            Ok(m) => m,
            Err(e) => {
                refused.push(format!("{name}: {e}"));
                continue;
            }
        };
        one_kind += 1;
        let map = &vt.maps[0];
        let w = map.header[4] as usize;
        for rev in [false, true] {
            let mut by: BTreeMap<(usize, usize, u8, usize), BTreeSet<u16>> = BTreeMap::new();
            for k in 0..w * w {
                let mut tl = m.tiled_layers(usize::from(map.shader[k]));
                if rev {
                    tl.reverse();
                }
                for (slot, l) in tl.into_iter().enumerate().take(4) {
                    let f = map.flags[k][slot];
                    if f > 0 {
                        let (x, z) = (k % w / usize::from(f), k / w / usize::from(f));
                        by.entry((x, z, f, l)).or_default().insert(map.tiles[k][slot]);
                    }
                }
            }
            let one = by.values().filter(|s| s.len() == 1).count();
            if rev {
                rev_ok += one;
                rev_all += by.len();
            } else {
                slot_ok += one;
                slot_all += by.len();
            }
        }
        // the atlas's borders: each border texel against the nearest inside texel
        let atlas_name = format!("{}.res", map.atlas);
        if let Some(ab) = read(bundle, &atlas_name)
            && let Ok(at) = Atlas::of(&ab, map.header)
        {
            let used: BTreeSet<u16> = map
                .tiles
                .iter()
                .zip(&map.flags)
                .flat_map(|(t, f)| t.iter().zip(f).filter(|(_, f)| **f > 0).map(|(t, _)| *t))
                .collect();
            let h = itexture::Header::read(&ab).unwrap();
            let (aw, per) = (h.width as usize, map.header[2] as usize);
            let px = |x: usize, y: usize| ab[itexture::HEADER + y * aw + x];
            for &t in &used {
                let (x0, y0) = ((t as usize % per) * 66, (t as usize / per) * 66);
                for i in 1..65 {
                    for (b, inside) in [
                        ((x0, y0 + i), (x0 + 1, y0 + i)),
                        ((x0 + 65, y0 + i), (x0 + 64, y0 + i)),
                        ((x0 + i, y0), (x0 + i, y0 + 1)),
                        ((x0 + i, y0 + 65), (x0 + i, y0 + 64)),
                    ] {
                        border_all += 1;
                        border_same += usize::from(px(b.0, b.1) == px(inside.0, inside.1));
                    }
                }
            }
            let _ = at.capacity();
        }
    }
    println!(
        "layers: {one_kind} of {} terrains have one kind per layer; refused: {refused:#?}",
        vts.len()
    );
    println!(
        "slot order (one tile per aligned block and layer): name order {slot_ok} of {slot_all}, reversed {rev_ok} of \
         {rev_all}"
    );
    println!(
        "atlas borders: {border_same} of {border_all} border texels equal their nearest inside texel ({:.1} %)",
        100.0 * border_same as f64 / border_all.max(1) as f64
    );
    assert!(one_kind >= 7, "{one_kind}");
    assert_eq!(slot_ok, slot_all, "name order: one tile per block and layer");
    assert!(rev_ok < rev_all, "reversed order breaks it");
    assert!(border_same * 10 >= border_all * 9);
    // the material maps' names against their levels' grids (MAT-GRID-2)
    let maps = corpus(&game, "terrain-00.fbrb", |n| n.ends_with("-materialmap.res"));
    let (mut held, mut names) = (0usize, 0usize);
    let mut not_held = BTreeSet::new();
    for (bundle, name, data) in &maps {
        let lv = bundle
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap();
        let level = bundle.with_file_name("level-00.fbrb");
        let Some(grid) = read(&level, &format!("levels/{lv}/materials_win32.dbx")) else {
            continue;
        };
        let have: BTreeSet<String> = terrainedit::grid_terrain_materials(&grid)
            .iter()
            .map(|n| n.to_ascii_lowercase())
            .collect();
        let m = MaterialMap::decode(data).unwrap();
        for n in &m.names {
            names += 1;
            if have.contains(&n.to_ascii_lowercase()) {
                held += 1;
            } else {
                not_held.insert(format!("{name}: {n}"));
            }
        }
    }
    println!(
        "material maps: {held} of {names} names are a terrain, default or construction material of their level's grid; not: {:?}",
        not_held.iter().take(12).collect::<Vec<_>>()
    );
    assert_eq!(held, names, "every material map name is its grid's");
    // MP_005's pairing
    let t = game
        .join("Dist")
        .join("win32")
        .join("levels")
        .join("mp_005")
        .join("terrain-00.fbrb");
    let v = read(&t, "Terrains/MP_005/MP_005.Visual.res").unwrap();
    let a = read(&t, "Terrains/MP_005/MP_005.MaskTileAtlas.res").unwrap();
    let mm = MaterialMap::decode(&read(&t, "Terrains/MP_005/MP_005-MaterialMap.res").unwrap()).unwrap();
    let pairs = terrainpaint::pairing(&v, &a, 2048, &mm).unwrap();
    for (l, p) in &pairs {
        println!(
            "MP_005 pairing: {l} -> {} ({:.0} % of {} strong-mask samples)",
            p.material,
            p.share * 100.0,
            p.samples
        );
    }
    assert_eq!(pairs.len(), 5, "{pairs:?}");
}

#[test]
#[ignore = "game tier: MP_005's visual terrain, atlas and Dx9 database"]
// spec: TEDIT-VT-1, TEDIT-SH-1, TEDIT-ATL-2
fn mp_005_paint_written() {
    let _gate = crate::gate();
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let lv = game.join("Dist").join("win32").join("levels").join("mp_005");
    let t = lv.join("terrain-00.fbrb");
    let v = read(&t, "Terrains/MP_005/MP_005.Visual.res").unwrap();
    let a = read(&t, "Terrains/MP_005/MP_005.MaskTileAtlas.res").unwrap();
    let db = read(&lv.join("level-00.fbrb"), "Levels/MP_005/shaders/Dx9_Single.res").unwrap();
    let textures = terrainpaint::entry_textures(&db).unwrap();
    let vt = VisualTerrain::decode(&v).unwrap();
    let m = LayerModel::of(&vt).unwrap();
    println!(
        "MP_005: {} layers ({:?}), tiled {:?}, {} patch shaders, {} entry texture sets",
        m.layers.len(),
        m.layers,
        m.paintable(),
        m.shaders.len(),
        textures.len()
    );
    let own = terrainpaint::layer_textures(&m, &textures);
    for (l, o) in m.layers.iter().zip(&own) {
        println!("  {l}: own textures {o:?}");
    }
    // every tiled layer over every shader's combination: how a paint draws it
    let (mut exact, mut padded, mut cloned, mut none) = (0, 0, 0, 0);
    for (s, ls) in m.shaders.iter().enumerate() {
        for l in m.paintable() {
            let li = m.index(l).unwrap();
            if ls.contains(&li) {
                continue;
            }
            let mut want: BTreeSet<usize> = ls.iter().copied().collect();
            want.insert(li);
            if m.find(&want).is_some() {
                exact += 1;
            } else if m.find_superset(&want).is_some() {
                padded += 1;
            } else {
                // a paint of l on a cell of shader s, through the real writer
                let k = vt.maps[0].shader.iter().position(|x| usize::from(*x) == s);
                let Some(k) = k else {
                    continue;
                };
                let w = vt.maps[0].header[4] as usize;
                let mut r = Raster::new(2048, 2048, 0);
                let (cx, cz) = ((k % w) * 16 + 8, (k / w) * 16 + 8);
                r.set(cx as u32, cz as u32, 255);
                match terrainpaint::paint(&v, &a, 2048, &[PaintLayer { layer: l, weights: &r }], Some(&textures)) {
                    Ok(p) if !p.added.is_empty() => cloned += 1,
                    _ => {
                        none += 1;
                        println!("  refused: {} + {l}", vt.shaders[s].name);
                    }
                }
            }
        }
    }
    println!(
        "MP_005: a tiled layer painted on each patch shader's cells: {exact} combinations the terrain's own, {padded} \
         drawn by a shader with more tiled layers, {cloned} cloned with textures retargeted, {none} refused"
    );
    // three paints written: DryGrass over open dunes, Asphalt beside it, Trash over both
    let disc = |cx: u32, cz: u32, r: f64| {
        let mut w = Raster::new(2048, 2048, 0);
        for j in cz.saturating_sub(r as u32)..(cz + r as u32).min(2047) {
            for i in cx.saturating_sub(r as u32)..(cx + r as u32).min(2047) {
                if f64::from(i as i32 - cx as i32).hypot(f64::from(j as i32 - cz as i32)) < r {
                    w.set(i, j, 255);
                }
            }
        }
        w
    };
    let (g, asph, trash) = (disc(1100, 1000, 40.0), disc(1180, 1000, 30.0), disc(1140, 1000, 25.0));
    let layers = [
        PaintLayer {
            layer: "DryGrass",
            weights: &g,
        },
        PaintLayer {
            layer: "Asphalt",
            weights: &asph,
        },
        PaintLayer {
            layer: "Trash",
            weights: &trash,
        },
    ];
    let t0 = std::time::Instant::now();
    let p = terrainpaint::paint(&v, &a, 2048, &layers, Some(&textures)).unwrap();
    println!("painted in {:.2} s: {:?}", t0.elapsed().as_secs_f64(), p.notes);
    assert_eq!(check_visual(&v, &p.visual, &a, &p.atlas), Vec::<String>::new());
    let pv = VisualTerrain::decode(&p.visual).unwrap();
    assert_eq!(pv.roads, vt.roads, "the road blocks DICE's");
    assert_eq!(
        &pv.shaders[..vt.shaders.len()],
        &vt.shaders[..],
        "DICE's patch shaders kept"
    );
    let changed = pv.maps[0]
        .shader
        .iter()
        .zip(&vt.maps[0].shader)
        .filter(|(x, y)| x != y)
        .count();
    println!(
        "{changed} cells draw another patch shader, {} cells rewritten, {} tiles",
        p.cells, p.tiles
    );
    assert!(changed > 0 && p.tiles > 0);
    // the atlas: only the tiles the paint wrote differ
    let at = Atlas::of(&a, vt.maps[0].header).unwrap();
    let span = at.span();
    let differ = (0..at.capacity())
        .filter(|&t| {
            (0..span * span).any(|i| {
                Atlas::of(&p.atlas, vt.maps[0].header)
                    .unwrap()
                    .get(t, i % span, i / span)
                    != at.get(t, i % span, i / span)
            })
        })
        .count();
    println!("atlas: {differ} tiles differ from DICE's ({} written)", p.tiles);
    assert!(differ <= p.tiles);
    // nothing painted: DICE's bytes
    let none_r = Raster::new(2048, 2048, 0);
    let p0 = terrainpaint::paint(
        &v,
        &a,
        2048,
        &[PaintLayer {
            layer: "DryGrass",
            weights: &none_r,
        }],
        None,
    )
    .unwrap();
    assert!(p0.visual == v && p0.atlas == a);
}

#[test]
#[ignore = "game tier: DICE's colour / occlusion maps"]
// spec: TEDIT-OCC-1, LIGHT-BAKE-6
fn dice_colour_maps_written_in_place() {
    let _gate = crate::gate();
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let maps = corpus(&game, "level-00.fbrb", |n| {
        n.starts_with("terrains/") && n.contains("/textures/") && (n.contains("occlusion") || n.ends_with("occ.res"))
    });
    let mut checked = 0;
    for (_, name, data) in &maps {
        let Some(r) = colour_raster(data) else {
            println!("{name}: not DXT1 / L8 (left)");
            continue;
        };
        checked += 1;
        assert!(
            write_colour(data, &r).unwrap() == *data,
            "{name}: nothing painted, DICE's bytes"
        );
        let grey = terrainpaint::colour_is_grey(data);
        let mut s = r.clone();
        let (cx, cz) = (r.width() / 2, r.height() / 2);
        for z in cz..cz + 64 {
            for x in cx..cx + 64 {
                let c = terrainpaint::colour_of(s.get(x, z), grey);
                s.set(x, z, terrainpaint::sample_of(c.map(|v| v / 2), grey));
            }
        }
        let out = write_colour(data, &s).unwrap();
        assert!(check_colour(data, &out).is_empty(), "{name}");
        let h = itexture::Header::read(data).unwrap();
        let l0 = h.level_start(0)..h.level_start(1);
        let changed0 = out[l0.clone()].iter().zip(&data[l0]).filter(|(a, b)| a != b).count();
        let rest = out[h.level_start(1)..]
            .iter()
            .zip(&data[h.level_start(1)..])
            .filter(|(a, b)| a != b)
            .count();
        let most0 = if grey { 64 * 64 } else { 16 * 16 * 8 };
        assert!(changed0 > 0 && changed0 <= most0, "{name}: {changed0}");
        assert!(rest <= most0, "{name}: {rest} mip bytes");
        // a re-shade after a 20 m pit at the map's centre: darker around it, nothing far away
        let flat = |_: f64, _: f64| 100.0;
        let pit = |x: f64, z: f64| if x.hypot(z) < 25.0 { 80.0 } else { 100.0 };
        let (shaded, done) = terrainpaint::reshade(&r, grey, &flat, &pit, [-90.0, -90.0, 90.0, 90.0]);
        assert!(done.texels > 0 && done.mean_ratio < 1.0, "{name}: {done:?}");
        let changed = r
            .samples()
            .iter()
            .zip(shaded.samples())
            .filter(|(a, b)| **a != *b)
            .count();
        println!(
            "{name}: {} x {} {}, a 64 x 64 shade rewrote {changed0} level-0 bytes and {rest} mip bytes; the pit's \
             re-shade {changed} texels (mean ratio {:.3})",
            r.width(),
            r.height(),
            if grey { "L8" } else { "DXT1" },
            done.mean_ratio
        );
    }
    assert!(checked >= 12, "{checked}");
}

#[test]
#[ignore = "game tier: DICE's single-ring terrains"]
// spec: TERR-HF-12, TEDIT-HF-2
fn dice_rings_stitched_after_a_sculpt() {
    let _gate = crate::gate();
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let mains = corpus(&game, "terrain-00.fbrb", |n| n.ends_with(".heightfield-000.res"));
    let mut rings = 0;
    for (bundle, name, data) in &mains {
        let stem = name.strip_suffix("-000.res").unwrap();
        let ring: Vec<Vec<u8>> = (1..=13)
            .map_while(|k| read(bundle, &format!("{stem}-{k:03}.res")))
            .collect();
        if ring.len() != 12 {
            continue;
        }
        rings += 1;
        let (m2, r2) = write_ring(data, data, &ring, None).unwrap();
        assert!(m2 == *data && r2 == ring, "{name}: nothing edited, DICE's bytes");
        let mut m = MainBlock::decode(data).unwrap();
        let n = m.size as usize;
        // a 30 m raise along the west edge
        let up = (30.0 * 65535.0 / f64::from(m.max[1] - m.min[1])) as u16;
        for z in n / 2 - 100..n / 2 + 100 {
            for x in 0..40 {
                m.heights[z * n + x] = m.heights[z * n + x].saturating_add(up);
            }
        }
        m.rebuild(Region {
            x0: 0,
            z0: n / 2 - 101,
            x1: 41,
            z1: n / 2 + 101,
        });
        let edited = m.encode();
        let (m3, r3) = write_ring(data, &edited, &ring, None).unwrap();
        let mb = MainBlock::decode(&m3).unwrap();
        let blocks: Vec<Vec<u16>> = r3.iter().map(|b| SubBlock::decode(b).unwrap().heights).collect();
        let s = bc2_terragen::ring::stitching(&mb.heights, n, &blocks);
        let moved = r3.iter().zip(&ring).filter(|(a, b)| a != b).count();
        println!(
            "{name}: n {n}; a raise on its west edge: {moved} of 12 ring blocks rewritten, seams {} / {}, ring seams \
             {} / {}, edges {:.2} units off straight",
            s.main_seams.0, s.main_seams.1, s.ring_seams.0, s.ring_seams.1, s.straight
        );
        assert_eq!(s.main_seams.0, s.main_seams.1, "{name}");
        assert_eq!(s.ring_seams.0, s.ring_seams.1, "{name}");
        assert!(s.straight <= 0.5, "{name}");
        assert!(
            check_heights(data, &m3).is_empty(),
            "{name}: {:?}",
            check_heights(data, &m3)
        );
        // the range widened by half: every sample within a stored unit of its height
        let base = MainBlock::decode(data).unwrap();
        let (from, to) = (f64::from(base.max[1]), f64::from(base.max[1]) * 1.5);
        let wide = write_heights_in(
            data,
            &rescale(&Raster::from_samples(base.size, base.size, &base.heights), from, to),
            Some(to as f32),
        )
        .unwrap();
        let wb = MainBlock::decode(&wide).unwrap();
        let worst = wb
            .heights
            .iter()
            .zip(&base.heights)
            .map(|(a, b)| (f64::from(*a) * to - f64::from(*b) * from).abs() / 65535.0)
            .fold(0.0, f64::max);
        assert!(worst <= to / 65535.0, "{name}: {worst} m");
        assert!(
            check_heights(data, &wide).is_empty(),
            "{name}: {:?}",
            check_heights(data, &wide)
        );
        let (_, r4) = write_ring(data, &wide, &ring, None).unwrap();
        let b4 = SubBlock::decode(&r4[0]).unwrap();
        assert!(
            (f64::from(b4.max[1]) - to).abs() < 0.01,
            "{name}: the ring takes the range"
        );
        println!("{name}: range {from} -> {to} m, worst {worst:.4} m off");
    }
    assert!(rings >= 6, "{rings}");
}
