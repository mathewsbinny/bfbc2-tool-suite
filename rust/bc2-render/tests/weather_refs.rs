//! A level's weather found the way the game finds it (round GW1): the level partition's instances name their
//! blueprints; the blueprint whose object is a `WeatherSystemEntityData` names its `WeatherSystem` and `Settings`
//! partitions; each is read wherever the level's stack holds it ([`bc2_render::world::resolve_weather`]).
//!
//! - OPFOR's WW2 levels, read from their bundles in place (nothing unpacked): each finds its own state of the shared
//!   `weathersystem_wake` - Guadalcanal's and Iwo Jima's settings are named after the level, not `weathersettings*`,
//!   so until GW1 the view had no weather for them and failed.
//! - Other Package levels (Vietnam, Onslaught) when the game copy has them: a weather state is found by reference.
//! - The prepared stock levels (`work\levels\<lv>`, read only): the reference chain gives the state the old folder
//!   read gave wherever that read succeeded.
//!
//! No Direct3D: only the partitions are read. Skipped (with a note) without the game copy or a level.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::doc_markdown,
    clippy::panic,
    missing_docs
)]

use crate::common;

use std::path::Path;

use bc2_render::files::{self, Files};
use bc2_render::weather::{self, Weather};
use bc2_render::world::{Placed, resolve_weather};

/// The blueprints the level partition's instances name (`<field ref="path/guid" name="ReferencedObject"`), as placed things.
fn placed_blueprints(files: &Files, level: &str) -> Option<Vec<Placed>> {
    let xml = bc2_formats::dbx::to_xml(&files.read(&format!("levels/{level}.dbx"))?).ok()?;
    let mut out: Vec<Placed> = Vec::new();
    for line in xml.lines().filter(|l| l.contains("name=\"ReferencedObject\"")) {
        let Some(r) = line.split("ref=\"").nth(1).and_then(|r| r.split('"').next()) else {
            continue;
        };
        let Some(path) = r.rfind('/').and_then(|k| r.get(..k)) else {
            continue;
        };
        out.push(Placed {
            guid: String::new(),
            blueprint: path.to_owned(),
            rows: [0.0; 16],
            group: None,
        });
    }
    Some(out)
}

/// A Package level's weather from its bundles in place: read, fetch what was asked for, again, until nothing more
/// is asked for (as `World::load_package` does).
fn package_weather(game: &Path, lv: &str) -> Option<(Weather, Option<String>)> {
    let stack = files::package_stack(game, lv)?;
    let files = Files::bundles(&stack).unwrap();
    for _ in 0..12 {
        let placed = placed_blueprints(&files, lv).unwrap_or_default();
        let got = resolve_weather(&files, lv, &placed, None);
        let missed = files.missed();
        if missed.is_empty() {
            return Some(got);
        }
        files.fetch(&missed).unwrap();
    }
    panic!("{lv}: still reading after 12 rounds");
}

#[test]
fn ww2_levels_find_their_weather_through_the_entity() {
    let _gate = crate::gate();
    let game = common::opfor().join("game");
    for (lv, state) in [
        ("guadal_canal", "weather/wake_island/states/guadal_canal"),
        ("iwo_jima_s", "weather/wake_island/states/iwo_jima"),
        // Coral Sea's settings are Wake's (`ActiveState` 0); its WeatherStateEntityData's spawn transition to state
        // 4 (`coral_sea`) is noted, not applied (RNDR-WTHR-6)
        ("coral_sea", "weather/wake_island/states/sunny_day"),
        ("wake_island_s", "weather/wake_island/states/sunny_day"),
    ] {
        let Some((w, note)) = package_weather(&game, lv) else {
            println!("{lv}: skipped, not in the game copy's registry");
            continue;
        };
        println!("{lv}: {} {}", w.state, note.as_deref().unwrap_or(""));
        assert_eq!(w.state, state, "{lv}: {note:?}");
        assert!(
            !note
                .as_deref()
                .is_some_and(|n| n.starts_with(weather::NO_WEATHER) || n.starts_with("weather read by folder")),
            "{lv}: found through the entity: {note:?}"
        );
        for (kind, f) in [
            ("Sun", &w.sun),
            ("OutdoorLight", &w.outdoor_light),
            ("PostProcess", &w.post_process),
        ] {
            assert!(!f.is_empty(), "{lv}: the state's {kind}");
        }
    }
}

#[test]
fn other_package_levels_find_a_weather_state() {
    let _gate = crate::gate();
    let game = common::opfor().join("game");
    let wanted = std::env::var("BC2_GW1_PACKAGE_LEVELS").unwrap_or_else(|_| "nam_mp_002cq,ons_mp_002".into());
    for lv in wanted.split(',') {
        let Some((w, note)) = package_weather(&game, lv) else {
            println!("{lv}: skipped, not in the game copy's registry");
            continue;
        };
        println!("{lv}: {} {}", w.state, note.as_deref().unwrap_or(""));
        assert!(
            !note.as_deref().is_some_and(|n| n.starts_with(weather::NO_WEATHER)),
            "{lv}: {note:?}"
        );
    }
}

#[test]
fn stock_levels_resolve_as_their_folder_did() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    let Ok(rd) = std::fs::read_dir(opfor.join("work").join("levels")) else {
        println!("skipped: no prepared levels");
        return;
    };
    let mut lvs: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().join("dist").is_dir())
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();
    lvs.sort();
    let mut compared = 0;
    for lv in &lvs {
        let files = Files::new(&files::level_folders(&opfor, lv));
        let Some(placed) = placed_blueprints(&files, lv) else {
            println!("{lv}: skipped, no level partition");
            continue;
        };
        let (w, note) = resolve_weather(&files, lv, &placed, None);
        // the old way: the entity blueprint's folder, else `weather/r_<lv without _>`, read by name
        let folder = placed
            .iter()
            .find_map(|p| {
                let b = p.blueprint.to_lowercase();
                b.find("/weathersystementity")
                    .and_then(|i| b.get(..i))
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("weather/r_{}", lv.replace('_', "")));
        let old = weather::read(&files, &folder);
        println!(
            "{lv}: {} {}| by folder {folder}: {}",
            w.state,
            note.as_deref().map_or_else(String::new, |n| format!("({n}) ")),
            old.as_ref().map_or_else(Clone::clone, |o| o.state.clone())
        );
        assert!(
            !note.as_deref().is_some_and(|n| n.starts_with(weather::NO_WEATHER)),
            "{lv}: {note:?}"
        );
        if let Ok(o) = old {
            assert_eq!(w.state, o.state, "{lv}: the entity's state against the folder's");
            compared += 1;
        }
    }
    println!(
        "{compared} of {} prepared levels compared with the folder read",
        lvs.len()
    );
}

#[test]
fn a_level_without_weather_gets_the_default_sky() {
    let _gate = crate::gate();
    // no level partition, no weather entity, no folder: the neutral weather and the status line's words
    let files = Files::default();
    let (w, note) = resolve_weather(&files, "nowhere", &[], Some("weather/r_nowhere"));
    assert_eq!(w.state, weather::neutral().state);
    let note = note.unwrap();
    assert!(note.starts_with(weather::NO_WEATHER), "{note}");
    assert!(note.contains("weather/r_nowhere"), "{note}");
    let (light, _) = weather::light(&w, weather::NOON_DIR);
    assert!(
        light["outdoorLightKeyColor"].iter().take(3).all(|c| *c > 1.0),
        "a lit default"
    );
}
