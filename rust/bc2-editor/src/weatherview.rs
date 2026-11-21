//! The weather picker (round VW2): the level's own weather states - every state of the weather system its weather
//! entity names - each one's look in both views. A view setting (never in the project's log; an export's weather is
//! the Export window's choice, [`bc2_levelwrite::weather`]).
//!
//! - **The level's own state** is the one players see as it starts: the spawn-time switch (the first enabled
//!   `WeatherStateEntityData` with `StartTransitionOnSpawn`) when it names a state, else the settings' `ActiveState`
//!   (spec RNDR-WTHR-6; WX0: in 45 of 70 levels with a switch it names another state, and the `ActiveState`
//!   placeholders are dark or unlit). The game-accurate view opens on it.
//! - **The game-accurate view** draws a chosen state with its light, post-process and sky ([`World::set_state`]).
//! - **The simplified view** tints its scene-lit colours and its sky by the state's look ([`weather::tint`]).

use std::fmt::Write as _;
use std::path::Path;

use crate::error::EditorError;
use bc2_render::files::Files;
use bc2_render::weather::{self, LevelWeather, Look, Weather};
use bc2_render::world::{self, Placed};

/// One state of the level's weather system.
#[derive(Debug, Clone)]
pub struct StateRow {
    /// Its partition (`weather/r_mp02/states/ons_mp_002_night`).
    pub partition: String,
    /// Its short name.
    pub name: String,
    /// What DICE's name says it is (night, rain, cutscene ...; empty: a plain state).
    pub kind: &'static str,
    /// Its look from its values (`None`: its partitions did not read).
    pub look: Option<Look>,
    /// The simplified view's tint.
    pub tint: Option<[f32; 3]>,
    /// Its key light (colour times intensity) and `MiddleGray`, for the picker's tooltip.
    pub key: [f32; 3],
    /// `MiddleGray`.
    pub middle_gray: f32,
}

/// What a state is, from DICE's name (WX0's classification by name: `classify.py`).
#[must_use]
pub fn kind_of(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| n.contains(x));
    if has(&["night"]) {
        "night"
    } else if has(&["rain"]) {
        "rain"
    } else if has(&["lightning", "storm"]) && !has(&["snowstorm"]) {
        "storm"
    } else if has(&["snow", "frost", "blizzard"]) {
        "snow"
    } else if has(&["fog", "lowvis"]) {
        "fog"
    } else if has(&["dawn", "dusk", "morning", "evening", "sunset"]) {
        "dawn / dusk"
    } else if has(&["_win"]) {
        "round end"
    } else if has(&["_cs", "cutscene", "cs_"]) {
        "cutscene"
    } else if has(&["indoor", "sewer", "basement", "insideboat", "tunnel"]) {
        "indoor"
    } else if matches!(
        n.as_str(),
        "default" | "deafult" | "state1" | "nolighting" | "defaultstate"
    ) {
        "placeholder"
    } else {
        ""
    }
}

/// The level's weather and its states, read once.
pub struct WeatherView {
    /// The files the states are read from.
    pub files: Files,
    /// The level's weather.
    pub level: LevelWeather,
    /// Every state.
    pub rows: Vec<StateRow>,
    /// The state the user chose (`None`: none yet; both views show the level's own).
    pub shown: Option<usize>,
}

impl WeatherView {
    /// Reads a level's weather and every state of its system (`package`: a level of `game\Package` only).
    ///
    /// # Errors
    /// The level's files or its weather cannot be read.
    pub fn load(opfor: &Path, lv: &str, level: &str, placed: &[Placed], package: bool) -> Result<Self, EditorError> {
        let (files, level) = world::weather_files(opfor, lv, level, placed, package)?;
        Ok(Self::with(files, level))
    }

    /// The rows of a level's weather read from its files.
    #[must_use]
    pub fn with(files: Files, level: LevelWeather) -> Self {
        let rows = (0..level.states.len())
            .map(|k| {
                let w = level.state(&files, k).ok();
                row(&level, k, w.as_ref())
            })
            .collect();
        Self {
            files,
            level,
            rows,
            shown: None,
        }
    }

    /// State `k`'s partitions read.
    ///
    /// # Errors
    /// They are not found.
    pub fn state(&self, k: usize) -> Result<Weather, EditorError> {
        Ok(self.level.state(&self.files, k)?)
    }

    /// The state both views show: the chosen one, else the level's own.
    #[must_use]
    pub fn current(&self) -> usize {
        self.shown.unwrap_or_else(|| self.level.starts_in())
    }

    /// A row's label in the picker: its number, name and kind, and what the level does with it.
    #[must_use]
    pub fn label(&self, k: usize) -> String {
        let Some(r) = self.rows.get(k) else {
            return String::new();
        };
        let mut s = format!("{k}: {}", r.name);
        if !r.kind.is_empty() {
            let _ = write!(s, " ({})", r.kind);
        }
        let mut what = Vec::new();
        if self.level.spawn == Some(k) {
            what.push("spawn switch");
        }
        if self.level.active == k {
            what.push("ActiveState");
        }
        if !what.is_empty() {
            let _ = write!(s, " - {}", what.join(", "));
        }
        s
    }
}

fn row(level: &LevelWeather, k: usize, w: Option<&Weather>) -> StateRow {
    let name = level.name(k);
    let (key, mg) = w.map_or(([0.0; 3], 0.0), |w| {
        let ol = &w.outdoor_light;
        let i = ol
            .get("KeyColorIntensity")
            .and_then(|v| v.first().copied())
            .unwrap_or(1.0);
        let c = ol.get("KeyColor").cloned().unwrap_or_default();
        (
            [0, 1, 2].map(|j| c.get(j).copied().unwrap_or(0.0) * i),
            w.post_process
                .get("MiddleGray")
                .and_then(|v| v.first().copied())
                .unwrap_or(0.0),
        )
    });
    StateRow {
        partition: level.states.get(k).cloned().unwrap_or_default(),
        kind: kind_of(&name),
        name,
        look: w.map(weather::look),
        tint: w.map(weather::tint),
        key,
        middle_gray: mg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_from_dicess_names() {
        assert_eq!(kind_of("ons_mp_002_night"), "night");
        assert_eq!(kind_of("afterrain"), "rain");
        assert_eq!(kind_of("prison_rain"), "rain");
        assert_eq!(kind_of("lightning_new"), "storm");
        assert_eq!(kind_of("sp04_frostbite"), "snow");
        assert_eq!(kind_of("mp_008_ons_fog"), "fog");
        assert_eq!(kind_of("mp04_morning"), "dawn / dusk");
        assert_eq!(kind_of("day_1_us_win"), "round end");
        assert_eq!(kind_of("sp03_cs_17_01"), "cutscene");
        assert_eq!(kind_of("deafult"), "placeholder");
        assert_eq!(kind_of("midday"), "");
    }
}
