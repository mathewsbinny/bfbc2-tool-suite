//! The suite's settings for its users (round OR15, IN1): the game folder the installs write into, so the level
//! installer, the doctor and the editor's Install work on any game copy - an ordinary install of the game, not only
//! Project OPFOR BFBC2's `game\` folder - with no Python and no OPFOR needed.
//!
//! ```text
//! %APPDATA%\bfbc2-tool-suite\settings.json     {"format": "bfbc2-settings/1", "game_folder": "D:\\Games\\BFBC2"}
//! ```
//!
//! `BFBC2_SETTINGS` names another file (tests). The file holds only the user's choices, never a PC name; it is written
//! beside itself and moved over the old one ([`crate::atomic`]). A file that is not one of these is refused in words,
//! never overwritten unasked.
//!
//! The game folder a tool uses ([`game_folder`]): the one given (`--game`), else the setting, else Project OPFOR
//! BFBC2's `game\` when OPFOR is there; each answer says where it came from.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::game::Game;

/// The settings file's format.
pub const FORMAT: &str = "bfbc2-settings/1";
/// The environment variable naming another settings file.
pub const SETTINGS_ENV: &str = "BFBC2_SETTINGS";

/// The settings file: `BFBC2_SETTINGS`, else `%APPDATA%\bfbc2-tool-suite\settings.json` (None: no such folder).
#[must_use]
pub fn path() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os(SETTINGS_ENV).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(v));
    }
    std::env::var_os("APPDATA")
        .filter(|v| !v.is_empty())
        .map(|a| PathBuf::from(a).join("bfbc2-tool-suite").join("settings.json"))
}

/// The user's settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// The game copy installs write into.
    pub game_folder: Option<PathBuf>,
}

impl Settings {
    /// The settings in `file` (none there: the defaults).
    ///
    /// # Errors
    /// The file is there but is not a settings file, in words.
    pub fn load_from(file: &Path) -> Result<Self, String> {
        let data = match fs::read(file) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("{}: {e}", file.display())),
        };
        let bad = |why: &str| {
            format!(
                "{}: not the suite's settings ({why}) - fix it or delete it",
                file.display()
            )
        };
        let v: Value = serde_json::from_slice(&data).map_err(|e| bad(&e.to_string()))?;
        if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err(bad(&format!("format is not {FORMAT}")));
        }
        let game_folder = match v.get("game_folder") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if !s.is_empty() => Some(PathBuf::from(s)),
            Some(_) => return Err(bad("game_folder is not a folder name")),
        };
        Ok(Self { game_folder })
    }

    /// The settings in [`path`]'s file.
    ///
    /// # Errors
    /// As [`Settings::load_from`].
    pub fn load() -> Result<Self, String> {
        match path() {
            Some(p) => Self::load_from(&p),
            None => Ok(Self::default()),
        }
    }

    /// Written to `file` (its folder made), atomically, with sorted keys.
    ///
    /// # Errors
    /// The write failed.
    pub fn save_to(&self, file: &Path) -> Result<(), String> {
        let v = json!({ "format": FORMAT, "game_folder": self.game_folder.as_ref().map(|p| p.to_string_lossy()) });
        let mut text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
        text.push('\n');
        if let Some(d) = file.parent() {
            fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        crate::atomic::write(file, &[text.as_bytes()]).map_err(|e| format!("{}: {e}", file.display()))
    }

    /// Written to [`path`]'s file.
    ///
    /// # Errors
    /// No settings folder (`APPDATA` unset), or the write failed.
    pub fn save(&self) -> Result<(), String> {
        let p = path().ok_or("no settings folder: APPDATA is not set (set BFBC2_SETTINGS to a file)")?;
        self.save_to(&p)
    }
}

/// Why `dir` cannot be the game folder (None: it is a copy of the game).
#[must_use]
pub fn not_a_game(dir: &Path) -> Option<String> {
    if !dir.is_dir() {
        return Some(format!("{}: no such folder", dir.display()));
    }
    if Game::of_copy(dir).is_none() {
        return Some(format!(
            "{}: not a copy of the game (no {} there) - choose the folder holding the game's executable",
            dir.display(),
            Game::Bfbc2.executable()
        ));
    }
    None
}

/// Where a game folder came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Given on the command line.
    Given,
    /// The setting (`bfbc2 settings game-folder`).
    Setting,
    /// Project OPFOR BFBC2's `game\`.
    Opfor,
}

impl Source {
    /// In words.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::Given => "given",
            Self::Setting => "the game-folder setting",
            Self::Opfor => "Project OPFOR BFBC2's game folder",
        }
    }
}

/// The game folder a tool uses: `given`, else the setting, else `<opfor>\game` (`opfor`: OPFOR's folder when found).
///
/// # Errors
/// None of them, in words (how to set one); a damaged settings file.
pub fn game_folder(given: Option<&str>, opfor: Option<&Path>) -> Result<(PathBuf, Source), String> {
    if let Some(g) = given.filter(|g| !g.is_empty()) {
        return Ok((PathBuf::from(g), Source::Given));
    }
    if let Some(g) = Settings::load()?.game_folder {
        return Ok((g, Source::Setting));
    }
    if let Some(o) = opfor {
        return Ok((o.join("game"), Source::Opfor));
    }
    Err("no game folder: pass --game <folder>, or set one with bfbc2 settings game-folder <folder>".to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_damage_is_refused() {
        let d = std::env::temp_dir().join(format!("bc2-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let f = d.join("sub").join("settings.json");
        assert_eq!(Settings::load_from(&f).unwrap(), Settings::default());
        let s = Settings {
            game_folder: Some(PathBuf::from(r"D:\Games\BFBC2")),
        };
        s.save_to(&f).unwrap();
        assert_eq!(Settings::load_from(&f).unwrap(), s);
        let text = fs::read_to_string(&f).unwrap();
        assert!(text.contains("\"format\": \"bfbc2-settings/1\""), "{text}");
        fs::write(&f, b"{\"format\": \"other\"}").unwrap();
        assert!(
            Settings::load_from(&f)
                .unwrap_err()
                .contains("not the suite's settings")
        );
        fs::write(&f, b"not json").unwrap();
        assert!(Settings::load_from(&f).unwrap_err().contains("fix it or delete it"));
        assert!(not_a_game(&d).unwrap().contains("not a copy of the game"));
        fs::write(d.join("BFBC2Game.exe"), b"").unwrap();
        assert_eq!(not_a_game(&d), None);
        let _ = fs::remove_dir_all(&d);
    }
}
