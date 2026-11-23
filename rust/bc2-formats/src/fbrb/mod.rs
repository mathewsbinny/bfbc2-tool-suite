//! `.fbrb` bundles, ported from `core/bfbc2core/fbrb.py` (the layout FbrbTool read and wrote; FbrbTool.cs was retired
//! in round R4, 2026-11-18): the header's entries and any entry's data, without unpacking.
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
//! | `read(path, names)` | [`read_path_uncached`] ([`read`] on any reader); `bc2_gamefs::bundles::read_path` / `read_many` through the bundle cache |
//! | `build(items, zipped)` | [`build`] (a test writer) |
//! | `pack(folder, out)` (FbrbTool's `pack`) | [`pack_dir`]; [`pack_rows_into`] to any writer (streamed: the payload is never held); [`pack`] from entries in memory; `bc2_gamefs::bundles::pack_path` / `pack_many` to a file |
//! | `unpack(bundle, out)` (FbrbTool's `unpack`) | `bc2_tools::unpack_path` (FbrbTool's engine, `bc2_tools::fbrbtool`) |
//! | `Entry`, `Entry.deleted`, `DELETED` | [`Entry`], [`Entry::is_deleted`], [`DELETED`] |
//! | `ValueError` / `KeyError` / `OSError` | [`FbrbError`] |
//!
//! FbrbTool's command line is `bc2_tools::fbrbtool` (round PR1b-2; its gzip streams read as .NET's `GZipStream` reads
//! them: [`netgzip`]). The bundle cache (round E3e) is `bc2_gamefs::bundles`, built on [`stream`]: the payload's
//! chunks, gzip member and entry collector.
//!
//! New here: [`extract`] / [`extract_path_uncached`], every entry's data at once.
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
mod error;
mod gzip;
mod header;
pub mod netgzip;
mod pack;
mod read;

pub use build::build;
pub use error::{FbrbError, Result};
pub use header::{DELETED, Entry, Header, MAGIC, parse_header_fields, read_header};
pub use pack::{
    EMPTY_GZIP_MEMBER, Layout, Manifest, ManifestLine, PackError, PackItem, PackResult, PackStats, Padding, SourceRow,
    decode_text, entry_path, pack, pack_dir, pack_rows, pack_rows_into, parse_manifest,
};
pub use read::{entries_path, extract, extract_path_uncached, read, read_path_uncached, read_payload, stream_payload};

/// The payload's stream pieces the bundle cache (`bc2_gamefs::bundles`) builds on: the chunk the reference reads, the
/// gzip member fed chunk by chunk, the entries collected as the payload streams past.
pub mod stream {
    pub use super::gzip::{Member, zlib_header};
    pub use super::pack::{StreamError, dir_rows, lines, pack_rows_stream, token};
    pub use super::read::{CHUNK, Collector, read_full};
}

/// The length of the gzip member header at the start of `data` as zlib's inflate reads it (`zlib.decompressobj(31)`:
/// reserved flags refused, the header checksum checked); `Ok(None)` when not all of it is there.
///
/// # Errors
/// [`FbrbError::Gzip`] for a header zlib refuses.
pub fn gzip_member_header(data: &[u8]) -> Result<Option<usize>> {
    gzip::zlib_header(data)
}
