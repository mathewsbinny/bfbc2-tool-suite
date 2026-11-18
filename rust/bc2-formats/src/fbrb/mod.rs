//! `.fbrb` bundles, ported from `core/bfbc2core/fbrb.py` (the layout `bfbc2-fbrb-tool/FbrbTool.cs` reads and
//! writes): the header's entries and any entry's data, without unpacking.
//!
//! Layout: `FbRB`, a big-endian `u32` header length, the header (gzip), then the payload (one gzip stream when the
//! header's zipped byte is set, else stored). The header: `u32` 2, `u32` string-table length, the strings, `u32`
//! entry count, per entry six big-endian `u32`s (name offset, flags, payload offset, size, size2, type offset; the
//! string offsets from the start of the table), the zipped byte, `u32` payload length.
//!
//! | Python (`bfbc2core.fbrb`) | here |
//! |---|---|
//! | `entries(path)` | [`entries_path`] ([`read_header`] on any reader) |
//! | `names(path)` | [`Header::names`] |
//! | `read(path, names)` | [`read_path`] ([`read`] on any reader), [`read_many`] for many bundles on all cores |
//! | `build(items, zipped)` | [`build`] (a test writer) |
//! | `pack(folder, out)` (FbrbTool's `pack`) | [`pack_path`], [`pack_dir`], [`pack_many`]; [`pack`] from entries in memory |
//! | `unpack(bundle, out)` (FbrbTool's `unpack`) | [`unpack_path`], [`unpack_bytes`] (every bundle as FbrbTool unpacks it: [`tool`]'s engine) |
//! | `Entry`, `Entry.deleted`, `DELETED` | [`Entry`], [`Entry::is_deleted`], [`DELETED`] |
//! | `ValueError` / `KeyError` / `OSError` | [`FbrbError`] |
//!
//! FbrbTool's command line in process ([`tool`], round E3k: `unpack`, `pack`, `list`, `big`, FbrbTool's files, messages
//! and exit codes), its gzip streams read as .NET's `GZipStream` reads them ([`netgzip`]).
//!
//! New here: [`extract`] / [`extract_path`] / [`extract_many`], every entry's data at once; the bundle cache
//! ([`cache`], round E3e): the path functions keep headers and decompressed payloads, in the process and on disk, so a
//! bundle read again is not decompressed again ([`read_path_uncached`] / [`extract_path_uncached`] read without it).
//!
//! The two gzip streams are read by the rules of the reference's two standard-library readers (`gzip.decompress`
//! for the header, `zlib.decompressobj` fed 1 MiB at a time for the payload: see the `gzip` module), so the same
//! damaged bundles are refused. As in the reference the payload is read only up to the last byte the entries asked
//! for need: damage after it is not seen.
//!
//! ```
//! use bc2_formats::fbrb;
//!
//! let bundle = fbrb::build(&[("Levels/X.dbx", "<non-resource>", b"{binary}..."), ("Gone", fbrb::DELETED, b"")], true);
//! let header = fbrb::read_header(&mut bundle.as_slice())?;
//! assert!(header.entries[1].is_deleted());
//! assert!(header.names().contains_key("levels/x.dbx"));
//! let (_, data) = fbrb::extract(&mut std::io::Cursor::new(&bundle))?;
//! assert_eq!(data[0], b"{binary}...");
//! # Ok::<(), fbrb::FbrbError>(())
//! ```

mod build;
pub mod cache;
mod error;
mod gzip;
mod header;
pub mod netgzip;
mod pack;
mod read;
pub mod tool;
mod unpack;

pub use build::build;
pub use error::{FbrbError, Result};
pub use header::{DELETED, Entry, Header, MAGIC, parse_header_fields, read_header};
pub use pack::{
    Layout, Manifest, ManifestLine, PackError, PackItem, PackResult, PackStats, Padding, SourceRow, decode_text,
    entry_path, pack, pack_dir, pack_many, pack_path, pack_rows, parse_manifest,
};
pub use read::{
    entries_path, extract, extract_many, extract_path, extract_path_uncached, read, read_many, read_path,
    read_path_uncached, read_payload, stream_payload,
};
pub use unpack::{UnpackError, UnpackStats, unpack_bytes, unpack_path};
