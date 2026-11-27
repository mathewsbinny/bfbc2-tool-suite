//! Project OPFOR's menu patches applied (round OR5): the port of OPFOR's `src\tools\apt\AptTool.cs` `patch`, which
//! `build_menus.bat` compiled with the .NET Framework's `csc.exe` and ran on each `Menu\<menu>.res` of the main menu
//! bundle. `bfbc2 aptpatch <in.res> <file.patch> <out.res>` gives the archive AptTool gave, byte for byte.
//!
//! The patch format is the one `aptcheck lint` reads ([`crate::common::parse_patch`]):
//! `<member> <hex offset> <expected bytes> -> <new bytes>` (a new byte may be `@label`, the label's 4-byte
//! little-endian offset) and `append <member> <label> "text"` (a NUL-terminated string at the member's end, 4-byte
//! aligned, defining `@label`). Every expected byte is checked before anything is written; the archive is rebuilt with
//! the members in their index order, 4-byte aligned ([`bc2_formats::aptmodel::write_bigf`], the layout AptTool wrote).
//!
//! Where AptTool was looser, the patch is refused (spec MENU-PATCH-1): a line `aptcheck lint` finds malformed, a label
//! defined twice (AptTool kept the last), a member named twice in the archive. OPFOR's twelve patch files have none
//! of these, so their output is unchanged.

#![allow(clippy::doc_markdown, reason = "AptTool's and the patch format's words")]

use crate::common::{apply_patch, parse_bigf, parse_patch};
use crate::error::{MenuError, MenuResult};

/// A patched archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patched {
    /// The rebuilt BIGF archive.
    pub res: Vec<u8>,
    /// Byte patch lines applied.
    pub applied: usize,
    /// Strings appended.
    pub appended: usize,
}

fn refuse(msg: impl Into<String>) -> MenuError {
    MenuError::Menu(msg.into())
}

/// The members of `res` (a BIGF archive; `label` names it in errors), in index order. A repeated name and a member
/// whose data runs past the end of the file (a cut archive: the readers' slice would shorten it silently) are refused.
pub fn members(res: &[u8], label: &str) -> MenuResult<Vec<(String, Vec<u8>)>> {
    let m = parse_bigf(res, label)?;
    let n = bc2_formats::bigf::parse(res).map_or(0, |v| v.len());
    if n != m.items.len() {
        return Err(refuse(format!(
            "{label}: a member name is in the archive's index twice"
        )));
    }
    let word = |at: usize| -> Option<usize> {
        res.get(at..at + 4)?
            .try_into()
            .ok()
            .map(|b: [u8; 4]| u32::from_be_bytes(b) as usize)
    };
    let mut p = 16usize;
    for (name, _) in &m.items {
        let (off, size) = (word(p).unwrap_or(0), word(p + 4).unwrap_or(0));
        if off.checked_add(size).is_none_or(|end| end > res.len()) {
            return Err(refuse(format!(
                "{label}: member {name} runs past the end of the file (cut short?)"
            )));
        }
        p += 8 + name.len() + 1;
    }
    Ok(m.items)
}

/// Applies the patch file `patch` to the archive `res` (`label` names it in errors): AptTool's `patch`.
pub fn patch(res: &[u8], label: &str, patch: &str) -> MenuResult<Patched> {
    let (lines, problems) = parse_patch(patch)?;
    if let Some((n, msg)) = problems.first() {
        return Err(refuse(format!("{patch} line {n}: {msg}")));
    }
    let items = members(res, label)?;
    let mut have = crate::common::Members::default();
    for (k, v) in &items {
        have.insert(k.clone(), v.clone());
    }
    let (data, _named, problems) = apply_patch(&have, &lines);
    if let Some((n, msg)) = problems.first() {
        return Err(refuse(format!("{patch} line {n}: {msg}")));
    }
    let appended = lines.iter().filter(|l| l.append).count();
    let out: Vec<(String, Vec<u8>)> = data.items;
    let res = bc2_formats::aptmodel::write_bigf(&out).map_err(|e| refuse(format!("{label}: {e}")))?;
    Ok(Patched {
        res,
        applied: lines.len() - appended,
        appended,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
        let m: Vec<(String, Vec<u8>)> = members.iter().map(|(n, d)| ((*n).to_owned(), d.to_vec())).collect();
        bc2_formats::aptmodel::write_bigf(&m).unwrap()
    }

    fn patch_file(dir: &std::path::Path, text: &str) -> String {
        let p = dir.join("t.patch");
        std::fs::write(&p, text).unwrap();
        p.to_string_lossy().into_owned()
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bc2menu-aptpatch-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn patches_and_appends() {
        let d = scratch("ok");
        let res = archive(&[("m.apt", &[1, 2, 3, 4, 5, 6, 7, 8, 9]), ("m.const", &[0xAA; 6])]);
        let p = patch_file(
            &d,
            "# a comment\nappend m.apt lbl \"hi#there\" # note\nm.apt 4 05 06 07 08 -> @lbl # point at it\nm.const 1 AA -> BB\n",
        );
        let got = patch(&res, "m.res", &p).unwrap();
        assert_eq!((got.applied, got.appended), (2, 1));
        let m = members(&got.res, "out").unwrap();
        // the string at the 4-aligned end (12), its offset written little-endian at 4
        assert_eq!(
            m[0].1,
            [
                1, 2, 3, 4, 12, 0, 0, 0, 9, 0, 0, 0, b'h', b'i', b'#', b't', b'h', b'e', b'r', b'e', 0
            ]
        );
        assert_eq!(m[1].1, [0xAA, 0xBB, 0xAA, 0xAA, 0xAA, 0xAA]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn refusals() {
        let d = scratch("bad");
        let res = archive(&[("m.apt", &[1, 2, 3, 4])]);
        for text in [
            "m.apt 0 09 -> 01\n",                           // the expected byte is not there
            "m.apt 3 04 05 -> 01 02\n",                     // past the end
            "m.apt 0 01 -> 01 02\n",                        // a size change
            "x.apt 0 01 -> 02\n",                           // no such member
            "m.apt 0 01 -> @nolabel\n",                     // an undefined label
            "m.apt zz 01 -> 02\n",                          // not hex
            "append m.apt a \"x\"\nappend m.apt a \"y\"\n", // a label twice
        ] {
            let p = patch_file(&d, text);
            assert!(patch(&res, "m.res", &p).is_err(), "{text}");
        }
        assert!(patch(b"NOPE", "m.res", &patch_file(&d, "")).is_err());
        // a cut archive: its last member runs past the end (AptTool's index read failed on it)
        let good = patch_file(&d, "m.apt 0 01 -> 02\n");
        assert!(patch(&res, "m.res", &good).is_ok());
        assert!(patch(&res[..res.len() - 1], "m.res", &good).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
