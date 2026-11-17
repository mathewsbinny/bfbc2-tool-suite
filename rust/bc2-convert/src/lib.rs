//! The converter pipeline's hot spots in pure Rust (`bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` 3.8: the
//! pipeline stays in Python; what a profile shows the time goes to is ported here, one at a time, measured in
//! `rust/BENCHMARKS.md`).
//!
//! Each module is a port of a Python reference that stays the fallback; a port replaces it only when it gives the
//! same bytes on every real input the pipeline converts. This crate has no Python types: the bindings live in
//! `bc2-py`.
//!
//! - [`texture`]: Xbox 360 textures to the PC's (`bfbc2-console-convert/texture.py`'s `convert`): the Xenos
//!   untiling, the byte swap and the three decoded console formats.
//! - [`audio`]: EA-XAS v1 audio (`bfbc2-console-convert/audio.py`'s `xas_encode_frames` / `xas_decode_frames`):
//!   the PC's in-memory sound codec, encoded group by group on all cores.
#![deny(unsafe_code)]

pub mod audio;
pub mod texture;
