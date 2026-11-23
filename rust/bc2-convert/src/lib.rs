//! The converter pipeline's hot spots in pure Rust (`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 3.8: the
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
//! - [`texcheck`] (round F0): a PC texture's levels and a level decoded to pixels (`texture.py`'s `levels` /
//!   `decode`); [`hkclasses`] (round F0): the Havok class layouts of BFBC2Game.exe (`havok.py`'s `classes`).
//! - round F2b, `bfbc2-bf1943-import`'s remaining Python: [`mopp`] (`mopp.py`'s MOPP queries and ray casts),
//!   [`havokport`] (`havokport.py`: the 4.6.1 restructure, the material table, the shape subparts, the physics
//!   materials), [`emitteratlas`] (`emitteratlas.py`'s build, proof and tree comparison), [`dbxcatalogue`]
//!   (`dbxcatalogue.build`'s pass), [`meshport`] (`Porter.pc_layout`'s search) and [`pyfmt`] (the Python number
//!   rules they reproduce).
//! - Round F2a: [`terrain`] (`terrain.py`'s console terrain, water and visual terrain / water; `soot.py`),
//!   [`meshconv`] (`mesh.py`'s `console_layouts`, `decode`, `relayout`), `havok::convert_wrapper`,
//!   `texcheck::check` (`texture.check`'s statistics), [`texwrite`] (`bfbc2-model-tools/texwrite.py`'s encoders),
//!   [`skinmesh`] (`skinmesh.py`'s vertex work) and [`granny_edit`] (`granny.py`'s writing side).
//! - Round `OR3b`: [`shaderlevels`] (`shaderdb.py`'s level cover index and its cache, `level_scan`): the game copy's
//!   level shader databases read as a whole.
//! - Round GR3: [`meshview`] (`bfbc2-mesh-viewer/meshview.py`'s index and its cache, `load_mesh`, `stack_models` and
//!   `render`, pixel for pixel), for the map editor's asset preview.
#![deny(unsafe_code)]

pub mod audio;
pub mod dbxcatalogue;
pub mod dbxport;
pub mod emitteratlas;
pub mod granny;
pub mod granny_edit;
pub mod havok;
pub mod havokport;
pub mod hkclasses;
pub mod mesh;
pub mod meshconv;
pub mod meshport;
pub mod meshview;
pub mod mopp;
pub mod pyfmt;
pub mod shaderlevels;
pub mod skinmesh;
pub mod terrain;
pub mod texcheck;
pub mod texqa;
pub mod texture;
pub mod texwrite;
pub mod wavport;
