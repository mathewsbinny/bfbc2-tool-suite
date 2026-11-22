//! The game-copy installer on the install ledger (round OR1, "OPFOR in Rust" stage 1; audit AU3 OP-1, OP-6): Project
//! OPFOR's WW2 install - `wake_install.py --plan / --apply / --revert` and `ww2_level_install.py`'s install side -
//! as a library, run by `bfbc2 install` (bc2-cli) and, later, the map editor's installer (round IN1).
//!
//! OPFOR's build stages every bundle and writes the install plan ([`plan::Plan`]: its decisions as data - the
//! levels, their registration blocks, the DLL's markers, what marks a backup as holding the install's content). This
//! crate does the rest:
//!
//! | Project OPFOR (Python, until OR1) | here |
//! |---|---|
//! | `wake_install.mft_parse` / `mft_text` / `mft_with_level` / `bm_with_level` | [`text`] |
//! | `wake_install.ini_value` / `ini_with_level` / `ini_with_key` / `ini_without_key` / `maps_with_levels` | [`text`] |
//! | `wake_install.ww2_marks` / `before_wake` / `guard_backup` / `ini_backup_refresh` | [`marks::Guard`] |
//! | `ww2_level_install.install_all`'s guard and write half, `set_ini`, `register`, `install_patch`, `mark_stings`, `mark_tips`, `verify` | [`run::Run::install`] |
//! | `ww2_level_install.revert_all`, `wake_install.revert` (less the menu mode's revert) | [`run::Run::revert`] |
//! | `ww2_level_install.save_staged` / `load_staged` / `fingerprint` | [`staged`] |
//! | `opforcore.stages.Stages.install` / `restate` / `check_install` | `bc2_gamefs::ledger` ([`Ledger::install_keep`](bc2_gamefs::ledger::Ledger::install_keep), `restate`), [`run`] |
//! | `rebuild_all.locked` (the install takes it too) | [`lock`] |
//! | `opforcore\migrate.py`; the mod manager (round MM1): enable / disable / uninstall / restore-original / verify | [`mods`] |

pub mod lock;
pub mod marks;
pub mod mods;
pub mod plan;
pub mod pytext;
pub mod run;
pub mod staged;
pub mod text;
