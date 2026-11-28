//! The game layer over the file formats (round PR1b-2, audit AU5 ARC-2): what the suite does with a game copy and
//! the user's disk, kept apart from the formats themselves (`bc2-formats` reads and writes bytes; nothing there
//! writes a file, keeps a cache or asks the system about processes).
//!
//! | module | what |
//! |---|---|
//! | [`game`] | the games the suite knows ([`game::Game`]) and a game copy found from a file in it |
//! | [`atomic`] | every file the suite writes: beside its name, flushed, moved over it |
//! | [`ledger`] | the game copy's install ledger (`opfor-ledger/1`): installs and reverts in any order |
//! | [`liveness`] | whether a process runs (the ledger's and the map project's locks), the host name |
//! | [`bundles`] | the bundle cache: reads of the same bundles made cheap, in the process and on disk |
//! | [`budget`] | the caches' budget: every cache's folder and cap, the pruning (one implementation) |
//! | [`points`] | restart points kept on disk by content (the BF1943 source's payloads) |
//! | [`meshindex`] | the index of a game copy's meshes and shaders |
//! | [`sha256`] | SHA-256 (the ledger's file identities) |
//! | [`workers`] | the one worker knob (`BFBC2_WORKERS`) and the process's pool |
//!
//! The multi-game goal (MOH 2010, BFBC1, BF3, MOH Warfighter, BF4 after BFBC2): what differs per game - its
//! executable, its archive and resource formats, where its files live - is reached through [`game::Game`]; the
//! editor core and the tools ask the game layer, not a game's formats, where a copy is and how its files are read.
#![deny(unsafe_code)]

pub mod atomic;
pub mod budget;
pub mod bundles;
pub mod game;
pub mod ledger;
pub mod liveness;
pub mod meshindex;
pub mod points;
pub mod privacy;
pub mod settings;
pub mod sha256;
pub mod workers;
