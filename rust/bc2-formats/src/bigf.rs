//! BIGF archives (the menu and HUD movies' `.res`: `.apt`, `.const` and their other members), a port of
//! `bfbc2-menu-tools/menucommon.py`'s `parse_bigf` and `bfbc2-console-readers/apt/apt.py`'s `read_bigf`, round F0.
//!
//! The index is big-endian on every platform: after the magic and a size word, a u32 member count at 8, then from
//! 16 each member's u32 offset, u32 size and NUL-terminated name. The same members in the same order as the
//! reference (a repeated name keeps its first place and takes its last data, as the reference's dict does; data cut
//! at the end of the file as a slice is). Where the reference raises (not a BIGF archive, an index cut short, a name
//! without its NUL or not UTF-8) this returns `None` and the Python reads it.

/// The members in index order: (name, its data's byte range in the archive).
#[must_use]
#[allow(clippy::many_single_char_names)] // the reference's names
pub fn parse(b: &[u8]) -> Option<Vec<(String, std::ops::Range<usize>)>> {
    if b.get(..4) != Some(b"BIGF".as_slice()) {
        return None;
    }
    let word = |at: usize| -> Option<u32> { b.get(at..at + 4)?.try_into().ok().map(u32::from_be_bytes) };
    let n = word(8)?;
    let mut p = 16usize;
    let mut out = Vec::new();
    for _ in 0..n {
        let (off, size) = (word(p)? as usize, word(p + 4)? as usize);
        let rest = b.get(p + 8..)?;
        let e = rest.iter().position(|&c| c == 0)?;
        let name = std::str::from_utf8(rest.get(..e)?).ok()?.to_owned();
        let a = off.min(b.len());
        out.push((name, a..(off + size).min(b.len()).max(a)));
        p += 8 + e + 1;
    }
    Some(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::parse;

    #[test]
    fn members() {
        let mut b = b"BIGF".to_vec();
        b.extend(0u32.to_be_bytes());
        b.extend(2u32.to_be_bytes());
        b.extend(0u32.to_be_bytes());
        for (off, size, name) in [(40u32, 3u32, "a.apt"), (43, 9, "b.const")] {
            b.extend(off.to_be_bytes());
            b.extend(size.to_be_bytes());
            b.extend(name.as_bytes());
            b.push(0);
        }
        b.resize(40, 0);
        b.extend(b"abcdef");
        let m = parse(&b).unwrap();
        assert_eq!(m[0], ("a.apt".to_owned(), 40..43));
        assert_eq!(m[1], ("b.const".to_owned(), 43..46));
        assert_eq!(parse(&b[..20]), None);
        assert_eq!(parse(b"BIGX"), None);
    }
}
