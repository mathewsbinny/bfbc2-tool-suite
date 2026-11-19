//! What the examples and tests share: a prepared level's placed objects and group members, read with `bc2-scene` as
//! the editor reads them (`bc2-editor`'s `levels::load_options`: the level partition, Package before Dist; each group
//! file once, Package first), as [`Placed`]s; and the C++ harness to compare with.
#![allow(dead_code, reason = "each example uses part of it")]

use std::fs;
use std::path::{Path, PathBuf};

use bc2_render::world::Placed;
use bc2_scene::scene::{self, LoadOptions, Source};

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        fs::read_dir(dir).map_or_else(|_| Vec::new(), |r| r.flatten().map(|e| e.path()).collect());
    v.sort();
    v
}

fn lower_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_lowercase())
}

/// A prepared level's partition name (`MP_005`), its placed things and its weather folder.
///
/// # Errors
/// The level is not prepared or its partition does not load.
pub fn placed(opfor: &Path, lv: &str) -> Result<(String, Vec<Placed>, Option<String>), String> {
    let lv = lv.to_lowercase();
    let base = opfor.join("work").join("levels").join(&lv);
    let mut bases = vec![base.join("pkg"), base.join("dist")];
    bases.extend(
        entries(&opfor.join("work").join("navkit").join("common"))
            .into_iter()
            .filter(|d| d.is_dir()),
    );
    let partition = bases
        .iter()
        .take(2)
        .flat_map(|b| entries(&b.join("levels")))
        .find(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dbx"))
                && p.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == lv)
        })
        .ok_or_else(|| format!("{lv}: level partition not found"))?;
    let name = partition
        .file_stem()
        .map_or_else(|| lv.to_uppercase(), |s| s.to_string_lossy().into_owned());
    let mut groups: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for b in bases.iter().take(2) {
        for g in entries(&b.join("levels").join(&lv)) {
            let n = lower_name(&g);
            if n.ends_with("_havokasset.dbx") && !seen.contains(&n) {
                seen.push(n);
                groups.push(g);
            }
        }
    }
    let mut options = LoadOptions::new(name.clone(), partition, bases);
    options.groups = groups;
    options.geometry = true;
    // the editor's mesh-alias cache (`bc2-editor`'s `levels::alias_cache`), when it is there
    options.alias_cache = std::env::var_os("LOCALAPPDATA")
        .map(|b| {
            PathBuf::from(b)
                .join("bfbc2-tool-suite")
                .join("cache")
                .join("collision")
        })
        .filter(|p| p.is_dir());
    let s = scene::load(&options).map_err(|e| e.to_string())?;
    let weather = s
        .entities
        .iter()
        .filter_map(|e| e.blueprint.as_deref())
        .find_map(|b| b.find("/weathersystementity").and_then(|i| b.get(..i)))
        .map(str::to_owned);
    let placed = s
        .entities
        .iter()
        .filter_map(|e| {
            Some(Placed {
                guid: e.guid.clone(),
                blueprint: e.blueprint.clone()?,
                rows: e.matrix?,
                group: match e.source {
                    Source::GroupMember { item, .. } => Some((e.group.clone()?, item)),
                    Source::Instance { .. } => None,
                },
            })
        })
        .collect();
    Ok((name, placed, weather))
}

/// The C++ harness: `SHADERVIEW`, else the suite's `build\shaderview.exe` or `bfbc2-shader-tools\native\shaderview.exe`
/// (of this checkout, then of the main checkout beside it).
#[must_use]
pub fn shaderview() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SHADERVIEW") {
        return Some(PathBuf::from(p));
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let main = here.join("..").join("bfbc2-tool-suite");
    [here, main]
        .iter()
        .flat_map(|r| {
            [
                r.join("build").join("shaderview.exe"),
                r.join("bfbc2-shader-tools").join("native").join("shaderview.exe"),
            ]
        })
        .find(|p| p.is_file())
}

/// OPFOR's folder: `OPFOR`, else the default checkout.
#[must_use]
pub fn opfor() -> PathBuf {
    std::env::var_os("OPFOR").map_or_else(
        || PathBuf::from(r"C:\Programming\Projects\project-opfor-bfbc2"),
        PathBuf::from,
    )
}

/// Writes RGBA8 as a PNG.
///
/// # Errors
/// The file cannot be written.
pub fn save_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .map_err(|e| e.to_string())
}
