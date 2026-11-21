//! The game's localized texts (`Localization/bin/language_<lang>_normal.bin`, `_huge`, `_numbers`) and their
//! histograms (`language_<lang>_histogram.bin`), a port of `bfbc2-menu-tools/bc2loc.py` (`LocFile`, `khash`), round
//! F0. The layout is in that module's docstring.
//!
//! The same entries in the same order, the same texts and the same bytes written. Where the reference raises (not a
//! localization file, a header that does not match the file, a histogram that is not one or is too short) this
//! returns an [`Error`] and the Python reads the file itself, so the error stays the reference's. A decoded text that
//! would hold a lone surrogate (a histogram entry in 0xD800..0xDFFF, which a Rust string cannot hold) is left to the
//! reference too ([`decode`] gives `None`).
//!
//! ```
//! use bc2_formats::loc;
//!
//! let file = loc::write(&[0; 16], b"", &[], &[(loc::khash(b"ID_OK"), b"OK".to_vec())]).unwrap();
//! let parsed = loc::parse(&file).unwrap();
//! assert_eq!(parsed.entries[0].0, loc::khash(b"ID_OK"));
//! assert_eq!(parsed.raw(&file, 0), b"OK");
//! assert_eq!(loc::decode(b"OK", None).as_deref(), Some("OK"));
//! ```

use crate::bytes::array;

/// The text file's first word.
pub const TEXT_MAGIC: u32 = 0x0003_9000;
/// The histogram's first word.
pub const HISTOGRAM_MAGIC: u32 = 0x0003_9001;

/// Why a file is left to the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Not a localization text file (or histogram).
    Magic,
    /// The header does not match the file.
    Header,
    /// A value past `u32` when writing.
    TooLarge,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Magic => "not a localization file",
            Self::Header => "the header does not match the file",
            Self::TooLarge => "a value too large for the file",
        })
    }
}

impl std::error::Error for Error {}

/// A key's hash: `h = 0xFFFFFFFF`, then `h = h * 33 + byte` for each byte, in 32 bits.
#[must_use]
pub fn khash(key: &[u8]) -> u32 {
    key.iter()
        .fold(u32::MAX, |h, &c| h.wrapping_mul(33).wrapping_add(u32::from(c)))
}

/// A key given as text: its characters as latin-1 bytes, `?` for any other (`key.encode('latin1', 'replace')`).
#[must_use]
pub fn khash_str(key: &str) -> u32 {
    khash(
        &key.chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect::<Vec<u8>>(),
    )
}

/// A parsed text file: its table's entries and where its strings start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocFile {
    /// The name field (bytes 20..36 up to the first NUL), as latin-1 bytes.
    pub name: Vec<u8>,
    /// In table order: (hash, text offset from the string offset, the text's byte range in the file).
    pub entries: Vec<(u32, u32, std::ops::Range<usize>)>,
    /// Where the strings start in the file (`8 + string offset`, at most the file's length).
    pub strings: usize,
}

impl LocFile {
    /// Entry `i`'s raw text.
    #[must_use]
    pub fn raw<'a>(&self, data: &'a [u8], i: usize) -> &'a [u8] {
        self.entries
            .get(i)
            .and_then(|(_, _, r)| data.get(r.clone()))
            .unwrap_or(&[])
    }
}

fn word(data: &[u8], at: usize) -> Option<u32> {
    array::<4>(data, at).map(u32::from_le_bytes)
}

/// `LocFile(data)`'s reading of the file.
///
/// # Errors
/// Where the reference raises.
pub fn parse(data: &[u8]) -> Result<LocFile, Error> {
    if data.len() < 36 || word(data, 0) != Some(TEXT_MAGIC) {
        return Err(Error::Magic);
    }
    let w = |k: usize| word(data, k * 4).ok_or(Error::Magic);
    let (size, count, table, strings) = (u64::from(w(1)?), u64::from(w(2)?), u64::from(w(3)?), u64::from(w(4)?));
    let len = data.len() as u64;
    if size != len - 8 || 8 + table + 8 * count > len {
        return Err(Error::Header);
    }
    let name_field = data.get(20..36).unwrap_or(&[]);
    let name = name_field.split(|&b| b == 0).next().unwrap_or(&[]).to_vec();
    let base = 8 + strings;
    let mut entries = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let at = 8 + table as usize + 8 * i;
        let (h, o) = (
            word(data, at).ok_or(Error::Header)?,
            word(data, at + 4).ok_or(Error::Header)?,
        );
        // data[base + o : the next NUL from there, or the end]
        let start = (base + u64::from(o)).min(len) as usize;
        let end = data
            .get(start..)
            .and_then(|rest| rest.iter().position(|&b| b == 0))
            .map_or(data.len(), |p| start + p);
        entries.push((h, o, start..end));
    }
    Ok(LocFile {
        name,
        entries,
        strings: base.min(len) as usize,
    })
}

/// A histogram's table (`struct.unpack_from('<{(len - 12) // 2}H', histogram, 12)`).
///
/// # Errors
/// [`Error::Magic`] where the reference raises (not a histogram, or shorter than its 12-byte header).
pub fn histogram(data: &[u8]) -> Result<Vec<u16>, Error> {
    if word(data, 0) != Some(HISTOGRAM_MAGIC) || data.len() < 12 {
        return Err(Error::Magic);
    }
    Ok(crate::bytes::u16s(data.get(12..).unwrap_or(&[])).collect())
}

/// `LocFile.decode(raw)` with the histogram's table (`None`: no histogram), or `None` where the text would hold a
/// lone surrogate.
#[must_use]
pub fn decode(raw: &[u8], table: Option<&[u16]>) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let chr = |v: u16, out: &mut String| -> Option<()> {
        out.push(char::from_u32(u32::from(v))?);
        Some(())
    };
    while let Some(&c) = raw.get(i) {
        i += 1;
        let Some(t) = table.filter(|_| c >= 0x80) else {
            if c < 0x80 {
                out.push(char::from(c));
            } else {
                // Python's f'\\x{c:02x}'
                out.push_str("\\x");
                out.push(char::from_digit(u32::from(c >> 4), 16).unwrap_or('0'));
                out.push(char::from_digit(u32::from(c & 15), 16).unwrap_or('0'));
            }
            continue;
        };
        let v = t.get(usize::from(c)).copied().unwrap_or(0);
        if v >= 0x80 {
            chr(v, &mut out)?;
        } else if let Some(&next) = raw.get(i) {
            let k = usize::from(v) * 128 + usize::from(next & 0x7F);
            i += 1;
            match t.get(k) {
                Some(&tk) if tk != 0 => chr(tk, &mut out)?,
                _ => out.push('?'),
            }
        }
    }
    Some(out)
}

/// A text as a language's file stores it (round EX1: a level's own name in every language): ASCII as itself, any other
/// character through the histogram's table - its one-byte code where it has one, else a two-byte code (a lead byte
/// whose entry is below 0x80, then `0x80 | low`, as every two-byte character of the Japanese file is stored) - each
/// checked by [`decode`]. `Err(c)`: the first character the table has no code for (or a NUL).
///
/// # Errors
/// The first character that cannot be stored.
pub fn encode(text: &str, table: Option<&[u16]>) -> Result<Vec<u8>, char> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let u = c as u32;
        if c == '\0' {
            return Err(c);
        }
        if u < 0x80 {
            out.push(u as u8);
            continue;
        }
        let t = table.ok_or(c)?;
        let at = |k: usize| t.get(k).copied().unwrap_or(0);
        let single = (0x80..=0xFFu8).find(|&b| at(usize::from(b)) >= 0x80 && u32::from(at(usize::from(b))) == u);
        if let Some(b) = single {
            out.push(b);
            continue;
        }
        let two = (0x80..=0xFFu8).find_map(|lead| {
            let v = at(usize::from(lead));
            if v >= 0x80 {
                return None;
            }
            (0..128u8)
                .find(|&j| u32::from(at(usize::from(v) * 128 + usize::from(j))) == u)
                .map(|j| [lead, 0x80 | j])
        });
        let bytes = two.ok_or(c)?;
        if decode(&bytes, Some(t)).as_deref() != Some(c.encode_utf8(&mut [0; 4])) {
            return Err(c);
        }
        out.extend_from_slice(&bytes);
    }
    Ok(out)
}

/// `LocFile.to_bytes()`: `name16` (the file's bytes 20..36), its strings with each changed text appended (in
/// `changed`'s order), and the entry table sorted by hash from `offsets` (each hash once) with the changed texts'
/// new offsets.
///
/// # Errors
/// [`Error::TooLarge`] where an offset, the count or the size does not fit a `u32`.
pub fn write(
    name16: &[u8; 16],
    strings: &[u8],
    offsets: &[(u32, u32)],
    changed: &[(u32, Vec<u8>)],
) -> Result<Vec<u8>, Error> {
    let mut map: std::collections::BTreeMap<u32, u32> = offsets.iter().copied().collect();
    let mut strings = strings.to_vec();
    for (h, b) in changed {
        map.insert(*h, u32::try_from(strings.len()).map_err(|_| Error::TooLarge)?);
        strings.extend_from_slice(b);
        strings.push(0);
    }
    let fit = |v: usize| u32::try_from(v).map_err(|_| Error::TooLarge);
    let table_len = map.len() * 8;
    let mut out = Vec::with_capacity(36 + table_len + strings.len());
    for v in [TEXT_MAGIC, 0, fit(map.len())?, 28, fit(28 + table_len)?] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(name16);
    for (h, o) in map {
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&strings);
    let size = fit(out.len() - 8)?;
    if let Some(s) = out.get_mut(4..8) {
        s.copy_from_slice(&size.to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn hashes() {
        assert_eq!(khash(b""), u32::MAX);
        assert_eq!(khash(b"A"), u32::MAX.wrapping_mul(33).wrapping_add(65));
        assert_eq!(khash_str("\u{e9}x"), khash(&[0xE9, b'x']));
        assert_eq!(khash_str("\u{20ac}"), khash(b"?"));
    }

    #[test]
    fn reads_and_writes() {
        let mut name = [0u8; 16];
        name[..6].copy_from_slice(b"NORMAL");
        let a = write(&name, b"", &[], &[(9, b"nine".to_vec()), (3, b"three".to_vec())]).unwrap();
        let f = parse(&a).unwrap();
        assert_eq!(f.name, b"NORMAL");
        assert_eq!(f.entries.iter().map(|e| e.0).collect::<Vec<_>>(), vec![3, 9]);
        assert_eq!(f.raw(&a, 0), b"three");
        assert_eq!(f.raw(&a, 1), b"nine");
        let offsets: Vec<(u32, u32)> = f.entries.iter().map(|e| (e.0, e.1)).collect();
        let n16: [u8; 16] = a[20..36].try_into().unwrap();
        assert_eq!(write(&n16, &a[f.strings..], &offsets, &[]).unwrap(), a);
        assert_eq!(parse(&a[..a.len() - 1]), Err(Error::Header));
        assert_eq!(parse(b"short"), Err(Error::Magic));
    }

    #[test]
    fn decodes() {
        let mut t = vec![0u16; 256 + 128 * 2];
        t[0x81] = 0xE9; // a one-byte character
        t[0x82] = 1; // starts a two-byte one: t[1 * 128 + (next & 0x7F)]
        t[128 + 5] = 0x3042;
        t[0x83] = 0xD800;
        assert_eq!(decode(b"a\x81\x82\x85", Some(&t)).as_deref(), Some("a\u{e9}\u{3042}"));
        assert_eq!(decode(b"\x82\x86", Some(&t)).as_deref(), Some("?"));
        assert_eq!(decode(b"\x82", Some(&t)).as_deref(), Some(""));
        assert_eq!(decode(b"\x83", Some(&t)), None);
        assert_eq!(decode(b"\xff", None).as_deref(), Some("\\xff"));
        assert_eq!(decode(b"\xff", Some(&t[..10])).as_deref(), Some(""));
    }

    #[test]
    fn encodes_through_the_table() {
        let mut t = vec![0u16; 256 + 128 * 2];
        t[0x81] = 0xE9; // a one-byte character
        t[0x82] = 2; // a lead byte: t[2 * 128 + low]
        t[256 + 5] = 0x3042;
        assert_eq!(encode("a\u{e9}\u{3042}", Some(&t)), Ok(vec![b'a', 0x81, 0x82, 0x85]));
        assert_eq!(
            decode(&encode("x\u{3042}\u{e9}", Some(&t)).unwrap(), Some(&t)).as_deref(),
            Some("x\u{3042}\u{e9}")
        );
        assert_eq!(encode("\u{416}", Some(&t)), Err('\u{416}'));
        assert_eq!(encode("\u{e9}", None), Err('\u{e9}'));
        assert_eq!(encode("a\0", Some(&t)), Err('\0'));
        assert_eq!(encode("plain", None), Ok(b"plain".to_vec()));
    }
}
