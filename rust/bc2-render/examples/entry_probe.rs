//! One shader database entry of a level, as the database holds it (round R5d's diagnosis tool): its programs (key
//! bytes, layout, state), each program's vertex and pixel shader reflection, the blocks' constants, textures and
//! engine textures; `PROBE_DUMP=<folder>` writes the shaders' DXBC there.
//!
//! `cargo run --release -p bc2-render --example entry_probe -- <level> <entry substring>`
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

use bc2_formats::shaderdb::query;
use bc2_render::dxbc;
use bc2_render::files::{Files, level_folders, package_stack};
use bc2_render::material::Db;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opfor = common::opfor();
    let lv = a[1].to_lowercase();
    let want = a[2].to_lowercase();
    let files = if opfor.join("work").join("levels").join(&lv).is_dir() {
        Files::new(&level_folders(&opfor, &lv))
    } else {
        Files::bundles(&package_stack(&opfor.join("game"), &lv).expect("a package level")).expect("bundles")
    };
    let name = format!("levels/{lv}/shaders/dx11_single.res");
    let data = files.read(&name).or_else(|| {
        files.fetch(&files.missed()).ok()?;
        files.read(&name)
    });
    let db = Db::parse(&data.expect("database")).unwrap();
    let s = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let mut names: Vec<String> = db.entries.keys().map(|k| s(k)).filter(|k| k.contains(&want)).collect();
    names.sort();
    for e in names {
        let progs = db.entry_programs(&e).unwrap_or_default();
        println!("entry {e}: {} programs", progs.len());
        for &p in progs {
            let pr = &db.programs[p];
            let key: Vec<u8> = (0x30..0x3c).map(|b| pr.key_byte(b)).collect();
            println!(
                "  program {p}: key 30.. {key:?} state {:#x} ref {} vs elements {:?}",
                pr.state.0,
                pr.state.1,
                bc2_render::material::vs_elements(&db.vs[pr.vs].tail)
                    .iter()
                    .map(|e| (e.semantic, e.format, e.slot, e.offset, e.instance))
                    .collect::<Vec<_>>()
            );
            for (what, code, param) in [
                ("vs", &db.vs[pr.vs].code, db.vs[pr.vs].param),
                ("ps", &db.ps[pr.ps].code, db.ps[pr.ps].param),
            ] {
                let r = dxbc::reflect(code);
                println!(
                    "    {what} {} bytes: globals {:?}",
                    code.len(),
                    r.globals
                        .iter()
                        .map(|g| (g.name.clone(), g.offset, g.size))
                        .collect::<Vec<_>>()
                );
                println!(
                    "      resources {:?}",
                    r.resources
                        .iter()
                        .map(|x| (x.name.clone(), x.kind, x.point))
                        .collect::<Vec<_>>()
                );
                let cs = query::block_constants(db.param(param), false);
                println!(
                    "      own block constants {:?}",
                    cs.iter().map(|c| (s(c.name), c.register, c.values)).collect::<Vec<_>>()
                );
                if let Some(dir) = std::env::var_os("PROBE_DUMP") {
                    let f = e.replace('/', "_");
                    std::fs::write(
                        std::path::PathBuf::from(dir).join(format!("{f}_{p}_{what}.cso")),
                        code.as_slice(),
                    )
                    .unwrap();
                }
            }
            for (what, b) in [("block1", pr.block1), ("block2", pr.block2)] {
                let blk = db.param(b);
                println!(
                    "    {what}: constants {:?} textures {:?} engine {:?}",
                    query::block_constants(blk, false)
                        .iter()
                        .map(|c| (s(c.name), c.register, c.values))
                        .collect::<Vec<_>>(),
                    query::block_textures(blk, false)
                        .iter()
                        .map(|t| (t.sampler, s(t.name)))
                        .collect::<Vec<_>>(),
                    query::block_engine_textures(blk, false)
                        .iter()
                        .map(|(n, k)| (s(n), *k))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}
