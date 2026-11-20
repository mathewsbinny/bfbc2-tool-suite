//! What the editor's simplified view reads of a prepared level's objects (round SV1): its render meshes as plain
//! geometry, which parts the game alpha-tests, their diffuse textures and tints, and the blueprints that name no
//! mesh (drawn as nothing: invisible walls, triggers, volumes).
//!
//! `cargo run -p bc2-render --release --example simple_meshes -- <level> [--parts]` (`OPFOR` names Project OPFOR's
//! folder).
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::cast_precision_loss, missing_docs)]

mod common;

use std::collections::BTreeMap;

use bc2_render::simple::{Shape, Simple};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    // `--dbx <file>`: a partition as XML (what a blueprint says of its parts and health states)
    if a.get(1).is_some_and(|x| x == "--dbx") {
        let data = std::fs::read(a.get(2).unwrap()).unwrap();
        println!("{}", bc2_formats::dbx::to_xml(&data).unwrap());
        return;
    }
    let lv = a.get(1).map_or("mp_005", String::as_str);
    let parts = a.iter().any(|x| x == "--parts");
    let opfor = common::opfor();
    let (name, placed, _) = common::placed(&opfor, lv).unwrap();
    let s = Simple::load(&opfor, lv, &name, &placed, false).unwrap();
    let indices: usize = s.meshes.iter().map(|m| m.index_count()).sum();
    let vertices: usize = s.meshes.iter().map(|m| m.positions.len()).sum();
    let textured = s
        .meshes
        .iter()
        .flat_map(|m| &m.parts)
        .filter(|p| p.texture.is_some())
        .count();
    let all_parts = s.meshes.iter().flat_map(|m| &m.parts).count();
    let tested = s.meshes.iter().flat_map(|m| &m.parts).filter(|p| p.tested).count();
    println!(
        "{lv}: {} placed, {} meshes ({vertices} vertices, {} triangles), {all_parts} parts ({tested} alpha-tested, \
         {textured} with a texture), read in {:.2} s; {} notes",
        placed.len(),
        s.meshes.len(),
        indices / 3,
        s.seconds,
        s.notes.len()
    );
    let mut nothing: BTreeMap<&str, usize> = BTreeMap::new();
    let mut unknown: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &placed {
        match s.shape(p) {
            Some(Shape::Nothing) => *nothing.entry(p.blueprint.as_str()).or_default() += 1,
            None => *unknown.entry(p.blueprint.as_str()).or_default() += 1,
            Some(Shape::Mesh(_)) => {}
        }
    }
    println!("drawn as nothing ({} blueprints):", nothing.len());
    for (b, n) in &nothing {
        println!("  {n:5}  {b}");
    }
    println!("not read ({} blueprints):", unknown.len());
    for (b, n) in unknown.iter().take(40) {
        println!("  {n:5}  {b}");
    }
    for n in s.notes.iter().take(20) {
        println!("note: {n}");
    }
    let with_lods = s.meshes.iter().filter(|m| !m.lods.is_empty()).count();
    println!(
        "{with_lods} of {} meshes have more than one level of detail",
        s.meshes.len()
    );
    // `--textures <part of a mesh name>`: every texture each draw of those meshes binds, by slot; its LODs' sizes
    if let Some(want) = a.iter().position(|x| x == "--textures").and_then(|k| a.get(k + 1)) {
        let files = bc2_render::files::Files::new(&bc2_render::files::level_folders(&opfor, lv));
        let db = bc2_render::material::Db::parse(&files.read(&format!("levels/{lv}/shaders/dx11_single.res")).unwrap())
            .unwrap();
        for m in s
            .meshes
            .iter()
            .filter(|m| m.mesh.to_lowercase().contains(want.as_str()))
        {
            let pr = bc2_render::material::prepare_untextured(&files, &db, &m.mesh, None, 0);
            for d in &pr.draws {
                println!("{} | {} | {:?}", m.mesh, d.part, d.textures);
            }
            let sizes: Vec<usize> = std::iter::once(m.index_count())
                .chain(m.lods.iter().map(|l| l.index_count()))
                .collect();
            println!("{}: indices by LOD {sizes:?}", m.mesh);
        }
    }
    if parts {
        for m in &s.meshes {
            for p in &m.parts {
                println!(
                    "{} | {} | tested {} blended {} ref {} | {:?} | tint {:?}",
                    m.mesh, p.name, p.tested, p.blended, p.alpha_ref, p.texture_name, p.tint
                );
            }
        }
    }
}
