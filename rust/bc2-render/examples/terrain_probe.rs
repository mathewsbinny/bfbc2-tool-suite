//! What a level's terrain holds (round R5d's diagnosis of Wake Island's black cells): the visual terrain's patch
//! map header, its patch shaders (layers, tiled masks), the cells' mask tiles and flags, the mask atlas, and each
//! patch shader's main program with the textures it binds.
//!
//! `cargo run --release -p bc2-render --example terrain_probe -- <level> [<level> ...]`
#![allow(
    clippy::print_stdout,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::chunks_exact_to_as_chunks,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    missing_docs
)]

mod common;

use std::collections::BTreeMap;

use bc2_render::files::{Files, level_folders, package_stack};
use bc2_render::material::{Db, TextureFiles};
use bc2_render::terrain::{Terrain, parse_visual};
use bc2_render::u32_at;

fn files_of(opfor: &std::path::Path, lv: &str) -> Files {
    if opfor.join("work").join("levels").join(lv).is_dir() {
        Files::new(&level_folders(opfor, lv))
    } else {
        Files::bundles(&package_stack(&opfor.join("game"), lv).expect("a package level")).expect("bundles")
    }
}

fn read(files: &Files, name: &str) -> Option<Vec<u8>> {
    files.read(name).or_else(|| {
        let m = files.missed();
        if m.is_empty() {
            return None;
        }
        files.fetch(&m).ok()?;
        files.read(name)
    })
}

fn main() {
    let opfor = common::opfor();
    for lv in std::env::args().skip(1) {
        let lv = lv.to_lowercase();
        let files = files_of(&opfor, &lv);
        let vt = files
            .names()
            .filter(|n| n.starts_with("terrains/") && n.ends_with(".visual.res"))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        println!("== {lv}: visual terrains {vt:?}");
        let Some(name) = vt.first() else { continue };
        let data = read(&files, name).expect("visual terrain");
        let v = parse_visual(&data).expect("parses");
        for (k, s) in v.shaders.iter().enumerate() {
            println!("  shader {k}: {} layers {} tiled {}", s.name, s.layers, s.tiled);
        }
        for (b, m) in v.maps.iter().enumerate() {
            println!(
                "  map {b}: header {:?} side {} shader side {} hf {} atlas {}",
                m.header,
                m.side(),
                m.shader_side,
                m.heightfield,
                m.atlas
            );
            let mut flags: BTreeMap<[u8; 4], usize> = BTreeMap::new();
            let mut max_tile = 0u16;
            let mut mismatch = 0;
            let ratio = (m.shader_side / m.side().max(1)).max(1);
            for c in 0..m.side() * m.side() {
                *flags.entry(m.flags[c]).or_default() += 1;
                let (pz, px) = (c / m.side(), c % m.side());
                let sh = m.shader[pz * ratio * m.shader_side + px * ratio];
                let tiled = v.shaders.get(usize::from(sh)).map_or(0, |s| s.tiled);
                let nz = m.flags[c].iter().filter(|&&f| f != 0).count() as u32;
                if nz != tiled {
                    mismatch += 1;
                }
                for (t, f) in m.tiles[c].iter().zip(m.flags[c]) {
                    if f != 0 {
                        max_tile = max_tile.max(*t);
                    }
                }
            }
            println!("    flags {flags:?}");
            println!(
                "    max tile used {max_tile}; cells whose non-zero flags differ from their shader's tiled count: {mismatch}"
            );
            if let Some(a) = read(&files, &format!("{}.res", m.atlas)) {
                println!(
                    "    atlas bytes {} header w {} h {} fmt {}",
                    a.len(),
                    u32_at(&a, 16),
                    u32_at(&a, 20),
                    u32_at(&a, 8)
                );
            }
            let mut sh_count: BTreeMap<u8, usize> = BTreeMap::new();
            for s in &m.shader {
                *sh_count.entry(*s).or_default() += 1;
            }
            println!("    shader cells {sh_count:?}");
        }
        let dbname = format!("levels/{lv}/shaders/dx11_single.res");
        let Some(dbb) = read(&files, &dbname) else {
            println!("  no database");
            continue;
        };
        let db = Db::parse(&dbb).unwrap();
        let mut cache = TextureFiles::new();
        let tname = name
            .trim_start_matches("terrains/")
            .split('/')
            .next()
            .unwrap()
            .to_owned();
        // fetch until the terrain loads with every file read
        for _ in 0..6 {
            let _ = Terrain::load(
                &files,
                &db,
                &tname,
                "Textures/Sky/Sky_Beach_Generic_RGBA16f",
                &mut cache,
            );
            let m = files.missed();
            if m.is_empty() {
                break;
            }
            files.fetch(&m).ok();
            cache = TextureFiles::new();
        }
        let t = Terrain::load(
            &files,
            &db,
            &tname,
            "Textures/Sky/Sky_Beach_Generic_RGBA16f",
            &mut cache,
        )
        .unwrap();
        println!("  atlas size {:?}", t.atlas_size);
        // `PROBE_HEIGHT=<water y>`: per patch shader, its cells' height range and how many cells rise above the water
        if let Some(wy) = std::env::var("PROBE_HEIGHT").ok().and_then(|v| v.parse::<f32>().ok()) {
            let m = &t.visual.maps[0];
            let side = m.side();
            let q = t.samples_per_patch();
            let mut per: BTreeMap<u8, (usize, usize, f32, f32)> = BTreeMap::new();
            for pz in 0..side {
                for px in 0..side {
                    let sh = m.shader[pz * m.shader_side / side * m.shader_side + px * m.shader_side / side];
                    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                    for j in pz * q..=((pz + 1) * q).min(t.hf.size as usize - 1) {
                        for i in px * q..=((px + 1) * q).min(t.hf.size as usize - 1) {
                            if let Some(h) = t.hf.sample(i, j) {
                                lo = lo.min(h);
                                hi = hi.max(h);
                            }
                        }
                    }
                    let e = per.entry(sh).or_insert((0, 0, f32::MAX, f32::MIN));
                    e.0 += 1;
                    if hi > wy {
                        e.1 += 1;
                        if sh == 0 {
                            println!("    shader 0 cell ({px}, {pz}) rises to {hi:.2} (lowest {lo:.2})");
                        }
                    }
                    e.2 = e.2.min(lo);
                    e.3 = e.3.max(hi);
                }
            }
            for (sh, (n, above, lo, hi)) in per {
                println!("  shader {sh}: {n} cells, {above} rise above {wy}; heights {lo:.1} .. {hi:.1}");
            }
        }
        // `PROBE_CELLS=px,pz,w,h`: the cells' first mask tile and flag (rows of z, from the top)
        if let Ok(c) = std::env::var("PROBE_CELLS") {
            let v: Vec<usize> = c.split(',').map(|s| s.parse().unwrap()).collect();
            let m = &t.visual.maps[0];
            for pz in (v[1]..v[1] + v[3]).rev() {
                let row: Vec<String> = (v[0]..v[0] + v[2])
                    .map(|px| {
                        let c = pz * m.side() + px;
                        format!("{:>3}/{}{}", m.tiles[c][0], m.flags[c][0], m.flags[c][1])
                    })
                    .collect();
                println!("  z {pz:>3}: {}", row.join(" "));
            }
        }
        if let Ok(which) = std::env::var("PROBE_ENTRY") {
            for k in which.split(',').map(|s| s.parse::<usize>().unwrap()) {
                let s = &t.visual.shaders[k];
                let progs = db.entry_programs(&s.name).unwrap_or_default();
                println!("  entry {k} {}: {} programs", s.name, progs.len());
                for &p in progs {
                    let pr = &db.programs[p];
                    let key: Vec<u8> = (0x30..0x3c).map(|b| pr.key_byte(b)).collect();
                    println!(
                        "    program {p}: key 30.. {key:?} layout {:?} ps {} bytes vs {} bytes state {:#x}",
                        pr.layout(),
                        db.ps[pr.ps].code.len(),
                        db.vs[pr.vs].code.len(),
                        pr.state.0
                    );
                }
            }
        }
        for (k, d) in t.draws.iter().enumerate() {
            match d {
                Ok(d) => {
                    println!(
                        "  draw {k}: program {} elements {:?} textures {:?} vs_textures {:?}",
                        d.program,
                        d.elements
                            .iter()
                            .map(|e| (e.semantic, e.format, e.offset))
                            .collect::<Vec<_>>(),
                        d.textures,
                        d.vs_textures
                    );
                    if std::env::var_os("PROBE_CONSTS").is_some() {
                        let pr = &db.programs[d.program];
                        for (what, b) in [
                            ("block2", pr.block2),
                            ("ps param", db.ps[pr.ps].param),
                            ("vs param", db.vs[pr.vs].param),
                        ] {
                            let cs = bc2_formats::shaderdb::query::block_constants(db.param(b), false);
                            println!(
                                "    {what}: {:?}",
                                cs.iter()
                                    .map(|c| (String::from_utf8_lossy(c.name).into_owned(), c.register, c.values))
                                    .collect::<Vec<_>>()
                            );
                        }
                    }
                    if std::env::var_os("PROBE_RES").is_some() {
                        println!(
                            "    ps resources {:?}",
                            d.ps_reflect
                                .resources
                                .iter()
                                .map(|r| (r.name.clone(), r.kind, r.point))
                                .collect::<Vec<_>>()
                        );
                        println!("    state {:?} class {} ps bytes {}", d.state, d.class, d.ps.len());
                    }
                    if let Some(dir) = std::env::var_os("PROBE_DUMP") {
                        let dir = std::path::PathBuf::from(dir);
                        std::fs::write(dir.join(format!("{lv}_{k}_ps.cso")), d.ps.as_slice()).unwrap();
                        std::fs::write(dir.join(format!("{lv}_{k}_vs.cso")), d.vs.as_slice()).unwrap();
                    }
                }
                Err(e) => println!("  draw {k}: {e}"),
            }
        }
    }
}
