//! The format tools' command lines as a library (round PR1b-2, audit AU5 ARC-2): the code behind `bfbc2.exe fbrbtool`
//! and `bfbc2.exe dbxedit` (and `bc2_native`'s), with the files, messages and exit codes of the retired FbrbTool.exe and
//! DbxEdit.exe. The formats they read and write are `bc2-formats`'; the files they write go through `bc2-gamefs`.
//!
//! | tool | here |
//! |---|---|
//! | FbrbTool (`unpack`, `pack`, `list`, `big`) | [`fbrbtool`]; [`unpack_path`] / [`unpack_bytes`], its unpack engine as a library call |
//! | DbxEdit (`xml`, `roundtrip`, `edit`, `import`, `check`, `fixslots`) | [`dbxedit`] |
//! | LevelDiff (two `.dbx` files compared; round PR1b-2-rest) | [`leveldiff`] |

pub mod dbxedit;
pub mod fbrbtool;
pub mod leveldiff;
mod unpack;

pub use unpack::{UnpackError, UnpackStats, unpack_bytes, unpack_path};
