//! The BFBC2 Tool Suite's map editor application (`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 3.8, 3.9): one
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
//! - [`display`]: how the simplified view shows the objects (round SV1: render mesh, collision, both, bounds; levels of
//!   detail and destruction states; the invisible objects).
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
//!
//! Round E2b (2026-11-19): [`workspace`] - the workspaces as editing methods (View, Objects with a category filter,
//! Terrain and Vegetation as registered stubs, Gameplay, AI) and the editor view's display modes (lit, unlit,
//! wireframe, collision, navmesh, cull tree: [`viewmodes`] reads their data); [`content`] - the content browser's
//! catalogue of placeable blueprints; in `app` the context menus, the sink popup (Alt+wheel), placing, polygon
//! selection and several objects' fields at once; deep duplicates and placements go through levelwrite.
//!
//! Round G4 (2026-11-19): the start flow of the user's design - Open project, Edit existing level (the mode cards,
//! then a mode's multiplayer maps) and Create level (from any existing level, or from a height map: [`heightmap`]
//! checks the file and saves a project of the import's settings); multiplayer or campaign from each level's
//! description ([`catalog::LevelKind`]).
//!
//! Round I1 (2026-11-19): [`appicon`] - the window's icon, the suite's application icon (the .exe carries it too,
//! embedded by `build.rs`).
//!
//! Round GR3 (2026-11-21): [`preview`] - the asset preview (the mesh viewer's browser page folded into the editor): a
//! blueprint picked in Place Objects or any mesh found by name (the asset browser's Meshes mode) read as
//! `meshview.py` reads it and drawn with its own triangle loop (`bc2_convert::meshview`), so the Side, Top and Front
//! views are its pictures pixel for pixel; besides, an orbiting camera, levels of detail, parts, destruction states,
//! textures by channel, wireframe, normals, bounds, comparisons and Save picture.
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
pub mod appicon;
pub mod autolayout;
pub mod camera;
pub mod catalog;
pub mod compare;
pub mod content;
pub mod dbxinspect;
pub mod dbxpanel;
pub mod demo;
pub mod display;
pub mod edit;
pub mod error;
pub mod export_ui;
pub mod factions;
pub mod fonts;
pub mod gameplay;
pub mod gameview;
pub mod gizmo;
pub mod headless;
pub mod heightmap;
pub mod help;
pub mod inspect;
pub mod levels;
pub mod math;
pub mod memberedit;
pub mod model;
pub mod modeview;
pub mod nav;
pub mod navdata;
pub mod navtool;
pub mod navview;
pub mod newterrain_view;
pub mod overlays;
pub mod package;
pub mod placetool;
pub mod prepare;
pub mod preview;
pub mod query;
pub mod render;
pub mod sculpt;
pub mod strip_ui;
pub mod task;
pub mod theme;
pub mod tool;
pub mod validation;
pub mod viewmodes;
pub mod walk;
pub mod weather_ui;
pub mod weatherview;
pub mod workspace;

/// The `VERSION` file at the repository's root, as read at build time (round P1; `CHANGELOG.md`).
const VERSION_FILE: &str = include_str!("../../../VERSION");

/// The suite's version (`0.MINOR.PATCH`), shown in the Help window and by `bfbc2-editor --version`.
#[must_use]
pub fn version() -> &'static str {
    VERSION_FILE.trim()
}

#[cfg(test)]
mod version_tests {
    #[test]
    fn version_is_semver() {
        let v = super::version();
        let parts: Vec<&str> = v.split('.').collect();
        assert_eq!(parts.len(), 3, "{v}");
        assert!(
            parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
            "{v}"
        );
    }
}
