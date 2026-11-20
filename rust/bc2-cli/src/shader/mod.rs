//! Round O5's `shader` group: the shader tools' command lines (`bfbc2-shader-tools`).
//!
//! - `bfbc2 shaderdb`: every subcommand (info, list, textures, merge, cover, roundtrip, find, needs) over
//!   `bc2_formats::shaderdb`, with this round's port of the merge (`merge`) and of `level_meshes`.
//! - `bfbc2 renderqa` / `bfbc2 xdiff`: their command lines up to the drawing (the parsers and the errors before it); a
//!   run that draws (Direct3D 11 through `shaderview.exe`, NumPy's measures, Pillow's sheets, `xenos_exec.py`'s
//!   interpreter) is handed to the Python.

use crate::pyproc::Tool;

// argparse's loop over the arguments by index (lengths checked before each index)
#[allow(clippy::indexing_slicing)]
mod argp;
// the reference's indexes (records within the sections the reader checked; slices after their lengths)
#[allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    reason = "program indices below MAX_PROGRAMS (checked first) fit u16"
)]
mod merge;
mod renderqa;
#[allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    reason = "indexes after their lengths are checked; sizes as Python's ints"
)]
mod shaderdb;
mod xdiff;

/// The group's tools.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "shaderdb",
        script: "bfbc2-shader-tools\\shaderdb.py",
        log: Some("shaderdb"),
        run: shaderdb::run,
    },
    Tool {
        name: "renderqa",
        script: "bfbc2-shader-tools\\renderqa.py",
        log: Some("renderqa"),
        run: renderqa::run,
    },
    Tool {
        name: "xdiff",
        script: "bfbc2-shader-tools\\xdiff.py",
        log: None,
        run: xdiff::run,
    },
];
