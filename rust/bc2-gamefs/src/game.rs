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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
