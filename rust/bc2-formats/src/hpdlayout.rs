//! The byte layout of a sector's runtime-mesh record: a port of NavKit's `hpdtool.mesh_layout` /
//! `_object_end` / `mesh_coverage` (`bfbc2-auto-navkit/tools/hpdtool.py`), round F3. The walk labels every region
//! of the record it knows (header, tile table, tiles, the objects they name, the table's end slot, the trailer, the
//! link table, its lists and links) and returns the structure it read on the way; [`coverage`] sorts the regions and
//! reports the gaps between them and the overlaps.
//!
//! The same regions (in order, with the same labels), tiles, objects (in first-seen order), links and tile offsets
//! as the reference. Where the reference raises, this stops at the same read with the same error ([`Error`]: its
//! `ValueError`s' messages, and `struct.error`'s for a read past the end, in CPython's words; round RT2, so the
//! reference could be retired). A tile item's float is the value CPython's `struct` gives ([`c_double`]: a NaN made
//! quiet, its payload kept).
//!
//! ```
//! use bc2_formats::hpdlayout;
//!
//! let e = hpdlayout::layout(&[0u8; 16]).unwrap_err();
//! assert_eq!(
//!     e.to_string(),
//!     "unpack_from requires a buffer of at least 60 bytes for unpacking 4 bytes at offset 56 (actual buffer size is 16)"
//! );
//! ```

use std::collections::HashSet;
use std::fmt::Write as _;

/// Why the walk stops: the reference's own error at the same point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The reference's `ValueError(message)`.
    Value(String),
    /// `struct.error`: `size` bytes at `offset` (made non-negative as `struct` makes it) of a record of `len`.
    Short {
        /// The bytes the read takes (4 for a word, 12 for a tile item or a link record).
        size: i64,
        /// Where the read starts.
        offset: i64,
        /// The record's length.
        len: i64,
    },
    /// `struct.error` for a negative offset further back than the record's start.
    Range {
        /// The offset asked for.
        offset: i64,
        /// The record's length.
        len: i64,
    },
}

impl Error {
    /// Whether this is the reference's `struct.error` (else its `ValueError`).
    #[must_use]
    pub fn is_struct(&self) -> bool {
        !matches!(self, Self::Value(_))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(m) => f.write_str(m),
            Self::Short { size, offset, len } => write!(
                f,
                "unpack_from requires a buffer of at least {} bytes for unpacking {size} bytes at offset {offset} \
                 (actual buffer size is {len})",
                size + offset
            ),
            Self::Range { offset, len } => write!(f, "offset {offset} out of range for {len}-byte buffer"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

/// `MESH_OBJECT_HEADER`: an object's fixed part.
pub const MESH_OBJECT_HEADER: i64 = 0x50;

/// A labelled region `(start, end, label)`; an end may lie before its start (the reference's arithmetic).
pub type Region = (i64, i64, String);

/// `mesh_coverage`'s result: the gaps `(start, end)` and the overlaps `(start, end, label)`.
pub type Coverage = (Vec<(i64, i64)>, Vec<Region>);

/// A tile: its offset, its items `(object pointer, f32, f32)` and the end offset after them (`None` when empty).
pub type Tile = (u32, Vec<(u32, f32, f32)>, Option<u32>);

/// `mesh_layout`'s result.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// The regions in the order the walk found them.
    pub regions: Vec<Region>,
    /// The tiles in table order.
    pub tiles: Vec<Tile>,
    /// Each object pointer and where its object ends, in first-seen order.
    pub objects: Vec<(u32, i64)>,
    /// The links `(sector id, count, list offset)`.
    pub links: Vec<(u32, u32, u32)>,
    /// The tile table's offsets (one more than the tiles).
    pub tile_offs: Vec<u32>,
}

/// The double CPython's `struct.unpack('<f')` gives for a float32: the value, and for a NaN the x86 conversion's
/// (the quiet bit set, the payload moved to the top of the double's), so the bits match the reference's.
#[must_use]
pub fn c_double(v: f32) -> f64 {
    if v.is_nan() {
        let b = u64::from(v.to_bits());
        f64::from_bits(((b >> 31) << 63) | (0x7FF << 52) | (1 << 51) | ((b & 0x7F_FFFF) << 29))
    } else {
        f64::from(v)
    }
}

/// `struct.unpack_from` of `size` bytes at `at`: the bytes, or the error CPython raises (a negative offset counts
/// from the end, as there).
fn read(m: &[u8], at: i64, size: i64) -> Result<&[u8]> {
    let len = m.len() as i64;
    let mut off = at;
    if off < 0 {
        if off + len < 0 {
            return Err(Error::Range { offset: at, len });
        }
        off += len;
    }
    if len - off < size {
        return Err(Error::Short { size, offset: off, len });
    }
    let (a, n) = (
        usize::try_from(off).unwrap_or(usize::MAX),
        usize::try_from(size).unwrap_or(0),
    );
    m.get(a..a + n).ok_or(Error::Short { size, offset: off, len })
}

fn word(b: &[u8], k: usize) -> u32 {
    let mut w = [0u8; 4];
    w.copy_from_slice(b.get(4 * k..4 * k + 4).unwrap_or(&[0; 4]));
    u32::from_le_bytes(w)
}

fn u32_at(m: &[u8], at: i64) -> Result<u32> {
    Ok(word(read(m, at, 4)?, 0))
}

/// The first of `count` records of `size` bytes from `at` that the record does not hold (the read the reference's
/// loop raises at), or `None` when all fit.
fn first_short(m: &[u8], at: i64, count: u64, size: i64) -> Option<i64> {
    if at < 0 {
        return (count > 0).then_some(at);
    }
    let room = (m.len() as i64 - at).max(-1);
    let fit = if room < size { 0 } else { (room - size) / size + 1 };
    (u64::try_from(fit).unwrap_or(0) < count).then(|| at + fit * size)
}

/// Python's `f'{v:#x}'` (a negative value as `-0x...`).
fn py_hex(v: i64) -> String {
    if v < 0 {
        format!("-{:#x}", v.unsigned_abs())
    } else {
        format!("{v:#x}")
    }
}

/// `_object_end(m, o)`.
fn object_end(m: &[u8], o: i64) -> Result<i64> {
    let (n_a, n_b, n_c) = (u32_at(m, o + 0x20)?, u32_at(m, o + 0x28)?, u32_at(m, o + 0x30)?);
    let mut end = o + MESH_OBJECT_HEADER + i64::from(n_a) * 0x20 + i64::from(n_b) * 0x20 + i64::from(n_c) * 0x2C;
    for (cnt_off, tab_off, stride) in [(0x38, 0x3C, 4i64), (0x40, 0x44, 4), (0x48, 0x4C, 16)] {
        let cnt = i64::from(u32_at(m, o + cnt_off)?);
        if cnt == 0 {
            continue;
        }
        let tab = o + i64::from(u32_at(m, o + tab_off)?);
        if tab != end {
            return Err(Error::Value(format!(
                "object {}: table {cnt_off:#x} at {}, expected {}",
                py_hex(o),
                py_hex(tab),
                py_hex(end)
            )));
        }
        let first = i64::from(u32_at(m, tab)?);
        let last = i64::from(u32_at(m, tab + cnt * stride)?);
        end = tab + (cnt + 1) * stride + ((last - first) & !3);
    }
    Ok(end)
}

/// `hpdtool.mesh_layout(m)`.
///
/// # Errors
///
/// The reference's error where it raises ([`Error`]).
pub fn layout(m: &[u8]) -> Result<Layout> {
    let mut regions: Vec<Region> = vec![(0, 0x50, "header".into())];
    let (nx, nz) = (u32_at(m, 0x38)?, u32_at(m, 0x3C)?);
    let (tiles_off, links_n, links_off) = (u32_at(m, 0x44)?, u32_at(m, 0x48)?, u32_at(m, 0x4C)?);
    if tiles_off != 0x50 {
        return Err(Error::Value(format!("tile table at {tiles_off:#x}")));
    }
    let tile_count = u64::from(nx) * u64::from(nz);
    if let Some(at) = first_short(m, 0x50, tile_count + 1, 4) {
        return Err(Error::Short {
            size: 4,
            offset: at,
            len: m.len() as i64,
        });
    }
    let tc = i64::try_from(tile_count).unwrap_or(i64::MAX);
    regions.push((0x50, 0x50 + (tc + 1) * 4, "tile table".into()));
    let tile_offs = (0..=tc).map(|i| u32_at(m, 0x50 + i * 4)).collect::<Result<Vec<_>>>()?;
    let mut objects: Vec<(u32, i64)> = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    let mut tiles = Vec::with_capacity(tile_offs.len().saturating_sub(1));
    for (t, &to) in tile_offs.iter().take(tile_offs.len() - 1).enumerate() {
        let to64 = i64::from(to);
        let n = u32_at(m, to64)?;
        if let Some(at) = first_short(m, to64 + 4, u64::from(n), 12) {
            return Err(Error::Short {
                size: 12,
                offset: at,
                len: m.len() as i64,
            });
        }
        let mut items = Vec::with_capacity(n as usize);
        for k in 0..i64::from(n) {
            let b = read(m, to64 + 4 + k * 12, 12)?;
            items.push((word(b, 0), f32::from_bits(word(b, 1)), f32::from_bits(word(b, 2))));
        }
        let end_ptr = if n == 0 {
            None
        } else {
            Some(u32_at(m, to64 + 4 + i64::from(n) * 12)?)
        };
        let len = if n == 0 { 4 } else { 8 + i64::from(n) * 12 };
        regions.push((to64, to64 + len, format!("tile {t}")));
        for &(ptr, _, _) in &items {
            if seen.insert(ptr) {
                let end = object_end(m, i64::from(ptr))?;
                objects.push((ptr, end));
                regions.push((i64::from(ptr), end, format!("object @{ptr:#x}")));
            }
        }
        tiles.push((to, items, end_ptr));
    }
    let end_slot = i64::from(tile_offs.last().copied().unwrap_or(0));
    regions.push((end_slot, end_slot + 4, "tile table end slot".into()));
    let len = m.len() as i64;
    if i64::from(u32_at(m, len - 8)?) != len - 8 || u32_at(m, len - 4)? != 0 {
        return Err(Error::Value("mesh trailer not found".into()));
    }
    regions.push((len - 8, len, "trailer".into()));
    let mut links = Vec::new();
    if links_n != 0 {
        let lo = i64::from(links_off);
        regions.push((lo, lo + i64::from(links_n) * 12, "link table".into()));
        for k in 0..i64::from(links_n) {
            let b = read(m, lo + k * 12, 12)?;
            let (sid, cnt, lst) = (word(b, 0), word(b, 1), word(b, 2));
            let l64 = i64::from(lst);
            regions.push((l64, l64 + i64::from(cnt) * 4, format!("link list {sid:#x}")));
            if let Some(at) = first_short(m, l64, u64::from(cnt), 4) {
                // the reference reads each link (and its count) before the next pointer: the links that fit first
                for j in 0..(at - l64) / 4 {
                    let lk = i64::from(u32_at(m, l64 + j * 4)?);
                    u32_at(m, lk + 0xC)?;
                }
                return Err(Error::Short {
                    size: 4,
                    offset: at,
                    len,
                });
            }
            for j in 0..i64::from(cnt) {
                let lk = i64::from(u32_at(m, l64 + j * 4)?);
                let c = i64::from(u32_at(m, lk + 0xC)?);
                let mut label = String::new();
                let _ = write!(label, "link {sid:#x}/{j}");
                regions.push((lk, lk + 0x10 + c * 4, label));
            }
            links.push((sid, cnt, lst));
        }
    }
    Ok(Layout {
        regions,
        tiles,
        objects,
        links,
        tile_offs,
    })
}

/// `hpdtool.mesh_coverage(m)`: the gaps `(start, end)` between the sorted regions and the overlaps
/// `(start, end, label)`.
///
/// # Errors
///
/// [`layout`]'s.
pub fn coverage(m: &[u8]) -> Result<Coverage> {
    let mut regions = layout(m)?.regions;
    regions.sort();
    let mut gaps = Vec::new();
    let mut overlaps = Vec::new();
    let mut pos = 0i64;
    for (s, e, label) in regions {
        if s > pos {
            gaps.push((pos, s));
        } else if s < pos {
            overlaps.push((s, pos, label));
        }
        pos = pos.max(e);
    }
    if pos < m.len() as i64 {
        gaps.push((pos, m.len() as i64));
    }
    Ok((gaps, overlaps))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    fn put(m: &mut [u8], at: usize, v: u32) {
        m[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// A record of one tile naming one object with no tables, and the trailer.
    fn record() -> Vec<u8> {
        let mut m = vec![0u8; 0x50 + 8 + 16 + 0x50 + 8];
        put(&mut m, 0x38, 1);
        put(&mut m, 0x3C, 1);
        put(&mut m, 0x44, 0x50);
        put(&mut m, 0x50, 0x58);
        put(&mut m, 0x54, 0x60);
        put(&mut m, 0x58, 1);
        put(&mut m, 0x5C, 0x6C);
        let n = m.len();
        put(&mut m, n - 8, (n - 8) as u32);
        m
    }

    #[test]
    fn walks_a_record() -> Result<()> {
        let m = record();
        let l = layout(&m)?;
        assert_eq!(l.tile_offs, vec![0x58, 0x60]);
        assert_eq!(l.objects, vec![(0x6C, 0x6C + 0x50)]);
        assert_eq!(l.tiles[0].2, Some(0));
        assert_eq!(l.regions.last().map(|r| r.2.as_str()), Some("trailer"));
        let (gaps, overlaps) = coverage(&m)?;
        assert_eq!(
            overlaps,
            vec![
                (0x60, 0x6C, "tile table end slot".into()),
                (0xB8, 0xBC, "trailer".into())
            ]
        );
        assert_eq!(gaps, vec![]);
        Ok(())
    }

    #[test]
    fn raises_what_the_reference_raises() {
        let mut m = record();
        put(&mut m, 0x44, 0x54);
        assert_eq!(layout(&m), Err(Error::Value("tile table at 0x54".into())));
        assert_eq!(
            layout(&record()[..0x40]).map_err(|e| (e.is_struct(), e.to_string())),
            Err((
                true,
                "unpack_from requires a buffer of at least 72 bytes for unpacking 4 bytes at offset 68 (actual buffer size is 64)".into()
            ))
        );
        let mut big = record();
        put(&mut big, 0x38, u32::MAX);
        assert_eq!(
            layout(&big),
            Err(Error::Short {
                size: 4,
                offset: 0xC0,
                len: 0xC0
            })
        );
        let mut trailer = record();
        let n = trailer.len();
        put(&mut trailer, n - 8, 0);
        assert_eq!(layout(&trailer), Err(Error::Value("mesh trailer not found".into())));
        assert_eq!(py_hex(-16), "-0x10");
        assert_eq!(read(&[0; 4], -8, 4), Err(Error::Range { offset: -8, len: 4 }));
    }

    #[test]
    fn nan_as_cpython_reads_it() {
        assert_eq!(c_double(f32::from_bits(0x7F80_0001)).to_bits(), 0x7FF8_0000_2000_0000);
        assert_eq!(c_double(f32::from_bits(0xFF80_0ABC)).to_bits(), 0xFFF8_0157_8000_0000);
        assert_eq!(c_double(f32::from_bits(0x7FC0_0000)).to_bits(), f64::NAN.to_bits());
        assert_eq!(c_double(1.5).to_bits(), 1.5f64.to_bits());
    }
}
