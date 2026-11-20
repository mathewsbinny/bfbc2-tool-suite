//! The map editor's game-accurate view (`bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` 3.11, round R5): a level's
//! meshes and terrain drawn as BFBC2's Direct3D 11 renderer draws them - DICE's own compiled shaders from the level's
//! `Dx11_Single.res`, their parameter blocks' textures, constants and sampler states, the programs' render states,
//! the level's weather light - off screen, then shown in the editor.
//!
//! It is the suite's render harness (`bfbc2-shader-tools/renderqa.py` and `native/shaderview.cpp`) in Rust:
//!
//! - [`files`]: game files by name in the unpacked bundles a level is prepared from (`fbrb.manifest`).
//! - [`dxbc`]: what a compiled shader declares (its `$Globals` and resource bindings) and whether it discards.
//! - [`material`]: a mesh's draws with a database, as the engine picks them (`renderqa.prepare`).
//! - [`weather`]: the light and post-process values of a level's active weather state (`renderqa.light`).
//! - [`placement`]: the level partition's shader-set overrides and model-group members.
//! - [`water`]: the visual water drawn with its slots' entries.
//! - [`terrain`]: terrain patches drawn with their `PatchMaskShaders` entries on the level's height map and mask atlas.
//! - [`frame`]: the camera, the engine constants and the draw list of a frame.
//! - [`post`]: DICE's own post-process shaders as full-screen passes; [`tonemap`]: the harness's CPU tonemap.
//! - [`world`]: a level's whole drawable world (objects, group members, terrain, strips, water, post shaders).
//! - [`simple`]: the same meshes as plain geometry with their alpha-tested parts and diffuse tints, for the editor's
//!   simplified view (round SV1).
//! - [`job`]: a frame as a job for the C++ harness (`shaderview`), for the pixel-for-pixel proofs.
//! - `d3d` (Windows): the Direct3D 11 device that draws a frame (and its post passes) and reads it back.
//!
//! Nothing is written to the game copy; every file is read from OPFOR's `work` folder.

#![allow(clippy::cast_precision_loss, reason = "pixel, texel and grid sizes, far below 2^23")]
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "vector and colour maths: x, y, z, r, g, b; the reference's names"
)]
#![allow(
    clippy::too_many_lines,
    reason = "a draw's set-up reads top to bottom, as the harness makes it"
)]

pub mod dxbc;
pub mod files;
pub mod frame;
pub mod job;
pub mod live;
pub mod material;
pub mod placement;
pub mod post;
pub mod simple;
pub mod terrain;
pub mod tonemap;
pub mod water;
pub mod weather;
pub mod world;

#[cfg(windows)]
pub mod d3d;

/// One `f16` (IEEE half) as `f32`.
#[must_use]
pub fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let frac = f32::from(h & 0x3ff);
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 => {
            if frac == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + frac / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// A little-endian `u32` at `at`, or 0 past the end.
#[must_use]
pub fn u32_at(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4)
        .and_then(|s| <[u8; 4]>::try_from(s).ok())
        .map_or(0, u32::from_le_bytes)
}

/// A little-endian `f32` at `at`, or 0 past the end.
#[must_use]
pub fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_bits(u32_at(b, at))
}

/// A zero-terminated latin-1 string at `at`.
#[must_use]
pub fn cstr_at(b: &[u8], at: usize) -> String {
    let s = b.get(at..).unwrap_or_default();
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    s.iter().take(end).map(|&c| char::from(c)).collect()
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::chunks_exact_to_as_chunks,
    clippy::trivially_copy_pass_by_ref,
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::*;

    #[test]
    fn halves() {
        assert!((half(0x3c00) - 1.0).abs() < f32::EPSILON);
        assert!((half(0xc000) + 2.0).abs() < f32::EPSILON);
        assert!((half(0x3555) - 0.333_251_95).abs() < 1e-6);
        assert!(half(0x0001) > 0.0);
        assert!(half(0x7e00).is_nan());
    }
}
