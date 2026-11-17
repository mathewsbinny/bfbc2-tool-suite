//! The Xbox 360 (Xenos) shader translator in pure Rust: a port of `bfbc2-shader-tools/xenos.py` (the XDK shader
//! container and its microcode, decoded) and `xenos_hlsl.py` (a decoded program as HLSL for BFBC2's Dx10 / Dx11
//! renderers), proven to give the same decoded trees and the same HLSL text, byte for byte, on every program of
//! Battlefield 1943's and BFBC2's Xbox shader databases (`rust/BENCHMARKS.md`, "E3f").
//!
//! The Python stays the reference and the fallback: the bindings (`bc2-py`, module `bc2_native`) give
//! `xenos.parse_shader` and `xenos_hlsl.translate` this crate's results, and hand anything this crate leaves to the
//! Python ([`xenos::XenosError::Malformed`], [`hlsl::TranslateError::Unsupported`]) back to it. No Python types here.
//!
//! - [`xenos`]: [`xenos::parse`], a shader blob as a [`xenos::Shader`] (container, constant table, literals, header,
//!   control flow, ALU and fetch instructions).
//! - [`hlsl`]: [`hlsl::translate`] with an [`hlsl::Interface`] (what the PC side provides), [`hlsl::translate_all`]
//!   many programs on all cores.
//! - [`translate_blobs`]: decode and translate many blobs on all cores, nothing shared between them.
//!
//! Built on it later: the Dx9 port and the BF1943 weapons' and soldiers' translations (queued work).
#![deny(unsafe_code)]
// A port keeps the Python's shape: its records' flag fields, its bit-field names (w0, w1, w2 ...) and its long
// straight-line decoders and emitter methods, so the two read side by side.
#![allow(
    clippy::struct_excessive_bools,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::too_many_lines
)]

pub mod hlsl;
pub mod pyrepr;
pub mod xenos;

use rayon::prelude::*;

/// Why a blob has no translation: its decoding or its translation failed.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The decoder's.
    #[error(transparent)]
    Decode(#[from] xenos::XenosError),
    /// The translator's.
    #[error(transparent)]
    Translate(#[from] hlsl::TranslateError),
}

/// Decodes and translates each `(blob, is_pixel, interface)` on all cores, in input order.
#[must_use]
pub fn translate_blobs(items: &[(&[u8], Option<bool>, &hlsl::Interface)]) -> Vec<Result<String, Error>> {
    items
        .par_iter()
        .map(|(blob, is_pixel, iface)| Ok(hlsl::translate(&xenos::parse(blob, *is_pixel)?, iface)?))
        .collect()
}
