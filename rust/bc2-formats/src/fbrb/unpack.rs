//! FbrbTool's `unpack` in process: every entry written under a folder by its archive name, and the `fbrb.manifest`
//! FbrbTool writes (`bfbc2-fbrb-tool/FbrbTool.cs`, `Unpack` and `LayoutTokens`), the same bytes.
//!
//! Round E3j added it for the bundles every reader agrees on and left any other bundle to FbrbTool.exe; round E3k
//! ported the rest of FbrbTool (its gzip reader, path rules, errors: [`super::tool`]), so every bundle is unpacked
//! here as FbrbTool unpacks it - an odd name where FbrbTool writes it, a damaged bundle with the files FbrbTool
//! leaves and its error ([`UnpackError::Refused`]). [`unpack_path`] and [`unpack_bytes`] are that one engine (the
//! command line's `unpack` is the same code, with FbrbTool's messages), each file written beside its name and moved
//! over it, the entries on all cores.

use std::io;
use std::path::{Path, PathBuf};

use super::tool;
use crate::dbxedit::{Fault, FaultKind};

/// Why a bundle could not be unpacked: what FbrbTool reports for it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UnpackError {
    /// FbrbTool refuses the bundle (damaged, an entry outside the payload or the folder, a name Windows refuses): its
    /// words. The entries before the one it stopped at are written, as FbrbTool writes them.
    #[error("FbrbTool refuses it: {0}")]
    Refused(String),
    /// A file or folder could not be read or written (FbrbTool's I/O errors: missing, in use, denied, too long).
    #[error("{path}: {source}")]
    Io {
        /// The bundle or the output folder.
        path: PathBuf,
        /// The error, in FbrbTool's words.
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

fn error(f: &Fault, path: &Path) -> UnpackError {
    let (cause, _) = f.explain();
    match f.kind {
        FaultKind::FileNotFound
        | FaultKind::DirectoryNotFound
        | FaultKind::Sharing
        | FaultKind::Io
        | FaultKind::UnauthorizedAccess
        | FaultKind::PathTooLong => UnpackError::Io {
            path: path.to_path_buf(),
            source: io::Error::other(cause),
        },
        _ => UnpackError::Refused(cause),
    }
}

/// The bundle at `bundle` unpacked into `out` as FbrbTool's `unpack` does.
///
/// # Errors
/// What FbrbTool reports: [`UnpackError::Io`] for a file or folder that cannot be read or written,
/// [`UnpackError::Refused`] for anything else.
pub fn unpack_path(bundle: impl AsRef<Path>, out: impl AsRef<Path>) -> Result<UnpackStats, UnpackError> {
    let (bundle, out) = (bundle.as_ref(), out.as_ref());
    tool::unpack_path(&bundle.to_string_lossy(), &out.to_string_lossy()).map_err(|f| error(&f, bundle))
}

/// [`unpack_path`] of a bundle's bytes.
///
/// # Errors
/// As [`unpack_path`].
pub fn unpack_bytes(file: &[u8], out: &Path) -> Result<UnpackStats, UnpackError> {
    tool::unpack_bytes("<bundle>", file, &out.to_string_lossy()).map_err(|f| error(&f, out))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::fs;

    use super::*;
    use crate::fbrb::{DELETED, build, pack_path};

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
    fn unusual_bundles_as_fbrbtool_unpacks_them() {
        // what FbrbTool.exe does with each (core/tests/test_fbrbtool.py runs the cases both ways)
        let d = temp("odd");
        let b = build(
            &[
                ("a/./b", "t", b"1"),
                ("c/d.", "t", b"2"),
                ("Z", "t", b"x"),
                ("a2", "t", b"y"),
            ],
            true,
        );
        unpack_bytes(&b, &d).unwrap();
        assert_eq!(fs::read(d.join("a").join("b")).unwrap(), b"1");
        assert_eq!(fs::read(d.join("c").join("d")).unwrap(), b"2");
        let same = build(&[("A", "t", b"first"), ("a", "t", b"second")], true);
        let d2 = temp("same");
        unpack_bytes(&same, &d2).unwrap();
        assert_eq!(fs::read(d2.join("a")).unwrap(), b"second");
        for (i, bad) in [
            build(&[("../x", "t", b"a")], true),
            build(&[("a", "t", b"a"), ("a/b", "t", b"b")], true),
            b"FbRB".to_vec(),
        ]
        .iter()
        .enumerate()
        {
            assert!(unpack_bytes(bad, &temp(&format!("bad{i}"))).is_err(), "case {i}");
        }
        // bytes after the payload are ignored, as .NET's reader ignores them
        let mut tail = build(&[("a", "t", b"a")], true);
        tail.push(0);
        assert_eq!(unpack_bytes(&tail, &temp("tail")).unwrap().entries, 1);
        assert!(matches!(
            unpack_path(temp("none").join("x.fbrb"), temp("none-out")),
            Err(UnpackError::Io { .. })
        ));
        for x in [d, d2] {
            let _ = fs::remove_dir_all(x);
        }
    }
}
