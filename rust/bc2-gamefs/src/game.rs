//! The games the suite knows, and a game copy found from a file in it.
//!
//! Only Battlefield: Bad Company 2 today. The suite is to cover more Frostbite games later (MOH 2010, BFBC1, BF3,
//! MOH Warfighter, BF4: the user, 2026-11-22); what differs per game is asked of [`Game`], so a second game is a new
//! variant and its answers, not a change in every tool.

use std::path::{Path, PathBuf};

use crate::ledger;

/// A game the suite reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Game {
    /// Battlefield: Bad Company 2 (PC, Frostbite 1.5): FBRB bundles of binary DBX partitions.
    Bfbc2,
}

impl Game {
    /// Every game the suite knows.
    pub const ALL: [Game; 1] = [Game::Bfbc2];

    /// The game's name as the suite shows it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Game::Bfbc2 => "Battlefield: Bad Company 2",
        }
    }

    /// The game's executable, in the copy's top folder.
    #[must_use]
    pub const fn executable(self) -> &'static str {
        match self {
            Game::Bfbc2 => "BFBC2Game.exe",
        }
    }

    /// Whether `dir` is a copy of this game (its executable is there).
    #[must_use]
    pub fn is_copy(self, dir: &Path) -> bool {
        dir.join(self.executable()).is_file()
    }

    /// The game whose copy `dir` is.
    #[must_use]
    pub fn of_copy(dir: &Path) -> Option<Game> {
        Game::ALL.into_iter().find(|g| g.is_copy(dir))
    }
}

/// The game copy a file is in that the suite installs into: the nearest folder above it holding the install ledger
/// (`opfor\ledger`; `None`: no ledger there).
#[must_use]
pub fn find_game(path: &Path) -> Option<PathBuf> {
    let mut d = path.parent();
    while let Some(a) = d {
        if ledger::default_root(a).is_dir() {
            return Some(a.to_path_buf());
        }
        d = a.parent();
    }
    None
}

// ------------------------------------------------------------------------------------------- Project OPFOR BFBC2

/// Project OPFOR BFBC2's folder name beside the suite.
pub const OPFOR_DIR: &str = "project-opfor-bfbc2";
/// The environment variable naming Project OPFOR BFBC2's folder.
pub const OPFOR_ENV: &str = "OPFOR_ROOT";

/// Where Project OPFOR BFBC2 is looked for after the folder given and `OPFOR_ROOT` (round PR1b-2-rest: the one
/// finder of the many copies the tools' ports had).
#[derive(Debug, Clone, Copy)]
pub enum Beside<'a> {
    /// `project-opfor-bfbc2` beside this folder only (`shaderdb.find_opfor`: beside the suite).
    Only(&'a Path),
    /// `project-opfor-bfbc2` in each of these folders and every folder above it, each once
    /// (`bfbc2core.paths.find_opfor`: from the suite; the editor also from its own folder).
    Above(&'a [&'a Path]),
}

/// `<start>\<name>`, then `<each folder above start>\<name>` (`paths.walk_up`; `start` made absolute first).
#[must_use]
pub fn walk_up(start: &Path, name: &str) -> Vec<PathBuf> {
    let start = std::path::absolute(start).unwrap_or_else(|_| start.to_path_buf());
    start.ancestors().map(|d| d.join(name)).collect()
}

/// The folders tried for Project OPFOR BFBC2, in order: `given` (when not empty), `OPFOR_ROOT` (when `env` and set),
/// then [`Beside`]'s.
#[must_use]
pub fn opfor_candidates(given: Option<&str>, env: bool, beside: Beside<'_>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = given.filter(|g| !g.is_empty()).map(PathBuf::from).into_iter().collect();
    if env && let Some(v) = std::env::var_os(OPFOR_ENV).filter(|v| !v.is_empty()) {
        out.push(PathBuf::from(v));
    }
    match beside {
        Beside::Only(dir) => {
            let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
            out.push(dir.parent().unwrap_or(&dir).join(OPFOR_DIR));
        }
        Beside::Above(starts) => {
            let mut above: Vec<PathBuf> = Vec::new();
            for s in starts {
                for c in walk_up(s, OPFOR_DIR) {
                    if !above.contains(&c) {
                        above.push(c);
                    }
                }
            }
            out.extend(above);
        }
    }
    out
}

/// Project OPFOR BFBC2 was not found: the folders tried, in order.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "Project OPFOR BFBC2 not found - pass --opfor <folder> or set OPFOR_ROOT (looked for a game\\ folder in: {})",
    .looked.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
)]
pub struct OpforNotFound {
    /// The folders tried.
    pub looked: Vec<PathBuf>,
}

/// Project OPFOR BFBC2's folder: the first of [`opfor_candidates`] with a `game` folder, made absolute.
///
/// # Errors
/// None has one ([`OpforNotFound`] names them).
pub fn find_opfor(given: Option<&str>, env: bool, beside: Beside<'_>) -> Result<PathBuf, OpforNotFound> {
    let looked = opfor_candidates(given, env, beside);
    for c in &looked {
        if c.join("game").is_dir() {
            return Ok(std::path::absolute(c).unwrap_or_else(|_| c.clone()));
        }
    }
    Err(OpforNotFound { looked })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn opfor_is_looked_for_in_order() {
        let c = walk_up(Path::new(r"C:\a\b"), OPFOR_DIR);
        assert_eq!(c.first().unwrap(), Path::new(r"C:\a\b\project-opfor-bfbc2"));
        assert_eq!(c.last().unwrap(), Path::new(r"C:\project-opfor-bfbc2"));
        let only = opfor_candidates(Some(r"X:\given"), false, Beside::Only(Path::new(r"C:\a\suite")));
        assert_eq!(
            only,
            vec![PathBuf::from(r"X:\given"), PathBuf::from(r"C:\a\project-opfor-bfbc2")]
        );
        let a = [Path::new(r"C:\a\b"), Path::new(r"C:\a\c")];
        let above = opfor_candidates(Some(""), false, Beside::Above(&a));
        assert_eq!(above.len(), 4, "{above:?}");
        assert_eq!(above[3], PathBuf::from(r"C:\a\c\project-opfor-bfbc2"));
        let none = find_opfor(
            Some(r"Z:\nowhere\x"),
            false,
            Beside::Only(Path::new(r"Z:\nowhere\suite")),
        )
        .unwrap_err();
        assert!(
            none.to_string()
                .ends_with(r"in: Z:\nowhere\x, Z:\nowhere\project-opfor-bfbc2)"),
            "{none}"
        );
    }

    #[test]
    fn a_copy_is_found_by_its_executable_and_its_ledger() {
        let dir = std::env::temp_dir().join(format!("bc2-gamefs-game-{}", std::process::id()));
        let level = dir.join("Levels").join("MP_005");
        std::fs::create_dir_all(&level).unwrap();
        assert_eq!(Game::of_copy(&dir), None);
        std::fs::write(dir.join("BFBC2Game.exe"), b"").unwrap();
        assert_eq!(Game::of_copy(&dir), Some(Game::Bfbc2));
        assert_eq!(find_game(&level.join("x.fbrb")), None);
        std::fs::create_dir_all(ledger::default_root(&dir)).unwrap();
        assert_eq!(find_game(&level.join("x.fbrb")), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
