//! The bundle header: its entries and whether the payload is zipped (`fbrb.py`'s `_header`).

use std::collections::HashMap;
use std::io::Read;

use super::error::{FbrbError, Result};
use super::gzip::gunzip_all;

/// The bundle's magic, its first four bytes.
pub const MAGIC: &[u8; 4] = b"FbRB";
/// The type of a *deleted* marker: a zero-byte entry that removes shared data from a level.
pub const DELETED: &str = "*deleted*";

/// One entry of a bundle, as the header lists it (`fbrb.Entry`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Entry {
    /// The archive name (`Levels/MP_005/MP_005.dbx`), each byte outside ASCII read as U+FFFD (Python's
    /// `decode('ascii', errors='replace')`).
    pub name: String,
    /// The resource type (`<non-resource>`, `TerrainHeightfield`, [`DELETED`] ...).
    pub type_name: String,
    /// The flags word (`0x00010000` for most entries).
    pub flags: u32,
    /// The offset of the entry's data in the decompressed payload.
    pub offset: u32,
    /// The size of the entry's data.
    pub size: u32,
    /// The second size field (equal to `size` for nearly every entry; FbrbTool keeps it).
    pub size2: u32,
}

impl Entry {
    /// Whether this is a *deleted* marker: type [`DELETED`] and no data.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.type_name == DELETED && self.size == 0
    }

    /// The offset one past the entry's data in the payload.
    #[must_use]
    pub fn end(&self) -> u64 {
        u64::from(self.offset) + u64::from(self.size)
    }
}

/// A bundle's header: its entries in order, and where and how its payload is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The first word of the decompressed header (2 in every bundle of the game).
    pub version: u32,
    /// The entries in archive order, *deleted* markers included.
    pub entries: Vec<Entry>,
    /// Whether the payload is one gzip stream (else it is stored as it is).
    pub zipped: bool,
    /// The header's payload-length field, when the header has it (the reference does not read it).
    pub payload_len: Option<u32>,
    /// Where the payload starts in the file: 8 + the header's stored length.
    pub payload_start: u64,
}

impl Header {
    /// The entries by lower-case name, *deleted* markers left out (`fbrb.names`): for a name listed twice the
    /// later entry wins, as in the reference's dictionary.
    #[must_use]
    pub fn names(&self) -> HashMap<String, &Entry> {
        self.entries
            .iter()
            .filter(|e| !e.is_deleted())
            .map(|e| (e.name.to_ascii_lowercase(), e))
            .collect()
    }
}

fn be32(h: &[u8], at: u64) -> Result<u32> {
    usize::try_from(at)
        .ok()
        .and_then(|a| h.get(a..a.checked_add(4)?))
        .and_then(|s| <[u8; 4]>::try_from(s).ok())
        .map(u32::from_be_bytes)
        .ok_or(FbrbError::HeaderPastEnd {
            len: h.len(),
            needed: at.saturating_add(4),
        })
}

/// The zero-terminated string at `at` (Python's `bytes.index(b'\0', at)`: the zero may be anywhere after it).
fn cstring(h: &[u8], at: u64) -> Result<String> {
    let rest = usize::try_from(at)
        .ok()
        .and_then(|a| h.get(a..))
        .ok_or(FbrbError::StringWithoutEnd { offset: at })?;
    let end = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or(FbrbError::StringWithoutEnd { offset: at })?;
    Ok(rest
        .iter()
        .take(end)
        .map(|&b| {
            if b.is_ascii() {
                char::from(b)
            } else {
                char::REPLACEMENT_CHARACTER
            }
        })
        .collect())
}

/// The fields of a decompressed header: `u32 version, u32 string-table length, the strings, u32 count, count x
/// (name offset, flags, payload offset, size, size2, type offset), u8 zipped, u32 payload length`. String offsets
/// count from the start of the string table (byte 8).
///
/// ```
/// let mut h = Vec::new();
/// h.extend(2u32.to_be_bytes());
/// h.extend(4u32.to_be_bytes());
/// h.extend(b"a\0t\0");
/// h.extend(1u32.to_be_bytes());
/// for v in [0u32, 0x10000, 0, 5, 5, 2] {
///     h.extend(v.to_be_bytes());
/// }
/// h.push(1);
/// let (version, entries, zipped, payload_len) = bc2_formats::fbrb::parse_header_fields(&h)?;
/// assert_eq!((version, entries[0].name.as_str(), entries[0].type_name.as_str(), zipped), (2, "a", "t", true));
/// assert_eq!(payload_len, None);
/// # Ok::<(), bc2_formats::fbrb::FbrbError>(())
/// ```
///
/// # Errors
/// [`FbrbError::HeaderPastEnd`] or [`FbrbError::StringWithoutEnd`] for a damaged header.
pub fn parse_header_fields(h: &[u8]) -> Result<(u32, Vec<Entry>, bool, Option<u32>)> {
    const BASE: u64 = 8;
    let version = be32(h, 0)?;
    let strlen = u64::from(be32(h, 4)?);
    let table = BASE + strlen;
    let count = u64::from(be32(h, table)?);
    let tail = table + 4 + 24 * count;
    // the whole table and the zipped byte are there before anything is allocated for them
    if tail >= h.len() as u64 {
        return Err(FbrbError::HeaderPastEnd {
            len: h.len(),
            needed: tail + 1,
        });
    }
    let mut entries = Vec::with_capacity(count as usize);
    for i in 0..count {
        let at = table + 4 + 24 * i;
        let field = |k: u64| be32(h, at + 4 * k);
        entries.push(Entry {
            name: cstring(h, BASE + u64::from(field(0)?))?,
            flags: field(1)?,
            offset: field(2)?,
            size: field(3)?,
            size2: field(4)?,
            type_name: cstring(h, BASE + u64::from(field(5)?))?,
        });
    }
    let zipped = h.get(tail as usize).is_some_and(|&b| b != 0);
    Ok((version, entries, zipped, be32(h, tail + 1).ok()))
}

/// Reads a bundle's header from the start of `r` (only the header: the payload is not read).
///
/// # Errors
/// [`FbrbError::NotBundle`] when `r` does not start with `FbRB`, [`FbrbError::Io`] when it cannot be read, a gzip or
/// header error for a damaged header.
pub fn read_header<R: Read>(r: &mut R) -> Result<Header> {
    let mut head = Vec::with_capacity(8);
    r.by_ref().take(8).read_to_end(&mut head)?;
    if head.len() < 8 || head.get(..4) != Some(&MAGIC[..]) {
        return Err(FbrbError::NotBundle);
    }
    let cut = head
        .get(4..8)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map_or(0, u32::from_be_bytes);
    let mut packed = Vec::new();
    r.by_ref().take(u64::from(cut)).read_to_end(&mut packed)?;
    let h = gunzip_all(&packed)?;
    let (version, entries, zipped, payload_len) = parse_header_fields(&h)?;
    Ok(Header {
        version,
        entries,
        zipped,
        payload_len,
        payload_start: 8 + u64::from(cut),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn header(strings: &[u8], rows: &[[u32; 6]], tail: &[u8]) -> Vec<u8> {
        let mut h = Vec::new();
        h.extend(2u32.to_be_bytes());
        h.extend((strings.len() as u32).to_be_bytes());
        h.extend(strings);
        h.extend((rows.len() as u32).to_be_bytes());
        for r in rows {
            for v in r {
                h.extend(v.to_be_bytes());
            }
        }
        h.extend(tail);
        h
    }

    #[test]
    fn fields() {
        let h = header(
            b"A/B.dbx\0t\0\xffx\0",
            &[[0, 1, 2, 3, 4, 8], [10, 0, 0, 0, 0, 8]],
            &[0, 0, 0, 0, 9],
        );
        let (v, e, zipped, plen) = parse_header_fields(&h).unwrap();
        assert_eq!((v, zipped, plen), (2, false, Some(9)));
        assert_eq!(e[0].name, "A/B.dbx");
        assert_eq!((e[0].flags, e[0].offset, e[0].size, e[0].size2), (1, 2, 3, 4));
        assert_eq!(e[1].name, "\u{FFFD}x");
        assert_eq!(e[0].end(), 5);
        // a name may run into the entry table, as the reference's search allows
        let h2 = header(b"ab", &[[0, 0, 0, 0, 0, 0]], &[1]);
        assert_eq!(parse_header_fields(&h2).unwrap().1[0].name, "ab");
    }

    #[test]
    fn damaged_headers() {
        let good = header(b"n\0", &[[0, 0, 0, 0, 0, 0]], &[1]);
        for cut in 0..good.len() {
            assert!(parse_header_fields(&good[..cut]).is_err(), "cut at {cut}");
        }
        let far = header(b"n\0", &[[999, 0, 0, 0, 0, 0]], &[1]);
        assert!(matches!(
            parse_header_fields(&far),
            Err(FbrbError::StringWithoutEnd { .. })
        ));
        let huge = header(b"", &[], &[]);
        let mut huge = huge[..12].to_vec();
        huge[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            parse_header_fields(&huge),
            Err(FbrbError::HeaderPastEnd { .. })
        ));
        let mut long_table = good.clone();
        long_table[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse_header_fields(&long_table).is_err());
    }

    #[test]
    fn names_skip_markers_and_later_wins() {
        let e = |n: &str, t: &str, size| Entry {
            name: n.into(),
            type_name: t.into(),
            flags: 0,
            offset: 0,
            size,
            size2: size,
        };
        let h = Header {
            version: 2,
            entries: vec![
                e("A.dbx", "x", 1),
                e("a.DBX", "y", 2),
                e("Gone", DELETED, 0),
                e("Kept", DELETED, 3),
            ],
            zipped: true,
            payload_len: None,
            payload_start: 8,
        };
        let n = h.names();
        assert_eq!(n.len(), 2);
        assert_eq!(n["a.dbx"].size, 2);
        assert!(!n.contains_key("gone") && n["kept"].size == 3);
    }

    #[test]
    fn read_header_refuses() {
        assert!(matches!(read_header(&mut &b"FbR"[..]), Err(FbrbError::NotBundle)));
        assert!(matches!(
            read_header(&mut &b"XbRB\0\0\0\0"[..]),
            Err(FbrbError::NotBundle)
        ));
        // no header data at all: gzip.decompress(b'') is empty, too short for the fields
        assert!(matches!(
            read_header(&mut &b"FbRB\0\0\0\0"[..]),
            Err(FbrbError::HeaderPastEnd { .. })
        ));
    }
}
