//! The map editor's level writer and map project (`bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` 3.1, 3.2 item 4;
//! round E2, 2026-11-18).
//!
//! - [`project`]: the map project, `<name>.mapedit.json`: the base level (its name, partition, sha1 and content
//!   profile), the operation log with `by` and `when` per operation, undo and redo, the base-changed report, the
//!   autosave beside it and crash recovery. The project holds data only (section 6, ruling 2): guids, transforms,
//!   field values and names, never game content.
//! - [`ops`]: the operations (move, rotate, delete, duplicate, set field, and the editor's organisation: layers,
//!   folders, groups) and what they say about each object (its layer, folder, group, surface placement).
//! - [`write`]: levelwrite - one mutation API for a level partition on [`bc2_formats::dbx::edit::EditDbx`] (the port
//!   of `dbxbin.py`, so an untouched partition writes back byte for byte): transforms, delete (DbxEdit's `drop`, and
//!   the event connections of what is deleted), duplicate (a clone with a new UUID v5 guid, wired into
//!   `LevelData.Objects` and `EventConnections` as the original is), typed field values. [`write::apply`] replays a
//!   project's log on its base partition.
//! - [`schema`]: field types from the partitions themselves, by `dbxcatalogue`'s rules (shapes, whole or decimal
//!   numbers, the texts a field takes): a value is written only in a type the data shows, never guessed.
//! - [`diff`]: a field-level comparison of two partitions (what changed, instance by instance).
//! - [`export`] (round E3): a project built into a NEW multiplayer level, staged - [`mpconvert`] (a campaign level
//!   made a multiplayer one: its scripting out, a mode's layer grafted from a donor level and placed) and
//!   [`culltree`] (moved objects given new cull tree leaves); the `mapexport` binary drives it.
#![deny(unsafe_code)]
#![allow(
    clippy::many_single_char_names,
    reason = "tree walks: d (the file), e / x / y (elements), a / b (the two files compared), t (a type)"
)]

pub mod culltree;
pub mod diff;
pub mod export;
pub mod mpconvert;
pub mod ops;
pub mod project;
pub mod schema;
pub mod write;

pub use ops::{FieldValue, Op, Operation, Placement, Rows, SinkUnit};
pub use project::{MapProject, Profile};
pub use schema::{FieldType, Schema};
pub use write::{LevelWriteError, LevelWriter, apply};

/// The namespace of the editor's UUID v5 guids (`dbxbin.new_guid(NAMESPACE, project id, operation, source)`).
pub const NAMESPACE: &str = "opfor:mapedit";

/// A new entity's guid: `dbxbin.new_guid` over the project, the operation's number and the source guid (UUID v5, upper
/// case as DICE writes them), so a replay of the log gives the same guids.
#[must_use]
pub fn new_guid(project: &str, operation: u64, source: &str) -> String {
    bc2_stockmp::guid::new_guid(
        NAMESPACE,
        &[project, &operation.to_string(), &source.to_ascii_uppercase()],
    )
}

/// The rows of an identity transform at a position.
#[must_use]
pub fn rows_at(p: [f32; 3]) -> Rows {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], p]
}

/// The rows (right, up, forward, position) of a 4 x 4 matrix stored row by row, as the scene keeps it.
#[must_use]
pub fn rows_of(m: &[f64; 16]) -> Rows {
    std::array::from_fn(|r| std::array::from_fn(|c| m.get(r * 4 + c).copied().unwrap_or(0.0) as f32))
}

/// The 4 x 4 matrix (row by row) of rows, each row's fourth value 0 and the position's 1 (as the partitions store a
/// `LinearTransform` and the scene reads it).
#[must_use]
pub fn matrix_of(rows: &Rows) -> [f64; 16] {
    let mut m = [0.0; 16];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            if let Some(slot) = m.get_mut(r * 4 + c) {
                *slot = f64::from(*v);
            }
        }
    }
    m[15] = 1.0;
    m
}

#[cfg(test)]
mod tests {
    #[test]
    fn guids_are_deterministic() {
        let a = super::new_guid("P", 3, "38e8002f-4e30-4f6d-ada2-52a7f1e1d565");
        assert_eq!(a, super::new_guid("P", 3, "38E8002F-4E30-4F6D-ADA2-52A7F1E1D565"));
        assert_ne!(a, super::new_guid("P", 4, "38E8002F-4E30-4F6D-ADA2-52A7F1E1D565"));
        assert_eq!(a.len(), 36);
        assert_eq!(a, a.to_ascii_uppercase());
        let r = super::rows_at([1.0, 2.0, 3.0]);
        assert_eq!(super::rows_of(&super::matrix_of(&r)), r);
    }
}
