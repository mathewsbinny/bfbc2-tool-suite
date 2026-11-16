//! A small bundle writer for tests (`fbrb.build`): FbrbTool packs the real ones.

use std::collections::HashMap;

use super::gzip::gzip;
use super::header::MAGIC;

/// The offset of `t` in the string table, added at its end the first time.
fn intern<'a>(t: &'a str, offsets: &mut HashMap<&'a str, u32>, strings: &mut Vec<u8>) -> u32 {
    *offsets.entry(t).or_insert_with(|| {
        let at = u32::try_from(strings.len()).unwrap_or(u32::MAX);
        strings.extend_from_slice(t.as_bytes());
        strings.push(0);
        at
    })
}

/// A bundle of `(name, type, data)` entries, packed back to back with flags `0x00010000` and `size2 = size`, the
/// payload gzip-compressed when `zipped` (`fbrb.build`). Strings are stored once each (names and types share the
/// table). Sizes and offsets past `u32::MAX` are not representable and saturate: this is a test writer.
///
/// ```
/// use bc2_formats::fbrb;
/// let b = fbrb::build(&[("A.dbx", "<non-resource>", b"x")], false);
/// let h = fbrb::read_header(&mut b.as_slice())?;
/// assert_eq!((h.entries[0].name.as_str(), h.entries[0].size, h.zipped), ("A.dbx", 1, false));
/// # Ok::<(), fbrb::FbrbError>(())
/// ```
#[must_use]
pub fn build(items: &[(&str, &str, &[u8])], zipped: bool) -> Vec<u8> {
    let sat = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let mut strings = Vec::new();
    let mut offsets: HashMap<&str, u32> = HashMap::new();
    let mut table = Vec::new();
    let mut payload = Vec::new();
    for (name, typ, data) in items {
        let row = [
            intern(name, &mut offsets, &mut strings),
            0x0001_0000,
            sat(payload.len()),
            sat(data.len()),
            sat(data.len()),
            intern(typ, &mut offsets, &mut strings),
        ];
        for v in row {
            table.extend(v.to_be_bytes());
        }
        payload.extend_from_slice(data);
    }
    let mut h = Vec::new();
    h.extend(2u32.to_be_bytes());
    h.extend(sat(strings.len()).to_be_bytes());
    h.extend(&strings);
    h.extend(sat(items.len()).to_be_bytes());
    h.extend(&table);
    h.push(u8::from(zipped));
    h.extend(sat(payload.len()).to_be_bytes());
    let hz = gzip(&h);
    let mut out = Vec::with_capacity(8 + hz.len() + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend(sat(hz.len()).to_be_bytes());
    out.extend(hz);
    if zipped {
        out.extend(gzip(&payload));
    } else {
        out.extend(payload);
    }
    out
}
