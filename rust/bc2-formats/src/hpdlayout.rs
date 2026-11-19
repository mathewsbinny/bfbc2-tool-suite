//! The byte layout of a sector's runtime-mesh record: a port of NavKit's `hpdtool.mesh_layout` /
//! `_object_end` / `mesh_coverage` (`bfbc2-auto-navkit/tools/hpdtool.py`), round F3. The walk labels every region
//! of the record it knows (header, tile table, tiles, the objects they name, the table's end slot, the trailer, the
//! link table, its lists and links) and returns the structure it read on the way; [`coverage`] sorts the regions and
//! reports the gaps between them and the overlaps.
//!
//! The same regions (in order, with the same labels), tiles, objects (in first-seen order), links and tile offsets
//! as the reference. Where the reference raises (its `ValueError`s, a read past the end) this answers [`Refused`] and
//! the Python runs, so the error stays its own; so does a NaN in a tile item (`struct`'s float conversion of a NaN is
//! the interpreter's).
//!
//! ```
//! use bc2_formats::hpdlayout;
//!
//! assert!(hpdlayout::layout(&[0u8; 16]).is_err());
//! ```

use std::collections::HashSet;
use std::fmt::Write as _;

/// The record is left to the Python reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("left to the Python reference")
    }
}

impl std::error::Error for Refused {}

type Result<T> = std::result::Result<T, Refused>;

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

fn u32_at(m: &[u8], at: i64) -> Result<u32> {
    let at = usize::try_from(at).map_err(|_| Refused)?;
    let w = m.get(at..at.checked_add(4).ok_or(Refused)?).ok_or(Refused)?;
    Ok(u32::from_le_bytes(w.try_into().map_err(|_| Refused)?))
}

fn f32_at(m: &[u8], at: i64) -> Result<f32> {
    let v = f32::from_bits(u32_at(m, at)?);
    if v.is_nan() { Err(Refused) } else { Ok(v) }
}

/// `count` records of `size` bytes from `at` fit in the record (checked before a loop the reference would run
/// until its read past the end).
fn fits(m: &[u8], at: i64, count: u64, size: u64) -> Result<()> {
    let end = i128::from(at) + i128::from(count) * i128::from(size);
    if at < 0 || end > m.len() as i128 {
        Err(Refused)
    } else {
        Ok(())
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
            return Err(Refused);
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
/// [`Refused`] where the reference raises, or a tile item's float is a NaN.
pub fn layout(m: &[u8]) -> Result<Layout> {
    let mut regions: Vec<Region> = vec![(0, 0x50, "header".into())];
    let (nx, nz) = (u32_at(m, 0x38)?, u32_at(m, 0x3C)?);
    let (tiles_off, links_n, links_off) = (u32_at(m, 0x44)?, u32_at(m, 0x48)?, u32_at(m, 0x4C)?);
    if tiles_off != 0x50 {
        return Err(Refused);
    }
    let tile_count = u64::from(nx) * u64::from(nz);
    fits(m, 0x50, tile_count + 1, 4)?;
    let tc = i64::try_from(tile_count).map_err(|_| Refused)?;
    regions.push((0x50, 0x50 + (tc + 1) * 4, "tile table".into()));
    let tile_offs = (0..=tc).map(|i| u32_at(m, 0x50 + i * 4)).collect::<Result<Vec<_>>>()?;
    let mut objects: Vec<(u32, i64)> = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    let mut tiles = Vec::with_capacity(tile_offs.len().saturating_sub(1));
    for (t, &to) in tile_offs.iter().take(tile_offs.len() - 1).enumerate() {
        let to64 = i64::from(to);
        let n = u32_at(m, to64)?;
        fits(m, to64 + 4, u64::from(n), 12)?;
        let mut items = Vec::with_capacity(n as usize);
        for k in 0..i64::from(n) {
            let at = to64 + 4 + k * 12;
            items.push((u32_at(m, at)?, f32_at(m, at + 4)?, f32_at(m, at + 8)?));
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
    let end_slot = i64::from(*tile_offs.last().ok_or(Refused)?);
    regions.push((end_slot, end_slot + 4, "tile table end slot".into()));
    let len = m.len() as i64;
    if len < 8 || i64::from(u32_at(m, len - 8)?) != len - 8 || u32_at(m, len - 4)? != 0 {
        return Err(Refused);
    }
    regions.push((len - 8, len, "trailer".into()));
    let mut links = Vec::new();
    if links_n != 0 {
        let lo = i64::from(links_off);
        regions.push((lo, lo + i64::from(links_n) * 12, "link table".into()));
        fits(m, lo, u64::from(links_n), 12)?;
        for k in 0..i64::from(links_n) {
            let (sid, cnt, lst) = (
                u32_at(m, lo + k * 12)?,
                u32_at(m, lo + k * 12 + 4)?,
                u32_at(m, lo + k * 12 + 8)?,
            );
            let l64 = i64::from(lst);
            regions.push((l64, l64 + i64::from(cnt) * 4, format!("link list {sid:#x}")));
            fits(m, l64, u64::from(cnt), 4)?;
            for j in 0..i64::from(cnt) {
                let lo = i64::from(u32_at(m, l64 + j * 4)?);
                let c = i64::from(u32_at(m, lo + 0xC)?);
                let mut label = String::new();
                let _ = write!(label, "link {sid:#x}/{j}");
                regions.push((lo, lo + 0x10 + c * 4, label));
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
/// [`Refused`] where [`layout`] refuses.
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
    fn refuses_what_the_reference_raises_on() {
        let mut m = record();
        put(&mut m, 0x44, 0x54);
        assert_eq!(layout(&m), Err(Refused));
        assert_eq!(layout(&record()[..0x40]), Err(Refused));
        let mut big = record();
        put(&mut big, 0x38, u32::MAX);
        assert_eq!(layout(&big), Err(Refused));
    }
}
