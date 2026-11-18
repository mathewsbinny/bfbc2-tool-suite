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
//! - [`mesh`]: Xbox 360 mesh vertices to the PC's (`bfbc2-console-convert/mesh.py`'s `convert`, its vertex loop):
//!   element by element, the packed normals and colours decoded to halves (round E3g).
//! - [`havok`]: Xbox 360 Havok 5.5.0 packfiles to the PC's (`bfbc2-console-convert/havok.py`'s
//!   `Packfile.convert`): the field walk over the game's class layouts (round E3g).
//! - [`granny`]: Xbox 360 Granny resources to the PC's (`bfbc2-anim-tools/granny.py`'s `to_pc`), DICE's bone
//!   table renumbered onto another skeleton; many on all cores.
//! - [`dbxport`]: BF1943's DBX partitions in BFBC2's schema (`bfbc2-bf1943-import/dbxport.py`'s `port`): the
//!   template choice, alignment, reference rules and value passes over trees kept here (round E4d).
#![deny(unsafe_code)]

pub mod audio;
pub mod dbxport;
pub mod granny;
pub mod havok;
pub mod mesh;
pub mod texture;
