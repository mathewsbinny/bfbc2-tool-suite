//! The header and string table (FORMAT.md 1): `{binary}`, four big-endian words (the body offset, a value, the
//! string area's size, the number of strings), the strings' offsets, then the zero-terminated strings.

use super::error::{DbxError, Result};

/// The magic bytes every binary `.dbx` starts with.
pub const MAGIC: &[u8; 8] = b"{binary}";

/// Whether `data` is a binary `.dbx` (the magic and more than the 24-byte header), as `dbx.is_binary`.
#[must_use]
pub fn is_binary(data: &[u8]) -> bool {
    data.len() > 24 && data.starts_with(MAGIC)
}

fn be_u32(data: &[u8], at: usize) -> Result<u32> {
    let end = at.checked_add(4).ok_or(DbxError::UnexpectedEnd { offset: at })?;
    let word: [u8; 4] = data
        .get(at..end)
        .and_then(|b| b.try_into().ok())
        .ok_or(DbxError::UnexpectedEnd { offset: data.len() })?;
    Ok(u32::from_be_bytes(word))
}

/// The string table and the offset the body starts at (`24 + area`).
///
/// `tolerant` reads a last string without its zero byte up to the end of the file (as the XML export does);
/// otherwise that is [`DbxError::StringWithoutEnd`]. Strings are UTF-8 with invalid bytes replaced by U+FFFD, as
/// Python's `decode('utf-8', errors='replace')`.
pub(crate) fn read_table(data: &[u8], tolerant: bool) -> Result<(Vec<String>, usize)> {
    let area = be_u32(data, 16)?;
    let count = be_u32(data, 20)?;
    let count_usize = count as usize;
    let base = count_usize
        .checked_mul(4)
        .and_then(|b| b.checked_add(24))
        .filter(|&b| b <= data.len())
        .ok_or(DbxError::StringTablePastEnd {
            count: u64::from(count),
        })?;
    let mut out = Vec::with_capacity(count_usize);
    for i in 0..count_usize {
        let off = be_u32(data, 24 + 4 * i)? as usize;
        let start = base.saturating_add(off);
        let tail = data.get(start..).unwrap_or_default();
        let bytes = match tail.iter().position(|&b| b == 0) {
            Some(len) => tail.get(..len).unwrap_or_default(),
            None if tolerant => tail,
            None => return Err(DbxError::StringWithoutEnd),
        };
        out.push(String::from_utf8_lossy(bytes).into_owned());
    }
    Ok((out, 24 + area as usize))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn header(area: u32, offsets: &[u32], blob: &[u8]) -> Vec<u8> {
        let mut d = MAGIC.to_vec();
        for w in [0u32, 0, area, offsets.len() as u32] {
            d.extend(w.to_be_bytes());
        }
        for o in offsets {
            d.extend(o.to_be_bytes());
        }
        d.extend(blob);
        d
    }

    #[test]
    fn reads_strings_and_body_offset() {
        let d = header(4 * 2 + 4, &[0, 2], b"a\0b\0");
        let (s, body) = read_table(&d, false).unwrap();
        assert_eq!(s, ["a", "b"]);
        assert_eq!(body, 24 + 12);
    }

    #[test]
    fn missing_terminator() {
        let d = header(0, &[0], b"abc");
        assert_eq!(read_table(&d, false), Err(DbxError::StringWithoutEnd));
        assert_eq!(read_table(&d, true).unwrap().0, ["abc"]);
        // an offset past the end reads as an empty string when tolerant
        let d = header(0, &[100], b"x");
        assert_eq!(read_table(&d, true).unwrap().0, [""]);
        assert!(read_table(&d, false).is_err());
    }

    #[test]
    fn invalid_utf8_replaced() {
        let d = header(0, &[0], b"a\xffb\xed\xa0\x80\0");
        assert_eq!(read_table(&d, false).unwrap().0, ["a\u{fffd}b\u{fffd}\u{fffd}\u{fffd}"]);
    }

    #[test]
    fn short_header_and_table() {
        assert!(read_table(b"{binary}\0\0", false).is_err());
        let mut d = header(0, &[], b"");
        d[23] = 200; // 200 strings, no offsets
        assert_eq!(read_table(&d, true), Err(DbxError::StringTablePastEnd { count: 200 }));
        d[20] = 0xFF; // a count whose offsets would overflow a 32-bit size
        assert!(read_table(&d, true).is_err());
    }

    #[test]
    fn is_binary_needs_magic_and_header() {
        assert!(!is_binary(b"{binary}"));
        assert!(is_binary(&header(0, &[], b"\0")));
        assert!(!is_binary(b"<?xml version=\"1.0\"?>\r\n<partition>"));
    }
}
