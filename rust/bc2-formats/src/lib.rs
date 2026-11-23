//! Battlefield: Bad Company 2 file formats in pure Rust: the BFBC2 Tool Suite's shared core
//! (`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 3.8).
//!
//! Each format is a port of the suite's Python reference implementation and replaces it only when it gives the
//! same answers on the same test corpora. This crate has no Python types: the bindings live in `bc2-py`.
//!
//! - [`apt`]: EA APT UI movies (menus, HUDs), a port of the dump and instruction records of
//!   `bfbc2-console-readers/apt/apt.py`.
//! - [`aptmodel`]: the menu tools' movie model (`bfbc2-menu-tools/aptgraft.py`: parse, layouts, writes in either
//!   byte order, leftovers, roundtrip); [`aptscan`]: `aptcheck`'s rules and `aptrefs`' scan over the decoded records.
//! - [`dbx`]: binary `.dbx` partitions (`bfbc2-dbx-tools/FORMAT.md` sections 1-3), a port of
//!   `core/bfbc2core/dbx.py`: the element tree, the readable XML (identical to `DbxEdit xml`), values and a writer.
//! - [`dbxedit`]: DbxEdit's model and commands in process (edits, import, export, check), a
//!   port of `bfbc2-dbx-tools/DbxEdit.cs` giving its files, messages and exit codes.
//! - [`fbrb`]: `.fbrb` bundles, a port of `core/bfbc2core/fbrb.py`: the header's entries and any entry's data.
//! - [`granny`]: Granny skeletons and animations (PC and Xbox 360), a port of the reader of
//!   `bfbc2-anim-tools/granny.py`: the tables, the object tree, DICE's per-bone table.
//! - [`terrain`]: terrain height fields, a port of `core/bfbc2core/terrain.py` (heights, exact height at a point,
//!   a mesh) with the tail's known layout.
//! - [`terrain_codec`]: the main height-field block and the material map decoded whole and written again (round E5:
//!   depth map, min / max tree, lower envelope; DICE's 30 terrains byte for byte).
//! - [`visual_terrain`]: the `VisualTerrain` decoded whole and written again (round TG2: DICE's 30 byte for byte;
//!   the generated terrain's patch maps and renamed patch shaders).
//! - [`itexture`]: PC `ITexture` resources written from pixels (round TG2: DXT1 and L8 levels, the mip chain; the
//!   generated terrain's colour / occlusion map, E5-rest's re-bake).
//! - [`water`]: water meshes and the shader of each triangle, a port of `core/bfbc2core/water.py`; the visual water
//!   decoded whole and written again (round TG2).
//! - [`havok`]: Havok packfiles and a blueprint's collision triangles, a port of NavKit's `hkpack.py` and
//!   `levelobjects.triangles`.
//! - [`shaderdb`]: per-level shader databases (PC Dx9 / Dx10 / Dx11, Xbox 360), a port of the reader and writer of
//!   `bfbc2-shader-tools/shaderdb.py`.
//! - [`culltree`]: static cull trees' and level partitions' element streams and the reference levels' discovery, a
//!   port of the hot parts of `bfbc2-culltree/culltree.py`.
//! - Round F0, the formats that had no Rust (each beside its Python reference): [`hpd`] (path data, NavKit's
//!   `hpdtool.py`) and [`kymesh::write`]; [`loc`] (localized texts, `bc2loc.py`) and [`bigf`] (BIGF archives);
//!   [`stfs`] / [`xiso`] (Xbox 360 content packages and discs); [`wave`] (Wave resources, EA stream framing,
//!   `audio.py`); [`meshdata`] (the `MeshData` header and the mesh sets, `mesh.py` / `meshset.py`).
//! - Round F3, the rest of the format code: [`aimanim`] (generated aim poses, `aimanim.py`), [`audioframe`]
//!   (`audio.py`'s EA-XMA parts, XMA2 RIFF and EA-XAS blocks) and [`hpdlayout`] (`hpdtool.mesh_layout` /
//!   `mesh_coverage`).
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

pub mod aimanim;
pub mod apt;
pub mod aptmodel;
pub mod aptscan;
pub mod audioframe;
pub mod bf1943;
pub mod bigf;
pub mod culltree;
pub mod dbx;
pub mod dbxedit;
pub mod fbrb;
pub mod granny;
pub mod guid;
pub mod havok;
pub mod hpd;
pub mod hpdlayout;
pub mod itexture;
pub mod kymesh;
pub mod loc;
pub mod meshdata;
pub mod pe;
pub mod restart;
pub mod shaderdb;
pub mod stfs;
pub mod terrain;
pub mod terrain_codec;
pub mod varint;
pub mod visual_terrain;
pub mod water;
pub mod wave;
pub mod xiso;
pub mod xmlinst;
pub mod zlib;

mod bytes;

/// The number of worker threads the batch functions ([`dbx::parse_all`], [`dbx::to_xml_all`],
/// `bc2_gamefs::bundles::read_many`) use: rayon's global pool, `RAYON_NUM_THREADS` or else one per logical core.
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
