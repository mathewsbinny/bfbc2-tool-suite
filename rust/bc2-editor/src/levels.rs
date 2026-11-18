//! Finding Project OPFOR BFBC2's game copy, its levels, and a level's files, as the suite's Python finds them
//! (`core/bfbc2core/paths.find_opfor`, NavKit's `Project.level_dirs`, `level.load`), without writing anything.
//!
//! The editor only reads: a level NavKit has not prepared yet (`work\levels\<lv>\dist`) is listed but cannot be
//! opened here (the Map Inspector or `navkit.py prepare` prepares it once), and a terrain unpack that is missing or
//! older than its bundle is not used (the level opens without its terrain and says so). The one thing written is
//! the collision index's mesh-alias cache, outside the game data, as `level.load` writes it
//! (`%LOCALAPPDATA%\bfbc2-tool-suite\cache\collision`, `BFBC2_COLLISION_CACHE`; `0` turns it off).

use std::fs;
use std::path::{Path, PathBuf};

use bc2_scene::scene::{self, LoadOptions};
use bc2_scene::walk;

/// Project OPFOR BFBC2's folder name, beside the suite.
pub const OPFOR_DIR: &str = "project-opfor-bfbc2";
/// The environment variable naming Project OPFOR BFBC2's folder.
pub const OPFOR_ENV: &str = "OPFOR_ROOT";

/// The suite's root folder as this crate was built in it (`rust\bc2-editor\..\..`).
#[must_use]
pub fn suite_root() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent().and_then(Path::parent).unwrap_or(here).to_path_buf()
}

/// `<start>\<name>`, then `<each folder above start>\<name>` (`paths.walk_up`).
#[must_use]
pub fn walk_up(start: &Path, name: &str) -> Vec<PathBuf> {
    start.ancestors().map(|d| d.join(name)).collect()
}

/// The folders tried for Project OPFOR BFBC2, in order: `given`, `OPFOR_ROOT`, then the suite and every folder above
/// it (`paths.opfor_candidates`), then the editor's own folder and those above it.
#[must_use]
pub fn opfor_candidates(given: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = given.map(Path::to_path_buf).into_iter().collect();
    if let Some(v) = std::env::var_os(OPFOR_ENV).filter(|v| !v.is_empty()) {
        out.push(PathBuf::from(v));
    }
    out.extend(walk_up(&suite_root(), OPFOR_DIR));
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        for c in walk_up(&exe_dir, OPFOR_DIR) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// Project OPFOR BFBC2's folder: the first candidate with a `game` folder.
///
/// # Errors
///
/// None has one: the message names every folder tried, as `paths.GameCopyNotFound`'s does.
pub fn find_opfor(given: Option<&Path>) -> Result<PathBuf, String> {
    let looked = opfor_candidates(given);
    for c in &looked {
        if c.join("game").is_dir() {
            return Ok(std::path::absolute(c).unwrap_or_else(|_| c.clone()));
        }
    }
    let list: Vec<String> = looked.iter().map(|p| p.display().to_string()).collect();
    Err(format!(
        "Project OPFOR BFBC2 not found - pass --opfor <folder> or set OPFOR_ROOT (looked for a game\\ folder in: {})",
        list.join(", ")
    ))
}

/// A level of the game copy, as the start screen lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelInfo {
    /// The level's name, lower case (`mp_005`).
    pub lv: String,
    /// NavKit has prepared it (`work\levels\<lv>\dist`): it can be opened.
    pub prepared: bool,
    /// Its terrain unpack is there and current.
    pub terrain_current: bool,
}

/// The entries of `dir` in the order the file system lists them (Python's `os.listdir` / `glob` order).
fn entries(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir).map_or_else(|_| Vec::new(), |r| r.flatten().map(|e| e.path()).collect())
}

fn lower_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_lowercase())
}

/// The levels of the game copy (`navkit.Project.level_dirs`): every folder of `game\Dist\win32\levels` with a
/// `level-00.fbrb`, except the `_common` folders and OPFOR's `opfor_*` data-fill bundles; sorted by name.
#[must_use]
pub fn list_levels(opfor: &Path) -> Vec<LevelInfo> {
    let base = opfor.join("game").join("Dist").join("win32").join("levels");
    let mut out: Vec<LevelInfo> = entries(&base)
        .into_iter()
        .filter(|d| d.join("level-00.fbrb").is_file())
        .map(|d| lower_name(&d))
        .filter(|lv| !lv.ends_with("_common") && !lv.starts_with("opfor_"))
        .map(|lv| LevelInfo {
            prepared: opfor.join("work").join("levels").join(&lv).join("dist").is_dir(),
            terrain_current: terrain_unpack(opfor, &lv).is_ok(),
            lv,
        })
        .collect();
    out.sort_by(|a, b| a.lv.cmp(&b.lv));
    out
}

/// The size and modification time of a file as `level._terrain_file`'s stamp writes them (`st_size st_mtime_ns`).
fn stamp_of(path: &Path) -> Option<String> {
    let m = fs::metadata(path).ok()?;
    let ns = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(format!("{} {ns}", m.len()))
}

/// The level's terrain bundle (`terrain-00.fbrb`): Dist first, then Package.
fn terrain_bundle(opfor: &Path, lv: &str) -> Option<PathBuf> {
    let game = opfor.join("game");
    for top in [
        game.join("Dist").join("win32").join("levels"),
        game.join("Package").join("levels"),
    ] {
        for d in entries(&top) {
            if lower_name(&d) == lv && d.join("terrain-00.fbrb").is_file() {
                return Some(d.join("terrain-00.fbrb"));
            }
        }
    }
    None
}

/// The level's terrain unpack folder (`work\levels\<lv>\terrain`) when it can be used as it is: there and stamped
/// from the current bundle (or there and no bundle to compare with, as `level._terrain_file` takes it).
///
/// # Errors
///
/// Why it cannot be used, in words.
pub fn terrain_unpack(opfor: &Path, lv: &str) -> Result<PathBuf, String> {
    let dest = opfor.join("work").join("levels").join(lv).join("terrain");
    if !dest.is_dir() {
        return Err("its terrain is not unpacked yet".into());
    }
    let Some(bundle) = terrain_bundle(opfor, lv) else {
        return Ok(dest);
    };
    let have = fs::read_to_string(dest.join(".source.stamp")).unwrap_or_default();
    if stamp_of(&bundle).is_some_and(|want| want == have.trim()) {
        Ok(dest)
    } else {
        Err("its terrain unpack is older than the terrain bundle".into())
    }
}

/// The first file under `dir` (walk order) named `name`, any case (`terrain.find`).
fn find_named(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_lowercase();
    walk::find(dir, &[want.as_str()])
        .into_iter()
        .flatten()
        .find(|f| f.rel.rsplit('/').next() == Some(want.as_str()))
        .map(|f| f.path)
}

/// The last file under `dir` (walk order) named `name`, any case (`water.find_files`).
fn find_named_last(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_lowercase();
    walk::find(dir, &[want.as_str()])
        .into_iter()
        .flatten()
        .rfind(|f| f.rel.rsplit('/').next() == Some(want.as_str()))
        .map(|f| f.path)
}

/// The mesh-alias cache folder `level.load` uses (`collision.cache_folder`).
#[must_use]
pub fn alias_cache() -> Option<PathBuf> {
    match std::env::var_os("BFBC2_COLLISION_CACHE") {
        Some(v) if v == "0" => None,
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => {
            let base = std::env::var_os("LOCALAPPDATA")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))?;
            Some(base.join("bfbc2-tool-suite").join("cache").join("collision"))
        }
    }
}

/// What [`load_options`] found: the scene's options and what the level opens without.
#[derive(Debug, Clone)]
pub struct Found {
    /// The scene's options.
    pub options: LoadOptions,
    /// Notes in words (no terrain unpack ...).
    pub notes: Vec<String>,
}

/// The scene options `level.load(lv)` builds, with geometry (static groups, collision, terrain, water).
///
/// # Errors
///
/// The level is not prepared, or has no partition.
pub fn load_options(opfor: &Path, lv: &str) -> Result<Found, String> {
    let lv = lv.to_lowercase();
    let base = opfor.join("work").join("levels").join(&lv);
    if !base.join("dist").is_dir() {
        return Err(format!(
            "{lv} is not prepared yet: open it once in the Map Inspector (or run navkit.py prepare {lv}), then here"
        ));
    }
    let mut bases = vec![base.join("pkg"), base.join("dist")];
    bases.extend(
        entries(&opfor.join("work").join("navkit").join("common"))
            .into_iter()
            .filter(|d| d.is_dir()),
    );
    // the level partition: a Package overlay wins over Dist
    let partition = bases
        .iter()
        .take(2)
        .flat_map(|b| entries(&b.join("levels")))
        .find(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dbx"))
                && p.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == lv)
        })
        .ok_or_else(|| format!("{lv}: level partition not found under {}", base.display()))?;
    let name = partition
        .file_stem()
        .map_or_else(|| lv.to_uppercase(), |s| s.to_string_lossy().into_owned());
    // static model groups: each group file once, the Package copy first
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
    let mut notes = Vec::new();
    let tname = scene::terrain_name(&partition)
        .ok()
        .flatten()
        .unwrap_or_else(|| lv.clone());
    let (heightfield, water) = match terrain_unpack(opfor, &lv) {
        Ok(dest) => {
            let hf = find_named(&dest, &format!("{tname}.heightfield-000.res"));
            let mesh = find_named_last(&dest, &format!("{tname}.water.mesh.res"));
            let visual = find_named_last(&dest, &format!("{tname}.water.res"));
            (hf, mesh.map(|m| (m, visual)))
        }
        Err(why) => {
            notes.push(format!(
                "Terrain not shown: {why}. Open the level once in the Map Inspector to unpack it."
            ));
            (None, None)
        }
    };
    let options = LoadOptions {
        groups,
        geometry: true,
        heightfield,
        terrain_stride: 2,
        water,
        alias_cache: alias_cache(),
        ..LoadOptions::new(name, partition, bases)
    };
    Ok(Found { options, notes })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn candidates_walk_up() {
        let c = walk_up(Path::new(r"C:\a\b"), OPFOR_DIR);
        assert_eq!(c.first().unwrap(), Path::new(r"C:\a\b\project-opfor-bfbc2"));
        assert_eq!(c.last().unwrap(), Path::new(r"C:\project-opfor-bfbc2"));
        let given = opfor_candidates(Some(Path::new(r"X:\given")));
        assert_eq!(given.first().unwrap(), Path::new(r"X:\given"));
    }

    #[test]
    fn a_stand_in_tree() {
        let root = std::env::temp_dir().join(format!("bc2_editor_levels_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let levels = root.join("game").join("Dist").join("win32").join("levels");
        for (d, bundle) in [
            ("MP_005", true),
            ("mp_common", true),
            ("OPFOR_fill", true),
            ("sp_01", true),
            ("empty", false),
        ] {
            fs::create_dir_all(levels.join(d)).unwrap();
            if bundle {
                fs::write(levels.join(d).join("level-00.fbrb"), b"x").unwrap();
            }
        }
        fs::create_dir_all(root.join("work").join("levels").join("sp_01").join("dist")).unwrap();
        assert_eq!(find_opfor(Some(&root)).unwrap(), std::path::absolute(&root).unwrap());
        let l = list_levels(&root);
        let names: Vec<_> = l.iter().map(|i| i.lv.as_str()).collect();
        assert_eq!(names, ["mp_005", "sp_01"]);
        assert!(!l[0].prepared && l[1].prepared);
        assert!(load_options(&root, "mp_005").unwrap_err().contains("not prepared"));
        assert!(
            load_options(&root, "sp_01")
                .unwrap_err()
                .contains("partition not found")
        );
        // a terrain unpack is used only while its stamp is the bundle's
        fs::write(levels.join("sp_01").join("terrain-00.fbrb"), b"terrain").unwrap();
        let dest = root.join("work").join("levels").join("sp_01").join("terrain");
        fs::create_dir_all(&dest).unwrap();
        assert!(terrain_unpack(&root, "sp_01").is_err());
        fs::write(
            dest.join(".source.stamp"),
            stamp_of(&levels.join("sp_01").join("terrain-00.fbrb")).unwrap(),
        )
        .unwrap();
        assert_eq!(terrain_unpack(&root, "sp_01").unwrap(), dest);
        let _ = fs::remove_dir_all(&root);
    }
}
