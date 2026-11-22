//! Which mod each install belongs to: the install ledger's owners, the level folders installed beside it and the
//! settings, named as Project OPFOR's tools name them (`opforcore.stages.owner_of`: `<stage>:<level>`, or the stage
//! alone for shared files).
//!
//! | mod | what |
//! |---|---|
//! | `ww2` | the Pacific War: owner `wake` (the shared bundles, registration, `opfor.ini`, `maps.ini`), `minimap-edges`, and the level folders with an `opfor-<stage>-installed.json` record (`wake_island_s`, `ww2_common`, ...) |
//! | `pacific-menus` | the Pacific War's menu mode: owner `pacificmenu` (the main menu bundle); needs `ww2` and `vietnam` |
//! | `vietnam` | `opfor.ini` `[patches] vietnam` (on when missing: the DLL reads it so since 2026-11-22) |
//! | `onslaught:<lv>` | the console Onslaught port of a level: its 14 stages (`console_port.py`) and the Napisal-level fixes (`combata`, `combatb`, `sectoroff`) on that level, and `loading:<lv>` on the startup bundle |
//! | `onslaught` | every `onslaught:<lv>`, and the stages' shared files (`loadset`, `loading` on `package.mft` / the startup bundle) |
//! | `mapedit` | the map editor's exports: owner `mapedit` (one patch of the shared files for every export), `mapedit-ai:<lv>`, the exported level folders (`opfor-mapedit-installed.json`) |
//! | `navkit:<lv>`, `stockmp:<lv>` | the suite's NavKit path data and stock-MP AI of a level |
//! | `base` | OPFOR's import (`import`: the merged startup bundle and registration) and the bundleManifest repair (`manifestfix`) |
//! | `owner:<owner>` | any other owner |

use std::path::Path;

/// The console port's stages (`console_port.STAGES`) and the Napisal-level fixes (`apply_*.py`).
pub const ONSLAUGHT_STAGES: &[&str] = &[
    "onslaught",
    "linux",
    "mode",
    "loadset",
    "vehicles",
    "soldiers",
    "shaders",
    "markv",
    "input",
    "deps",
    "cull",
    "loading",
    "hud",
    "icons",
    "combata",
    "combatb",
    "sectoroff",
];

/// A setting mod: one key of an ini file, on or off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    /// The mod.
    pub id: &'static str,
    /// The game-relative file.
    pub file: &'static str,
    /// Its section.
    pub section: &'static str,
    /// Its key.
    pub key: &'static str,
    /// The value that is on.
    pub on: &'static str,
    /// The value that is off.
    pub off: &'static str,
    /// What the reader (the DLL) takes a missing key for.
    pub default_on: bool,
}

/// The setting mods.
pub const SETTINGS: &[Setting] = &[Setting {
    id: "vietnam",
    file: "opfor.ini",
    section: "patches",
    key: "vietnam",
    on: "1",
    off: "0",
    default_on: true,
}];

/// The ini keys an owner's install sets in a settings file (Project OPFOR's WW2 install: `wake_install.ini_with_level`
/// and `[gamemode] pacific_levels`; `bc2_install::marks::Guard::ini_refresh`). On disable the file goes back to its
/// state before the owner **with every other edit made since kept** (the launcher writes `opfor.ini` without a
/// record): those keys alone take their values from before.
/// (owner's stage, file, its keys: (section, key)).
pub type OwnedKeys = (&'static str, &'static str, &'static [(&'static str, &'static str)]);

/// The owners' keys.
pub const OWNED_INI_KEYS: &[OwnedKeys] = &[("wake", "opfor.ini", &[("dev", "level"), ("gamemode", "pacific_levels")])];

/// What one mod needs enabled to work (a dependent is disabled first, with a confirmation).
pub const REQUIRES: &[(&str, &[&str])] = &[("pacific-menus", &["ww2", "vietnam"])];

/// Every mod's title, for the list.
#[must_use]
pub fn title(id: &str) -> String {
    let (head, lv) = id.split_once(':').unwrap_or((id, ""));
    match head {
        "ww2" => "Pacific War (WW2 levels, ww2_common, the shared bundles' WW2 content)".to_owned(),
        "pacific-menus" => "Pacific War menus (the main menu's Pacific mode)".to_owned(),
        "vietnam" => "Vietnam opt-in (opfor.ini [patches] vietnam)".to_owned(),
        "onslaught" if lv.is_empty() => "Onslaught (the console ports' shared files)".to_owned(),
        "onslaught" => format!("Onslaught console port of {}", lv.to_uppercase()),
        "mapedit" => "Map editor exports".to_owned(),
        "navkit" => format!("NavKit path data of {}", lv.to_uppercase()),
        "stockmp" => format!("stock-MP AI of {}", lv.to_uppercase()),
        "base" => "OPFOR's import (startup bundle, registration) and the bundleManifest repair".to_owned(),
        _ => format!("install {id}"),
    }
}

/// The mod a ledger owner belongs to.
#[must_use]
pub fn mod_of_owner(owner: &str) -> String {
    let (stage, lv) = owner.split_once(':').unwrap_or((owner, ""));
    match stage {
        "wake" | "minimap-edges" => "ww2".to_owned(),
        s if s.starts_with("ww2") => "ww2".to_owned(),
        "pacificmenu" => "pacific-menus".to_owned(),
        "mapedit" | "mapedit-ai" => "mapedit".to_owned(),
        "navkit" | "stockmp" if !lv.is_empty() => format!("{stage}:{lv}"),
        "import" | "manifestfix" => "base".to_owned(),
        s if ONSLAUGHT_STAGES.contains(&s) => {
            if lv.is_empty() {
                "onslaught".to_owned()
            } else {
                format!("onslaught:{lv}")
            }
        }
        _ => format!("owner:{owner}"),
    }
}

/// The mod of a level folder installed beside the ledger (`Package/levels/<lv>` holding `record`, its install
/// record's file name), None when it is not one.
#[must_use]
pub fn mod_of_folder(record: &str) -> Option<&'static str> {
    let low = record.to_lowercase();
    if low == "opfor-mapedit-installed.json" {
        Some("mapedit")
    } else if low.starts_with("opfor-") && low.ends_with("-installed.json") {
        Some("ww2")
    } else {
        None
    }
}

/// The level folders installed beside the ledger: (game-relative folder, its record's file name, mod).
#[must_use]
pub fn folders(game: &Path) -> Vec<(String, String, &'static str)> {
    let mut out = Vec::new();
    let top = game.join("Package").join("levels");
    let Ok(rd) = std::fs::read_dir(&top) else {
        return out;
    };
    let mut dirs: Vec<_> = rd.flatten().filter(|e| e.path().is_dir()).collect();
    dirs.sort_by_key(std::fs::DirEntry::file_name);
    for d in dirs {
        let Ok(files) = std::fs::read_dir(d.path()) else {
            continue;
        };
        let mut names: Vec<String> = files
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        if let Some((rec, m)) = names.iter().find_map(|n| mod_of_folder(n).map(|m| (n.clone(), m))) {
            out.push((format!("Package/levels/{}", d.file_name().to_string_lossy()), rec, m));
        }
    }
    out
}

/// The mods a name selects: the name itself, or a group (`onslaught`: it and every `onslaught:<lv>`; `navkit` /
/// `stockmp`: every level's), among `known`.
#[must_use]
pub fn select(name: &str, known: &[String]) -> Vec<String> {
    let name = name.to_lowercase();
    let mut out: Vec<String> = known
        .iter()
        .filter(|k| **k == name || k.starts_with(&format!("{name}:")))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The mods `id` needs (REQUIRES).
#[must_use]
pub fn requires(id: &str) -> &'static [&'static str] {
    REQUIRES.iter().find(|(m, _)| *m == id).map_or(&[], |(_, r)| r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_map_to_mods() {
        for (o, m) in [
            ("wake", "ww2"),
            ("minimap-edges", "ww2"),
            ("pacificmenu", "pacific-menus"),
            ("cull:ons_mp_002", "onslaught:ons_mp_002"),
            ("loading:ons_mp_004", "onslaught:ons_mp_004"),
            ("loadset", "onslaught"),
            ("sectoroff:ons_mp_005gr", "onslaught:ons_mp_005gr"),
            ("navkit:mp_005", "navkit:mp_005"),
            ("stockmp:mp_001", "stockmp:mp_001"),
            ("mapedit-ai:x_cq", "mapedit"),
            ("import", "base"),
            ("manifestfix", "base"),
            ("something", "owner:something"),
        ] {
            assert_eq!(mod_of_owner(o), m, "{o}");
        }
        assert_eq!(mod_of_folder("opfor-wake-installed.json"), Some("ww2"));
        assert_eq!(mod_of_folder("opfor-mapedit-installed.json"), Some("mapedit"));
        assert_eq!(mod_of_folder("level-00.fbrb"), None);
    }

    #[test]
    fn groups_select_their_levels() {
        let known: Vec<String> = [
            "onslaught",
            "onslaught:ons_mp_002",
            "onslaught:ons_mp_004",
            "ww2",
            "navkit:mp_001",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        assert_eq!(select("onslaught", &known).len(), 3);
        assert_eq!(select("onslaught:ons_mp_002", &known), vec!["onslaught:ons_mp_002"]);
        assert_eq!(select("navkit", &known), vec!["navkit:mp_001"]);
        assert_eq!(select("onsl", &known), Vec::<String>::new());
    }
}
