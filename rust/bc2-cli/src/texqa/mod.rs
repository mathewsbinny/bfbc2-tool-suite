//! Round O4's `texqa` group: the converted textures' QA command line (`bfbc2-console-convert\texqa.py`), its
//! decoding, measures and rows in Rust (NumPy's summation orders reproduced: `measure`), what only Python can answer
//! handed to it (`run`'s module text).

use crate::pyproc::Tool;

// NumPy's arrays as flat vectors indexed by the shapes the decoder made them with (texels in 4-byte chunks, loops
// over channels and blocks by index as the reference's); the reference's one-letter names (w, h, c, x, y ...); counts
// as floats (texel counts far below 2^52); floats compared exactly where Python does; the reference's long functions
#[allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::chunks_exact_to_as_chunks,
    clippy::needless_range_loop,
    clippy::too_many_lines
)]
mod analyse;
#[allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::chunks_exact_to_as_chunks,
    clippy::needless_range_loop,
    clippy::too_many_lines
)]
mod decode;
#[allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::chunks_exact_to_as_chunks,
    clippy::needless_range_loop
)]
mod measure;
#[allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::too_many_lines
)]
mod run;

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "texqa",
    script: "bfbc2-console-convert\\texqa.py",
    log: Some("texqa"),
    run: run::main,
}];
