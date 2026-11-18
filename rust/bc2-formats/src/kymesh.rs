//! NavKit's reader of the `KynogonRuntimeMesh` record inside `pathdata.hpd` sector blocks
//! (`bfbc2-auto-navkit/tools/kymesh.py`, `parse_mesh`; layout in `research/KYNAPSE_SPEC.md` 4.4 / 4.5).
//!
//! Round E3o: the same structure as the reference, value for value: the record's offsets replaced by references
//! (an area by its index in the order the tiles first name it, a portal's target by that index, a neighbour link's
//! pointer by its owning area and the offset inside it). Where the reference would raise (a short record, a read past
//! the end, A records, an unknown portal target, a link pointer before every area) this returns an error and the
//! Python reads the record itself, so the error stays the reference's. A NaN anywhere is also left to the reference:
//! `struct`'s float conversion of a NaN is the interpreter's to define.

use std::collections::HashMap;

use crate::bytes::array;

/// `(x, y, z)` as stored.
pub type Vec3 = [f32; 3];

const MARKER: &[u8] = b"KynogonRuntimeMesh\0\0";

/// Why the record is left to the reference reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Not a runtime-mesh record.
    Marker,
    /// A read past the record's end, or an offset that does not fit.
    Short,
    /// An area with A records (the reference refuses them).
    ARecords,
    /// A portal target or link pointer the reference cannot resolve.
    Reference,
    /// A NaN value.
    Nan,
}

type Result<T> = std::result::Result<T, Error>;

/// B: a portal to another area of the same sector.
#[derive(Debug, Clone, PartialEq)]
pub struct Portal {
    /// One end.
    pub p0: Vec3,
    /// The other end.
    pub p1: Vec3,
    /// Index of the target area.
    pub target: usize,
}

/// C: a portal on the sector border.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderPortal {
    /// One end.
    pub p0: Vec3,
    /// The other end.
    pub p1: Vec3,
    /// Out of the sector (or "up"); for a cell, its direction.
    pub direction: Vec3,
    /// Filled by the game at run time (NULL in files).
    pub runtime: (u32, u32),
}

/// F: a pitch-sized cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Out of the sector (or "up"); for a cell, its direction.
    pub direction: Vec3,
    /// The cell's points.
    pub points: Vec<Vec3>,
}

/// One walkable area inside one tile.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    /// sector id, tile x, tile z, layer
    pub ids: [u32; 4],
    /// radius, pitch, max y, min y
    pub values: [f32; 4],
    /// B: portals to areas of the sector.
    pub portals: Vec<Portal>,
    /// C: portals on the sector border.
    pub border_portals: Vec<BorderPortal>,
    /// D lines.
    pub d_lines: Vec<Vec<Vec3>>,
    /// E: the outline's lines.
    pub outline: Vec<Vec<Vec3>>,
    /// F cells.
    pub cells: Vec<Cell>,
    /// The floats of the F table's terminating row.
    pub end_row: Vec3,
    /// (count field offset, raw offset) of the tables stored with count 0, in field order.
    pub null_offsets: Vec<(u32, u32)>,
}

/// One tile item: (area index, a, b).
#[derive(Debug, Clone, PartialEq)]
pub struct TileItem {
    /// The area's index.
    pub area: usize,
    /// Stored per item (the area's height range in the samples).
    pub a: f32,
    /// Stored per item.
    pub b: f32,
}

/// A neighbour link's object: its header and (area index, offset inside the area) references.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkObject {
    /// The object's three header words.
    pub header: [u32; 3],
    /// (area index, byte offset inside that area).
    pub refs: Vec<(usize, u64)>,
}

/// The links to one neighbour sector.
#[derive(Debug, Clone, PartialEq)]
pub struct NeighbourLink {
    /// The neighbour sector.
    pub sector_id: u32,
    /// Its objects.
    pub objects: Vec<LinkObject>,
}

/// The parsed record.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// Header fields as stored.
    pub version: u16,
    /// Header flags.
    pub flags: u16,
    /// The loaded word.
    pub loaded: u32,
    /// The sector id.
    pub sector_id: u32,
    /// Sampling resolution.
    pub resolution: f32,
    /// Cell pitch.
    pub pitch: f32,
    /// min x, max x, min z, max z.
    pub rect: [f32; 4],
    /// Tiles across.
    pub tiles_x: u32,
    /// Tiles down.
    pub tiles_z: u32,
    /// Agent radius.
    pub radius: f32,
    /// Per tile its items.
    pub tiles: Vec<Vec<TileItem>>,
    /// The areas.
    pub areas: Vec<Area>,
    /// The neighbour links.
    pub links: Vec<NeighbourLink>,
    /// 4 bytes the loader never reads.
    pub end_slot: Vec<u8>,
    /// The link table's offset when there are no links, else `None` (the reference's NULL).
    pub links_null_offset: Option<u32>,
}

struct R<'a>(&'a [u8]);

impl R<'_> {
    fn u32(&self, at: usize) -> Result<u32> {
        array::<4>(self.0, at).map(u32::from_le_bytes).ok_or(Error::Short)
    }
    fn u16(&self, at: usize) -> Result<u16> {
        array::<2>(self.0, at).map(u16::from_le_bytes).ok_or(Error::Short)
    }
    fn f32(&self, at: usize) -> Result<f32> {
        let v = array::<4>(self.0, at).map(f32::from_le_bytes).ok_or(Error::Short)?;
        if v.is_nan() { Err(Error::Nan) } else { Ok(v) }
    }
    fn v3(&self, at: usize) -> Result<Vec3> {
        Ok([self.f32(at)?, self.f32(add(at, 4)?)?, self.f32(add(at, 8)?)?])
    }
    /// `_points(m, a, b)`: a point every 12 bytes from `a` up to (not including) `b`.
    fn points(&self, a: usize, b: usize) -> Result<Vec<Vec3>> {
        let mut out = Vec::with_capacity(b.saturating_sub(a).min(self.0.len()).div_ceil(12));
        let mut q = a;
        while q < b {
            out.push(self.v3(q)?);
            q += 12;
        }
        Ok(out)
    }
}

fn add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b).ok_or(Error::Short)
}

fn at(base: usize, k: usize, size: usize) -> Result<usize> {
    add(base, k.checked_mul(size).ok_or(Error::Short)?)
}

/// `count + 1` little-endian `u32`s of a table at `tab` (a table past the record's end is the reference's error).
fn table(r: &R<'_>, tab: usize, count: usize) -> Result<Vec<usize>> {
    if at(tab, count, 4)? >= r.0.len() {
        return Err(Error::Short);
    }
    (0..=count).map(|i| Ok(r.u32(at(tab, i, 4)?)? as usize)).collect()
}

/// The six (count, offset) pairs of an area's tables, at 0x20, 0x28, ... 0x48.
const TABLES: [usize; 6] = [0x20, 0x28, 0x30, 0x38, 0x40, 0x48];

/// One area at `o`; its portals' raw targets beside it (resolved once every area is known).
fn parse_area(r: &R<'_>, o: usize) -> Result<(Area, Vec<usize>)> {
    let ids = [r.u32(o)?, r.u32(add(o, 4)?)?, r.u32(add(o, 8)?)?, r.u32(add(o, 12)?)?];
    let values = [
        r.f32(add(o, 16)?)?,
        r.f32(add(o, 20)?)?,
        r.f32(add(o, 24)?)?,
        r.f32(add(o, 28)?)?,
    ];
    let mut cnt = [0usize; 6];
    let mut offs = [0usize; 6];
    for ((c, of), &k) in cnt.iter_mut().zip(offs.iter_mut()).zip(&TABLES) {
        *c = r.u32(add(o, k)?)? as usize;
        *of = r.u32(add(o, k + 4)?)? as usize;
    }
    let null_offsets = TABLES
        .iter()
        .zip(cnt.iter().zip(&offs))
        .filter(|(_, (c, _))| **c == 0)
        .map(|(&k, (_, &off))| (k as u32, off as u32))
        .collect();
    let [a_count, n_portals, n_borders, n_d, n_outline, n_cells] = cnt;
    let [_, portals_off, borders_off, d_off, outline_off, cells_off] = offs;
    if a_count != 0 {
        return Err(Error::ARecords);
    }
    let room = r.0.len();
    let mut area = Area {
        ids,
        values,
        portals: Vec::with_capacity(n_portals.min(room)),
        border_portals: Vec::with_capacity(n_borders.min(room)),
        d_lines: Vec::new(),
        outline: Vec::new(),
        cells: Vec::new(),
        end_row: [0.0; 3],
        null_offsets,
    };
    let mut targets = Vec::with_capacity(n_portals.min(room));
    let pos = add(o, portals_off)?;
    for k in 0..n_portals {
        let q = at(pos, k, 0x20)?;
        let (p0, p1) = (r.v3(q)?, r.v3(add(q, 12)?)?);
        targets.push(r.u32(add(q, 0x18)?)? as usize);
        area.portals.push(Portal { p0, p1, target: 0 });
    }
    let pos = add(o, borders_off)?;
    for k in 0..n_borders {
        let q = at(pos, k, 0x2C)?;
        area.border_portals.push(BorderPortal {
            p0: r.v3(q)?,
            p1: r.v3(add(q, 12)?)?,
            direction: r.v3(add(q, 0x20)?)?,
            runtime: (r.u32(add(q, 0x18)?)?, r.u32(add(q, 0x1C)?)?),
        });
    }
    for (count, off, target) in [
        (n_d, d_off, &mut area.d_lines),
        (n_outline, outline_off, &mut area.outline),
    ] {
        if count != 0 {
            let idx = table(r, add(o, off)?, count)?;
            for w in idx.windows(2) {
                if let [a, b] = *w {
                    target.push(r.points(add(o, a)?, add(o, b)?)?);
                }
            }
        }
    }
    if n_cells != 0 {
        let tab = add(o, cells_off)?;
        if at(tab, n_cells, 16)? >= room {
            return Err(Error::Short);
        }
        let rows = (0..=n_cells)
            .map(|j| {
                let q = at(tab, j, 16)?;
                Ok((r.u32(q)? as usize, r.v3(add(q, 4)?)?))
            })
            .collect::<Result<Vec<(usize, Vec3)>>>()?;
        for w in rows.windows(2) {
            if let [(a, dir), (b, _)] = *w {
                area.cells.push(Cell {
                    direction: dir,
                    points: r.points(add(o, a)?, add(o, b)?)?,
                });
            }
        }
        area.end_row = rows.last().map_or([0.0; 3], |row| row.1);
    }
    Ok((area, targets))
}

/// (tiles, area offsets in index order, offset -> index).
type Tiles = (Vec<Vec<TileItem>>, Vec<usize>, HashMap<usize, usize>);

/// The tiles' items (`ntiles` tiles from `tile_offs`), every area numbered as the tiles first name it.
fn parse_tiles(rd: &R<'_>, tile_offs: &[usize]) -> Result<Tiles> {
    let mut area_index: HashMap<usize, usize> = HashMap::new();
    let mut area_offsets: Vec<usize> = Vec::new();
    let mut tiles = Vec::with_capacity(tile_offs.len());
    for &to in tile_offs {
        let n = rd.u32(to)? as usize;
        if at(add(to, 4)?, n, 12)? > rd.0.len() {
            return Err(Error::Short);
        }
        let mut items = Vec::with_capacity(n);
        for k in 0..n {
            let q = at(add(to, 4)?, k, 12)?;
            let ptr = rd.u32(q)? as usize;
            let (a, b) = (rd.f32(add(q, 4)?)?, rd.f32(add(q, 8)?)?);
            let next = area_offsets.len();
            let idx = *area_index.entry(ptr).or_insert(next);
            if idx == next {
                area_offsets.push(ptr);
            }
            items.push(TileItem { area: idx, a, b });
        }
        tiles.push(items);
    }
    Ok((tiles, area_offsets, area_index))
}

/// The neighbour links: a pointer's owner is the last area that starts at or before it.
fn parse_links(r: &R<'_>, n_links: usize, links_off: usize, area_offsets: &[usize]) -> Result<Vec<NeighbourLink>> {
    let mut sorted: Vec<(usize, usize)> = area_offsets.iter().enumerate().map(|(i, &o)| (o, i)).collect();
    sorted.sort_unstable();
    let room = r.0.len();
    let mut links = Vec::with_capacity(n_links.min(room));
    for k in 0..n_links {
        let q = at(links_off, k, 12)?;
        let (sid, cnt, lst) = (r.u32(q)?, r.u32(add(q, 4)?)? as usize, r.u32(add(q, 8)?)? as usize);
        let mut objects = Vec::with_capacity(cnt.min(room));
        for j in 0..cnt {
            let lo = r.u32(at(lst, j, 4)?)? as usize;
            let header = [r.u32(lo)?, r.u32(add(lo, 4)?)?, r.u32(add(lo, 8)?)?];
            let c = r.u32(add(lo, 0xC)?)? as usize;
            let mut refs = Vec::with_capacity(c.min(room));
            for qn in 0..c {
                let ptr = r.u32(at(add(lo, 0x10)?, qn, 4)?)? as usize;
                let n = sorted.partition_point(|&(o, _)| o <= ptr);
                let &(owner, idx) = n.checked_sub(1).and_then(|n| sorted.get(n)).ok_or(Error::Reference)?;
                refs.push((idx, (ptr - owner) as u64));
            }
            objects.push(LinkObject { header, refs });
        }
        links.push(NeighbourLink {
            sector_id: sid,
            objects,
        });
    }
    Ok(links)
}

/// `kymesh.parse_mesh(m)`.
///
/// # Errors
/// Where the reference raises (`Error::Marker`, `Short`, `ARecords`, `Reference`) or a value is a NaN (`Nan`).
pub fn parse(m: &[u8]) -> Result<Mesh> {
    if !m.starts_with(MARKER) {
        return Err(Error::Marker);
    }
    let r = R(m);
    let (version, flags) = (r.u16(0x14)?, r.u16(0x16)?);
    let (loaded, sector_id) = (r.u32(0x18)?, r.u32(0x1C)?);
    let (resolution, pitch) = (r.f32(0x20)?, r.f32(0x24)?);
    let rect = [r.f32(0x28)?, r.f32(0x2C)?, r.f32(0x30)?, r.f32(0x34)?];
    let (tiles_x, tiles_z) = (r.u32(0x38)?, r.u32(0x3C)?);
    let radius = r.f32(0x40)?;
    let (tile_tab, n_links, links_off) = (r.u32(0x44)? as usize, r.u32(0x48)? as usize, r.u32(0x4C)? as usize);
    let ntiles = (tiles_x as usize).checked_mul(tiles_z as usize).ok_or(Error::Short)?;
    let tile_offs = table(&r, tile_tab, ntiles)?;
    let (tiles, area_offsets, area_index) = parse_tiles(&r, tile_offs.get(..ntiles).unwrap_or(&[]))?;
    let mut areas = Vec::with_capacity(area_offsets.len());
    let mut raw_targets = Vec::with_capacity(area_offsets.len());
    for &o in &area_offsets {
        let (area, targets) = parse_area(&r, o)?;
        areas.push(area);
        raw_targets.push(targets);
    }
    for (area, targets) in areas.iter_mut().zip(raw_targets) {
        for (p, t) in area.portals.iter_mut().zip(targets) {
            p.target = *area_index.get(&t).ok_or(Error::Reference)?;
        }
    }
    let links = parse_links(&r, n_links, links_off, &area_offsets)?;
    let end = tile_offs.get(ntiles).copied().unwrap_or(0);
    let end_slot = m
        .get(end.min(m.len())..end.saturating_add(4).min(m.len()))
        .unwrap_or(&[])
        .to_vec();
    Ok(Mesh {
        version,
        flags,
        loaded,
        sector_id,
        resolution,
        pitch,
        rect,
        tiles_x,
        tiles_z,
        radius,
        tiles,
        areas,
        links,
        end_slot,
        links_null_offset: (n_links == 0).then_some(links_off as u32),
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::expect_used
)]
mod tests {
    use super::*;

    /// A record of one tile holding two areas (one with a portal to the other, an outline, a cell) and one link.
    fn record() -> Vec<u8> {
        let mut m = MARKER.to_vec();
        let w = |m: &mut Vec<u8>, v: u32| m.extend_from_slice(&v.to_le_bytes());
        let f = |m: &mut Vec<u8>, v: f32| m.extend_from_slice(&v.to_le_bytes());
        m.extend_from_slice(&3u16.to_le_bytes());
        m.extend_from_slice(&1u16.to_le_bytes());
        w(&mut m, 0);
        w(&mut m, 0x0102);
        f(&mut m, 0.8);
        f(&mut m, 0.8);
        for v in [0.0, 32.0, 0.0, 32.0] {
            f(&mut m, v);
        }
        w(&mut m, 1);
        w(&mut m, 1);
        f(&mut m, 0.5);
        // 0x44 tile table, 0x48 links, 0x4C link table: patched below
        let fix = m.len();
        m.extend_from_slice(&[0; 12]);
        let tile_tab = m.len();
        m.extend_from_slice(&[0; 8]);
        let tile0 = m.len();
        w(&mut m, 2);
        let items = m.len();
        m.extend_from_slice(&[0; 24]);
        let end_slot = m.len();
        m.extend_from_slice(b"ABCD");
        let mut area_at = vec![];
        for n in 0..2u32 {
            let o = m.len();
            area_at.push(o);
            for v in [0x0102, 0, 0, n] {
                w(&mut m, v);
            }
            for v in [0.5, 0.8, 3.0, 1.0] {
                f(&mut m, v);
            }
            let counts = m.len();
            m.extend_from_slice(&[0; 48]);
            let put = |m: &mut Vec<u8>, slot: usize, c: u32, off: usize| {
                m[counts + slot * 8..counts + slot * 8 + 4].copy_from_slice(&c.to_le_bytes());
                m[counts + slot * 8 + 4..counts + slot * 8 + 8].copy_from_slice(&((off - o) as u32).to_le_bytes());
            };
            put(&mut m, 0, 0, o + 0x99);
            if n == 0 {
                let b = m.len();
                for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
                    f(&mut m, v);
                }
                w(&mut m, 0); // target patched below
                w(&mut m, 0);
                put(&mut m, 1, 1, b);
            }
            // outline: one line of two points
            let pts = m.len();
            for v in [0.0f32, 1.0, 0.0, 2.0, 1.0, 0.0] {
                f(&mut m, v);
            }
            let tab = m.len();
            w(&mut m, (pts - o) as u32);
            w(&mut m, (tab - o) as u32);
            put(&mut m, 4, 1, tab);
            // a cell with one point
            let cp = m.len();
            for v in [9.0f32, 9.5, 9.0] {
                f(&mut m, v);
            }
            let rows = m.len();
            w(&mut m, (cp - o) as u32);
            for v in [0.0f32, 1.0, 0.0] {
                f(&mut m, v);
            }
            w(&mut m, (rows - o) as u32);
            for v in [7.0f32, 7.0, 7.0] {
                f(&mut m, v);
            }
            put(&mut m, 5, 1, rows);
        }
        // the portal of area 0 targets area 1
        let target = area_at[0] + 0x50 + 0x18;
        m[target..target + 4].copy_from_slice(&(area_at[1] as u32).to_le_bytes());
        // tile items: area 1 first, then area 0
        for (k, &o) in [area_at[1], area_at[0]].iter().enumerate() {
            m[items + k * 12..items + k * 12 + 4].copy_from_slice(&(o as u32).to_le_bytes());
            m[items + k * 12 + 4..items + k * 12 + 8]
                .copy_from_slice(&f32::from(u8::try_from(k).unwrap()).to_le_bytes());
        }
        m[tile_tab..tile_tab + 4].copy_from_slice(&(tile0 as u32).to_le_bytes());
        m[tile_tab + 4..tile_tab + 8].copy_from_slice(&(end_slot as u32).to_le_bytes());
        // one link object with a pointer 8 bytes into area 1
        let obj = m.len();
        for v in [1u32, 2, 3, 1, area_at[1] as u32 + 8] {
            w(&mut m, v);
        }
        let list = m.len();
        w(&mut m, obj as u32);
        let links = m.len();
        for v in [0x0103u32, 1, list as u32] {
            w(&mut m, v);
        }
        m[fix..fix + 4].copy_from_slice(&(tile_tab as u32).to_le_bytes());
        m[fix + 4..fix + 8].copy_from_slice(&1u32.to_le_bytes());
        m[fix + 8..fix + 12].copy_from_slice(&(links as u32).to_le_bytes());
        m
    }

    #[test]
    fn reads_the_record() {
        let mesh = parse(&record()).expect("parses");
        assert_eq!(mesh.areas.len(), 2);
        assert_eq!(mesh.tiles[0].iter().map(|t| t.area).collect::<Vec<_>>(), vec![0, 1]);
        // the tiles name area 1 first: it is index 0, and area 0's portal (to it) targets index 0
        assert_eq!(mesh.areas[1].portals[0].target, 0);
        assert_eq!(mesh.areas[1].portals[0].p1, [4.0, 5.0, 6.0]);
        assert_eq!(mesh.areas[0].outline, vec![vec![[0.0, 1.0, 0.0], [2.0, 1.0, 0.0]]]);
        assert_eq!(mesh.areas[0].cells[0].points, vec![[9.0, 9.5, 9.0]]);
        assert_eq!(mesh.areas[0].end_row, [7.0, 7.0, 7.0]);
        assert_eq!(mesh.links[0].objects[0].refs, vec![(0, 8)]);
        assert_eq!(mesh.end_slot, b"ABCD");
        assert_eq!(mesh.links_null_offset, None);
        assert_eq!(mesh.areas[0].null_offsets.len(), 4);
    }

    #[test]
    fn refuses_what_the_reference_raises_on() {
        let m = record();
        assert_eq!(parse(&m[..30]), Err(Error::Marker).or(Err(Error::Short)));
        assert_eq!(parse(b"nothing"), Err(Error::Marker));
        for cut in [0x50, 0x60, m.len() / 2, m.len() - 1] {
            assert!(parse(&m[..cut]).is_err(), "cut at {cut}");
        }
        let mut nan = m.clone();
        nan[0x20..0x24].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(parse(&nan), Err(Error::Nan));
    }
}
