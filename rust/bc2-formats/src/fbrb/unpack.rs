//! FbrbTool's `unpack` in process (round E3j): every entry written under a folder by its archive name, and the
//! `fbrb.manifest` FbrbTool writes (`bfbc2-fbrb-tool/FbrbTool.cs`, `Unpack` and `LayoutTokens`), the same bytes.
//!
//! Only bundles every reader agrees on are unpacked here: a plain single-member gzip header (zero bytes may follow
//! it) and payload (nothing may follow it), ASCII names of plain folder and file names (no `.` or `..` part, no
//! part ending in a dot or space, no character Windows refuses, no device name, the whole path under 248
//! characters), no two entries on one file (Windows compares names without case) and no file where another entry
//! needs a folder, every entry inside the payload. Anything else is [`UnpackError::Unsupported`]: the caller runs
//! FbrbTool.exe, the reference, which unpacks it or says why not (`bfbc2core.fbrb.unpack` does).

use std::collections::{BTreeSet, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::gzip::gunzip_plain;
use super::header::{DELETED, Entry, MAGIC, parse_header_fields};

/// Why a bundle was not unpacked in process.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UnpackError {
    /// The bundle is one this port leaves to FbrbTool.exe (the text says what): damaged, unusual or with names
    /// .NET would write elsewhere than a plain join.
    #[error("not unpacked in process ({0}): FbrbTool.exe decides")]
    Unsupported(String),
    /// A file or folder could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder.
        path: PathBuf,
        /// The error.
        #[source]
        source: io::Error,
    },
}

/// What [`unpack_path`] did: FbrbTool's `unpacked N entries (P bytes) ...; D of them *deleted* markers` numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnpackStats {
    /// Entries in the bundle, markers included.
    pub entries: usize,
    /// The decompressed payload's length.
    pub payload_len: u64,
    /// *deleted* markers (listed in the manifest only).
    pub deleted: usize,
}

fn unsupported(why: impl Into<String>) -> UnpackError {
    UnpackError::Unsupported(why.into())
}

fn io_at(path: &Path) -> impl FnOnce(io::Error) -> UnpackError + '_ {
    move |source| UnpackError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// FbrbTool's `IsDeletedMarker`.
fn is_marker(e: &Entry) -> bool {
    e.type_name == DELETED && e.size == 0
}

const DEVICES: [&str; 4] = ["con", "prn", "aux", "nul"];

/// Whether `part` is a folder or file name .NET's `Path.GetFullPath` and Windows keep exactly as it is.
fn plain_part(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or("").trim_end().to_ascii_lowercase();
    let device = DEVICES.contains(&stem.as_str())
        || ((stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.len() == 4
            && stem.as_bytes().get(3).is_some_and(u8::is_ascii_digit));
    !part.is_empty()
        && part != "."
        && part != ".."
        && !part.ends_with(['.', ' '])
        && !part.starts_with(' ')
        && !device
        && part
            .bytes()
            .all(|b| b.is_ascii() && b >= 0x20 && !matches!(b, b'<' | b'>' | b':' | b'"' | b'|' | b'?' | b'*'))
}

/// FbrbTool's `LayoutTokens`: the padded layout of a stored payload, "" for none.
fn layout_tokens(entries: &[Entry], payload: &[u8], payload_field: u32, header_region: u64) -> String {
    let nd: Vec<&Entry> = entries.iter().filter(|e| !is_marker(e)).collect();
    let (Some(last), true) = (nd.last(), nd.len() >= 2) else {
        return String::new();
    };
    let (mut align, mut gap) = (true, true);
    for w in nd.windows(2) {
        let [prev, cur] = w else { continue };
        let end = prev.end();
        let at = u64::from(cur.offset);
        let filled = at >= end
            && at <= payload.len() as u64
            && payload
                .get(usize::try_from(end).unwrap_or(usize::MAX)..usize::try_from(at).unwrap_or(0))
                .is_some_and(|g| g.iter().all(|&b| b == 0xBF));
        align &= filled && at.is_multiple_of(4096) && at - end < 4096;
        gap &= filled && at - end == 64;
    }
    if !align && !gap {
        return String::new();
    }
    let hdr = if header_region.is_multiple_of(4096) {
        4096
    } else if header_region.is_multiple_of(4) {
        4
    } else {
        1
    };
    let mut s = format!(" layout={} hdr={hdr}", if align { "align4096" } else { "gap64" });
    // C#: a uint field against the int length (as longs); offset + size2 wraps as uint arithmetic
    if u64::from(payload_field) != payload.len() as u64 && payload_field == last.offset.wrapping_add(last.size2) {
        s.push_str(" plen=size2");
    }
    if nd.iter().all(|e| e.size >= 64 && e.size2 == e.size - 64) {
        s.push_str(" size2=minus64");
    }
    s
}

/// The manifest text FbrbTool's `unpack` writes (lines end in CR LF).
fn manifest(entries: &[Entry], zipped: bool, tokens: &str) -> String {
    let mut m = format!(
        "# fbrb.manifest - entry order matters. name|type|flags(hex)|size2|size  zipped={}{tokens}\r\n",
        u8::from(zipped)
    );
    for e in entries {
        // writing to a String cannot fail
        let _ = write!(
            m,
            "{}|{}|{:08X}|{}|{}\r\n",
            e.name, e.type_name, e.flags, e.size2, e.size
        );
    }
    m
}

/// A file to write: its path and its data.
type Planned<'a> = (PathBuf, &'a [u8]);

/// Where each live entry goes under `root` (absolute), with its data, and the number of *deleted* markers; a
/// bundle with a name .NET would write elsewhere, or entries that clash, is [`UnpackError::Unsupported`].
fn plan<'a>(entries: &[Entry], payload: &'a [u8], root: &Path) -> Result<(Vec<Planned<'a>>, usize), UnpackError> {
    let root_len = root.as_os_str().len();
    let mut files: Vec<Planned<'a>> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    let mut deleted = 0;
    for e in entries {
        if e.name.contains(char::REPLACEMENT_CHARACTER) || e.type_name.contains(char::REPLACEMENT_CHARACTER) {
            return Err(unsupported(format!("a name or type outside ASCII: {}", e.name)));
        }
        if is_marker(e) {
            deleted += 1;
            continue;
        }
        let parts: Vec<&str> = e.name.split(['/', '\\']).collect();
        if !parts.iter().all(|p| plain_part(p)) {
            return Err(unsupported(format!("entry name {:?}", e.name)));
        }
        // the whole path under .NET Framework's limits (no long paths in FbrbTool: 260 for a file, 248 for its
        // folder), kept under the smaller
        if root_len + 1 + e.name.len() >= 248 {
            return Err(unsupported(format!(
                "the path of {:?} is 248 characters or more",
                e.name
            )));
        }
        let lower = parts.join("\\").to_ascii_lowercase();
        if !seen.insert(lower.clone()) {
            return Err(unsupported(format!("two entries on one file: {:?}", e.name)));
        }
        let mut at = String::new();
        for p in parts.iter().take(parts.len().saturating_sub(1)) {
            if !at.is_empty() {
                at.push('\\');
            }
            at.push_str(&p.to_ascii_lowercase());
            dirs.insert(at.clone());
        }
        let lo = usize::try_from(e.offset).unwrap_or(usize::MAX);
        let data = lo
            .checked_add(usize::try_from(e.size).unwrap_or(usize::MAX))
            .and_then(|hi| payload.get(lo..hi))
            .ok_or_else(|| unsupported(format!("entry {:?} runs past the payload", e.name)))?;
        files.push((parts.iter().fold(root.to_path_buf(), |p, c| p.join(c)), data));
    }
    if let Some(d) = dirs.iter().find(|d| seen.contains(*d)) {
        return Err(unsupported(format!(
            "an entry is a file where another needs the folder {d}"
        )));
    }
    Ok((files, deleted))
}

/// The bundle `bundle` (its bytes) unpacked into `out` as FbrbTool's `unpack` does.
///
/// # Errors
/// [`UnpackError::Unsupported`] for a bundle left to FbrbTool.exe (nothing is written then), [`UnpackError::Io`]
/// when a file or folder cannot be written.
pub fn unpack_bytes(file: &[u8], out: &Path) -> Result<UnpackStats, UnpackError> {
    if file.len() < 8 || file.get(..4) != Some(&MAGIC[..]) {
        return Err(unsupported("not an FbRB bundle"));
    }
    if i32::try_from(file.len()).is_err() {
        return Err(unsupported("2 GiB or more"));
    }
    let cut = file
        .get(4..8)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map_or(0, u32::from_be_bytes);
    let header_end = usize::try_from(cut)
        .ok()
        .and_then(|c| c.checked_add(8))
        .filter(|&e| e <= file.len())
        .ok_or_else(|| unsupported("the header runs past the file"))?;
    let h = gunzip_plain(file.get(8..header_end).unwrap_or_default(), true)
        .ok_or_else(|| unsupported("the header is not one plain gzip member"))?;
    let (_version, entries, zipped, payload_field) =
        parse_header_fields(&h).map_err(|e| unsupported(format!("header: {e}")))?;
    let stored = file.get(header_end..).unwrap_or_default();
    let unzipped;
    let payload: &[u8] = if zipped {
        unzipped =
            gunzip_plain(stored, false).ok_or_else(|| unsupported("the payload is not one plain gzip member"))?;
        &unzipped
    } else {
        stored
    };
    if i32::try_from(payload.len()).is_err() {
        return Err(unsupported("a payload of 2 GiB or more"));
    }
    let root = std::path::absolute(out).map_err(io_at(out))?;
    let (files, deleted) = plan(&entries, payload, &root)?;
    let tokens = if zipped {
        String::new()
    } else {
        layout_tokens(&entries, payload, payload_field.unwrap_or(0), 8 + u64::from(cut))
    };
    let text = manifest(&entries, zipped, &tokens);
    fs::create_dir_all(&root).map_err(io_at(&root))?;
    let mut made: HashSet<PathBuf> = HashSet::new();
    for (f, _) in &files {
        if let Some(parent) = f.parent()
            && made.insert(parent.to_path_buf())
        {
            fs::create_dir_all(parent).map_err(io_at(parent))?;
        }
    }
    files
        .par_iter()
        .try_for_each(|(f, data)| fs::write(f, data).map_err(io_at(f)))?;
    let m = root.join("fbrb.manifest");
    fs::write(&m, text).map_err(io_at(&m))?;
    Ok(UnpackStats {
        entries: entries.len(),
        payload_len: payload.len() as u64,
        deleted,
    })
}

/// [`unpack_bytes`] of the bundle at `bundle`.
///
/// # Errors
/// As [`unpack_bytes`]; [`UnpackError::Io`] when the bundle cannot be read.
pub fn unpack_path(bundle: impl AsRef<Path>, out: impl AsRef<Path>) -> Result<UnpackStats, UnpackError> {
    let bundle = bundle.as_ref();
    let file = fs::read(bundle).map_err(io_at(bundle))?;
    unpack_bytes(&file, out.as_ref())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::fbrb::{build, pack_path};

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bc2-unpack-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn unpacks_and_packs_back() {
        for zipped in [true, false] {
            let b = build(
                &[
                    ("Levels/A.dbx", "<non-resource>", b"abc"),
                    ("Gone", DELETED, b""),
                    ("levels/b/c.bin", "Tex", b"0123456789"),
                ],
                zipped,
            );
            let d = temp(&format!("rt{zipped}"));
            let s = unpack_bytes(&b, &d).unwrap();
            assert_eq!((s.entries, s.deleted, s.payload_len), (3, 1, 13));
            assert_eq!(fs::read(d.join("Levels").join("A.dbx")).unwrap(), b"abc");
            assert!(!d.join("Gone").exists());
            let m = fs::read_to_string(d.join("fbrb.manifest")).unwrap();
            assert!(m.starts_with(&format!(
                "# fbrb.manifest - entry order matters. name|type|flags(hex)|size2|size  zipped={}\r\n",
                u8::from(zipped)
            )));
            assert!(m.contains("Gone|*deleted*|00010000|0|0\r\n"));
            let out = d.join("back.fbrb");
            pack_path(&d, &out).unwrap();
            let again = temp(&format!("rt2{zipped}"));
            unpack_path(&out, &again).unwrap();
            assert_eq!(
                fs::read(again.join("fbrb.manifest")).unwrap(),
                fs::read(d.join("fbrb.manifest")).unwrap()
            );
            let _ = fs::remove_dir_all(&d);
            let _ = fs::remove_dir_all(&again);
        }
    }

    #[test]
    fn unusual_bundles_are_left_to_fbrbtool() {
        let cases: [&[(&str, &str, &[u8])]; 6] = [
            &[("../x", "t", b"a")],
            &[("a/./b", "t", b"a")],
            &[("a/b.", "t", b"a")],
            &[("A", "t", b"a"), ("a", "t", b"b")],
            &[("a", "t", b"a"), ("a/b", "t", b"b")],
            &[("con.dbx", "t", b"a")],
        ];
        for (i, c) in cases.iter().enumerate() {
            let d = temp(&format!("odd{i}"));
            assert!(
                matches!(unpack_bytes(&build(c, true), &d), Err(UnpackError::Unsupported(_))),
                "case {i}"
            );
            assert!(!d.exists(), "case {i} wrote");
        }
        let mut b = build(&[("a", "t", b"a")], true);
        b.push(0);
        assert!(matches!(
            unpack_bytes(&b, &temp("tail")),
            Err(UnpackError::Unsupported(_))
        ));
        let n = b.len();
        assert!(matches!(
            unpack_bytes(&b[..n - 6], &temp("cut")),
            Err(UnpackError::Unsupported(_))
        ));
        assert!(matches!(
            unpack_bytes(b"FbRB", &temp("short")),
            Err(UnpackError::Unsupported(_))
        ));
    }

    #[test]
    fn layout_tokens_of_padded_payloads() {
        let e = |offset, size, size2| Entry {
            name: "x".into(),
            type_name: "t".into(),
            flags: 0,
            offset,
            size,
            size2,
        };
        let mut p = vec![1u8; 100];
        p.extend(vec![0xBF; 4096 - 100]);
        p.extend(vec![2u8; 100]);
        let es = [e(0, 100, 36), e(4096, 100, 36)];
        assert_eq!(
            layout_tokens(&es, &p, 4096 + 36, 4096),
            " layout=align4096 hdr=4096 plen=size2 size2=minus64"
        );
        let mut g = vec![1u8; 10];
        g.extend(vec![0xBF; 64]);
        g.extend(vec![2u8; 10]);
        assert_eq!(
            layout_tokens(&[e(0, 10, 10), e(74, 10, 10)], &g, 84, 22),
            " layout=gap64 hdr=1"
        );
        assert_eq!(layout_tokens(&[e(0, 10, 10), e(10, 10, 10)], &[0; 20], 20, 24), "");
    }
}
