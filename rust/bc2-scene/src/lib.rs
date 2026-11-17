//! A Battlefield: Bad Company 2 level as the suite's tools see it, in pure Rust: the BFBC2 Tool Suite's scene
//! (`bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` 3.2, 3.8).
//!
//! A port of `core/bfbc2core/level.py`'s loader, built on `bc2-formats`; it replaces the Python loader only where it
//! gives the same level (`core/tests/test_scene_native.py`).
//!
//! - [`collision`]: which packfile each blueprint's collision comes from (NavKit's `havok_index` and
//!   `mesh_aliases`).
//! - [`scene`]: the level: entities, outlines, collision, terrain and water, loaded on all cores; editable.
//! - [`walk`]: finding files as Python's recursive `glob` finds them, in its order.
#![deny(unsafe_code)]

pub mod collision;
pub mod scene;
pub mod walk;
