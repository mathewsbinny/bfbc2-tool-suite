//! The BFBC2 Tool Suite's map editor application (`bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` 3.8, 3.9): one
//! window in Rust and egui, its look generated from the suite's design language (`design/project/tokens.json`).
//!
//! Round G1 (2026-11-18): the shell and the read-only map view - the start screen ("View a map": the game copy's
//! levels), the 3D view on the Rust scene (`bc2-scene`: terrain, collision, water, entities as markers in team
//! colours with labels, outlines), orbit / fly / focus, picking with the selection outline, camera bookmarks, and
//! the docked Outliner, Properties and Problems panels. Nothing is written to the game copy or OPFOR's work folder.
//!
//! - [`theme`], [`fonts`]: the design language as an egui theme (Dark and Light), IBM Plex.
//! - [`levels`]: the game copy, its levels, a level's files (as the Python suite finds them).
//! - [`model`]: the outliner's and properties' model of a scene.
//! - [`camera`], [`render`]: the views (wgpu): perspective and top / front / side, each with its own camera.
//! - [`query`]: rays against the scene's surfaces; vertex and edge snapping; placing on a surface with a sink depth.
//! - [`actions`]: the action registry the top bar, the shortcuts and the command palette share.
//! - [`app`]: the window's contents; [`headless`]: the same without a window (tests, screenshots).
//! - [`demo`]: a made-up scene for tests without a game copy.
//!
//! Round G2 (2026-11-18): [`factions`] (team labels by faction, from the level's own data), the 3D view's own
//! tokens (`view-*`, `scene-*` and the third neutral `unowned`, in [`theme`]), [`help`] (the Help window, generated
//! from the registry) and [`prepare`] (an unprepared level is prepared when it is opened, with its progress; the one
//! write to OPFOR's work folder, through the suite's own NavKit prepare and terrain unpack).
//!
//! Round E2 (2026-11-18): editing a map. [`edit`]: a map project (`bc2_levelwrite`) open on its level, the scene and
//! the partition derived from its log; [`gizmo`]: the move and rotate gizmo; [`math`]: transforms; the Objects
//! workspace's interface in `app` (selection, gizmo and snapping toolbar, ruler, History, Layers, folders, groups).
//!
//! Round G3 (2026-11-18): [`package`] - the levels that live only in `game\Package` (Vietnam, Pacific War and
//! DICE's other Package-only levels) are listed, prepared from their bundles in memory into the editor's own copy
//! (`work\editor\levels\<level>`, never `work\levels`) and opened like the others.
#![deny(unsafe_code)]
#![allow(
    clippy::many_single_char_names,
    reason = "vector and colour maths: x, y, z, r, g, b, and the camera's basis"
)]
#![allow(clippy::cast_precision_loss, reason = "pixel sizes and counts, far below 2^23")]
#![allow(
    clippy::too_many_lines,
    reason = "interface layout and pipeline set-up read top to bottom, as the window shows them"
)]

pub mod actions;
pub mod app;
pub mod camera;
pub mod catalog;
pub mod demo;
pub mod edit;
pub mod factions;
pub mod fonts;
pub mod gizmo;
pub mod headless;
pub mod help;
pub mod levels;
pub mod math;
pub mod model;
pub mod package;
pub mod prepare;
pub mod query;
pub mod render;
pub mod theme;
