//! Battlefield: Bad Company 2 file formats in pure Rust: the BFBC2 Tool Suite's shared core
//! (`bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` 3.8).
//!
//! Each format is a port of the suite's Python reference implementation and replaces it only when it gives the
//! same answers on the same test corpora. This crate has no Python types: the bindings live in `bc2-py`.
//!
//! - [`dbx`]: binary `.dbx` partitions (`bfbc2-dbx-tools/FORMAT.md` sections 1-3), a port of
//!   `core/bfbc2core/dbx.py`: the element tree, the readable XML (identical to `DbxEdit xml`), values and a writer.
//! - [`fbrb`]: `.fbrb` bundles, a port of `core/bfbc2core/fbrb.py`: the header's entries and any entry's data.
//! - [`terrain`]: terrain height fields, a port of `core/bfbc2core/terrain.py` (heights, exact height at a point,
//!   a mesh) with the tail's known layout.
//! - [`water`]: water meshes and the shader of each triangle, a port of `core/bfbc2core/water.py`.
//! - [`havok`]: Havok packfiles and a blueprint's collision triangles, a port of NavKit's `hkpack.py` and
//!   `levelobjects.triangles`.
//! - [`shaderdb`]: per-level shader databases (PC Dx9 / Dx10 / Dx11, Xbox 360), a port of the reader and writer of
//!   `bfbc2-shader-tools/shaderdb.py`.
//! - [`culltree`]: static cull trees' and level partitions' element streams and the reference levels' discovery, a
//!   port of the hot parts of `bfbc2-culltree/culltree.py`.
//!
//! ```
//! use bc2_formats::dbx::{self, DbxFile, Element};
//!
//! let mut file = DbxFile::default();
//! let name = Element::text(&mut file, "field", &[("name", "Name")], "MP_005");
//! file.roots.push(name);
//! let bytes = dbx::write(&file)?;
//! assert_eq!(dbx::to_xml(&bytes)?, "<?xml version=\"1.0\"?>\r\n<field name=\"Name\">MP_005</field>\r\n");
//! assert_eq!(dbx::parse(&bytes)?.roots.len(), 1);
//! # Ok::<(), dbx::DbxError>(())
//! ```
#![deny(unsafe_code)]

pub mod culltree;
pub mod dbx;
pub mod fbrb;
pub mod havok;
pub mod shaderdb;
pub mod terrain;
pub mod water;
pub mod zlib;

mod bytes;

/// The number of worker threads the batch functions ([`dbx::parse_all`], [`dbx::to_xml_all`], [`fbrb::read_many`]) use: rayon's
/// global pool, `RAYON_NUM_THREADS` or else one per logical core.
#[must_use]
pub fn threads() -> usize {
    rayon::current_num_threads()
}

#[cfg(test)]
mod tests {
    #[test]
    fn at_least_one_thread() {
        assert!(super::threads() >= 1);
    }
}
