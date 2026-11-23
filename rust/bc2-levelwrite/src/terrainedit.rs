//! The terrain's resources written from a project's strokes (round E5; spec `transforms/terrain_edit.md`
//! TEDIT-HF-3, TEDIT-MM-1): a height-field stroke's raster ([`crate::files::Target::Terrain`]) becomes the main block
//! with its derived parts rebuilt over the edited region, a material stroke's raster
//! ([`crate::files::Target::Material`]) the material map coded again. Nothing edited: the base's bytes, unchanged.
//!
//! The codec is [`bc2_formats::terrain_codec`] (shared with round TG1's generator); this module is the edit layer
//! above it: what changed, where, and the checks an export runs on what it wrote ([`check_heights`],
//! [`check_materials`]).

use bc2_formats::terrain_codec::{MainBlock, MaterialMap, Region, SubBlock, minmax_tree};
use bc2_terragen::ring::{PLACES, RING_N};

use crate::delta::Raster;
use crate::write::LevelWriteError;

/// The rectangle of samples where two rasters (or a raster and samples) differ; `None` when they are equal.
#[must_use]
pub fn changed_region(base: &[u16], edited: &[u16], width: usize) -> Option<Region> {
    let mut r: Option<Region> = None;
    for (k, (a, b)) in base.iter().zip(edited).enumerate() {
        if a != b {
            let (x, z) = (k % width.max(1), k / width.max(1));
            let one = Region {
                x0: x,
                z0: z,
                x1: x + 1,
                z1: z + 1,
            };
            r = Some(r.map_or(one, |r| r.union(one)));
        }
    }
    r
}

/// A height-field file with the raster's heights (TEDIT-HF-3): the tree computed again and the envelope lowered over
/// the changed region, the depth map, bounds and header kept. The base's own bytes when nothing changed.
///
/// # Errors
/// The base is not a main block, or the raster is not its size.
pub fn write_heights(base: &[u8], raster: &Raster) -> Result<Vec<u8>, LevelWriteError> {
    write_heights_in(base, raster, None)
}

/// A raster's stored heights for another height range (TEDIT-HF-2): each value scaled by `from` / `to` (the max y
/// before and after, metres; min y 0), so every sample keeps its height in metres to within a stored unit.
#[must_use]
pub fn rescale(raster: &Raster, from: f64, to: f64) -> Raster {
    let k = from / to.max(f64::MIN_POSITIVE);
    let samples: Vec<u16> = raster
        .samples()
        .iter()
        .map(|&v| (f64::from(v) * k).round().clamp(0.0, 65535.0) as u16)
        .collect();
    Raster::from_samples(raster.width(), raster.height(), &samples)
}

/// [`write_heights`] with the height range widened to `max_y` metres (round E5-rest, TEDIT-HF-2: the raster's heights
/// already in the new range's units, [`rescale`]): the bounds' max y the new one, the lower envelope's grids scaled
/// down with the heights (each value floored, so it stays under them), the tree and envelope rebuilt over the whole
/// block. `None`: the base's range.
///
/// # Errors
/// As [`write_heights`]; a range narrower than the base's.
pub fn write_heights_in(base: &[u8], raster: &Raster, max_y: Option<f32>) -> Result<Vec<u8>, LevelWriteError> {
    let mut b = MainBlock::decode(base).map_err(|e| LevelWriteError::Refused(format!("the height field: {e}")))?;
    if let Some(to) = max_y.filter(|t| (*t - b.max[1]).abs() > f32::EPSILON) {
        let from = b.max[1] - b.min[1];
        if to - b.min[1] < from {
            return Err(LevelWriteError::Refused(format!(
                "the height range is {from} m; a range is only widened (to {to} m)"
            )));
        }
        if (raster.width(), raster.height()) != (b.size, b.size) {
            return Err(LevelWriteError::Refused(format!(
                "a height-field stroke of {} x {} samples on a block of {}",
                raster.width(),
                raster.height(),
                b.size
            )));
        }
        let k = f64::from(from) / f64::from(to - b.min[1]);
        for g in &mut b.envelope {
            for v in g.iter_mut() {
                *v = (f64::from(*v) * k).floor() as u16;
            }
        }
        b.max[1] = to;
        b.heights = raster.samples();
        let n = b.size as usize;
        b.rebuild(Region {
            x0: 0,
            z0: 0,
            x1: n,
            z1: n,
        });
        return Ok(b.encode());
    }
    if (raster.width(), raster.height()) != (b.size, b.size) {
        return Err(LevelWriteError::Refused(format!(
            "a height-field stroke of {} x {} samples on a block of {}",
            raster.width(),
            raster.height(),
            b.size
        )));
    }
    let samples = raster.samples();
    let Some(region) = changed_region(&b.heights, &samples, b.size as usize) else {
        return Ok(base.to_vec());
    };
    b.heights = samples;
    // one sample beyond: the tree nodes and envelope squares whose borders the changed samples are
    b.rebuild(Region {
        x0: region.x0.saturating_sub(1),
        z0: region.z0.saturating_sub(1),
        x1: region.x1 + 1,
        z1: region.z1 + 1,
    });
    Ok(b.encode())
}

/// A material-map file with the raster's cells (TEDIT-MM-1): every row coded as DICE codes it, the names kept. The
/// base's own bytes when nothing changed.
///
/// # Errors
/// The base does not read, the raster is not its size, or a cell names a material the map lacks.
pub fn write_materials(base: &[u8], raster: &Raster) -> Result<Vec<u8>, LevelWriteError> {
    write_materials_named(base, raster, &[])
}

/// [`write_materials`] with `added` names after the base's (round E5-rest: a material the map lacked, at most 16
/// names in all, TERR-MM-2), each named once.
///
/// # Errors
/// As [`write_materials`]; more than 16 names.
pub fn write_materials_named(base: &[u8], raster: &Raster, added: &[String]) -> Result<Vec<u8>, LevelWriteError> {
    let mut m = MaterialMap::decode(base).map_err(|e| LevelWriteError::Refused(format!("the material map: {e}")))?;
    for a in added {
        if !m.names.iter().any(|n| n.eq_ignore_ascii_case(a)) {
            m.names.push(a.clone());
        }
    }
    if m.names.len() > MAX_MATERIALS {
        return Err(LevelWriteError::Refused(format!(
            "a material map holds at most {MAX_MATERIALS} names (TERR-MM-2); this one would hold {}",
            m.names.len()
        )));
    }
    if (raster.width(), raster.height()) != (m.width, m.width) {
        return Err(LevelWriteError::Refused(format!(
            "a material stroke of {} x {} cells on a map of {}",
            raster.width(),
            raster.height(),
            m.width
        )));
    }
    let cells: Vec<u8> = raster.samples().iter().map(|&v| v.min(255) as u8).collect();
    if let Some(bad) = cells.iter().find(|&&c| usize::from(c) >= m.names.len()) {
        return Err(LevelWriteError::Refused(format!(
            "a material cell names index {bad}; the map has {} materials",
            m.names.len()
        )));
    }
    if cells == m.cells && added.is_empty() {
        return Ok(base.to_vec());
    }
    m.cells = cells;
    Ok(m.encode())
}

/// The materials of a level's grid a terrain cell may carry (MAT-GRID-1 / -2, TEDIT-MM-1): the `Entity.MaterialData`
/// names of its `Materials_Win32` partition whose `Category` is `Terrain`, `Default` or `Construction` (DICE's maps
/// name `Stone` and `Concrete`, construction materials, beside the terrain ones), in the grid's order. A material the
/// material map lacks is added from these (TEDIT-MM-1), so every name the map holds is one the level's grid knows.
#[must_use]
pub fn grid_terrain_materials(grid: &[u8]) -> Vec<String> {
    let Ok(d) = bc2_formats::dbx::edit::EditDbx::parse(grid) else {
        return Vec::new();
    };
    let text = |e, name: &[u8]| {
        d.child(e, name)
            .and_then(|c| d.text(c))
            .map(|t| String::from_utf8_lossy(t).into_owned())
    };
    d.of_type(b".MaterialData")
        .into_iter()
        .filter(|&e| matches!(text(e, b"Category").as_deref(), Some("Terrain" | "Default" | "Construction")))
        .filter_map(|e| text(e, b"Name"))
        .collect()
}

/// The most names a material map holds (TERR-MM-2: a cell is 4 bits).
pub const MAX_MATERIALS: usize = 16;

/// A material map's cells as a raster (indices as `u16`s), for the paint brush and the strokes' sources.
#[must_use]
pub fn material_raster(m: &MaterialMap) -> Raster {
    let cells: Vec<u16> = m.cells.iter().map(|&c| u16::from(c)).collect();
    Raster::from_samples(m.width, m.width, &cells)
}

/// A raster from a stroke target's bytes as [`crate::files::LevelFiles::edited`] gives them (little-endian `u16`s).
#[must_use]
pub fn raster_of(bytes: &[u8], width: u32, height: u32) -> Raster {
    let samples: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    Raster::from_samples(width, height, &samples)
}

// ------------------------------------------------------------------------------------------------ the ring

/// The blocks of DICE's single ring (TERR-HF-12).
pub const RING_BLOCKS: usize = PLACES.len();

/// Samples a side of the ring's raster ([`ring_raster`]): four ring blocks' worth.
pub const RING_SIDE: u32 = 4 * RING_N as u32;

/// The 12 ring blocks (in [`PLACES`] order, each [`RING_N`] x [`RING_N`]) as one raster over the outer square (round
/// E5-rest): sample (`I`, `J`) is block (`I / 64`, `J / 64`)'s sample (`I % 64`, `J % 64`), the outer square's min
/// corner first; the main block's square inside holds the main block's knots (every `st`-th sample), for the views.
#[must_use]
pub fn ring_raster(main: &[u16], n: usize, blocks: &[Vec<u16>]) -> Raster {
    let rn = RING_N;
    let st = (n / 2 / rn).max(1);
    let side = RING_SIDE as usize;
    let mut out = vec![0u16; side * side];
    for (b, &(px, pz)) in blocks.iter().zip(PLACES.iter()) {
        let (ox, oz) = (((px + 2) as usize) * rn, ((pz + 2) as usize) * rn);
        for j in 0..rn {
            for i in 0..rn {
                if let (Some(v), Some(slot)) = (b.get(j * rn + i), out.get_mut((oz + j) * side + ox + i)) {
                    *slot = *v;
                }
            }
        }
    }
    for j in 0..2 * rn {
        for i in 0..2 * rn {
            let (mx, mz) = ((i * st).min(n - 1), (j * st).min(n - 1));
            if let (Some(v), Some(slot)) = (main.get(mz * n + mx), out.get_mut((rn + j) * side + rn + i)) {
                *slot = *v;
            }
        }
    }
    Raster::from_samples(RING_SIDE, RING_SIDE, &out)
}

/// The ring raster's 12 blocks again ([`ring_raster`]'s inverse), in [`PLACES`] order.
#[must_use]
pub fn split_ring(r: &Raster) -> Vec<Vec<u16>> {
    let rn = RING_N;
    PLACES
        .iter()
        .map(|&(px, pz)| {
            let (ox, oz) = (((px + 2) as usize) * rn, ((pz + 2) as usize) * rn);
            (0..rn * rn)
                .map(|k| r.get((ox + k % rn) as u32, (oz + k / rn) as u32))
                .collect()
        })
        .collect()
}

/// The main block and its 12 ring blocks written together (round E5-rest, TERR-HF-12): `main` the main block as the
/// edit leaves it (encoded), `ring` the ring raster the strokes left (`None`: the base's ring); the ring blocks
/// rescaled when the main block's range was widened (their stored values share its scale), then DICE's stitching
/// (`bc2_terragen::ring::stitch`: the main edges straight between the ring's knots, the ring's seams the knots, each
/// block's last row and column its neighbour's first), the main block's tree and envelope rebuilt where the
/// stitching moved its edges. Returns the main block and each ring block's file; a ring block nothing changed keeps
/// its base bytes, and when neither the main block's edges nor the ring changed everything is returned as given.
///
/// # Errors
/// A file does not decode, or there are not 12 ring blocks of [`RING_N`] samples.
pub fn write_ring(
    main_base: &[u8],
    main: &[u8],
    ring_bases: &[Vec<u8>],
    ring: Option<&Raster>,
) -> Result<(Vec<u8>, Vec<Vec<u8>>), LevelWriteError> {
    let bad = |e: String| LevelWriteError::Refused(format!("the sub-terrain ring: {e}"));
    let b0 = MainBlock::decode(main_base).map_err(|e| bad(e.to_string()))?;
    let mut m = MainBlock::decode(main).map_err(|e| bad(e.to_string()))?;
    let mut subs: Vec<SubBlock> = ring_bases
        .iter()
        .map(|d| SubBlock::decode(d).map_err(|e| bad(e.to_string())))
        .collect::<Result<_, _>>()?;
    if subs.len() != PLACES.len() || subs.iter().any(|s| s.size as usize != RING_N) {
        return Err(bad(format!(
            "{} blocks; DICE's single rings have {} of {RING_N} samples (TERR-HF-12)",
            subs.len(),
            PLACES.len()
        )));
    }
    let n = m.size as usize;
    let edge = |h: &[u16]| -> Vec<u16> {
        (0..n)
            .flat_map(|k| [h[k], h[(n - 1) * n + k], h[k * n], h[k * n + n - 1]])
            .collect()
    };
    let widened = (m.max[1] - b0.max[1]).abs() > f32::EPSILON;
    let mut blocks: Vec<Vec<u16>> = match ring {
        Some(r) => split_ring(r),
        None => subs.iter().map(|s| s.heights.clone()).collect(),
    };
    if !widened && ring.is_none() && edge(&m.heights) == edge(&b0.heights) {
        return Ok((main.to_vec(), ring_bases.to_vec()));
    }
    // the ring's stored values in the main block's scale (a widened range)
    if widened && ring.is_none() {
        for (b, s) in blocks.iter_mut().zip(&subs) {
            let k = f64::from(s.max[1] - s.min[1]) / f64::from(m.max[1] - m.min[1]).max(f64::MIN_POSITIVE);
            for v in b.iter_mut() {
                *v = (f64::from(*v) * k).round().clamp(0.0, 65535.0) as u16;
            }
        }
    }
    let before = m.heights.clone();
    let st = (n / 2 / RING_N).max(1);
    bc2_terragen::ring::stitch(&mut m.heights, n, st, &mut blocks);
    if let Some(r) = changed_region(&before, &m.heights, n) {
        m.rebuild(Region {
            x0: r.x0.saturating_sub(1),
            z0: r.z0.saturating_sub(1),
            x1: r.x1 + 1,
            z1: r.z1 + 1,
        });
    }
    let mut out = Vec::with_capacity(subs.len());
    for ((s, b), base) in subs.iter_mut().zip(blocks).zip(ring_bases) {
        if s.heights == b && (s.max[1] - m.max[1]).abs() <= f32::EPSILON {
            out.push(base.clone());
            continue;
        }
        s.heights = b;
        s.max[1] = m.max[1];
        s.min[1] = m.min[1];
        out.push(s.encode());
    }
    Ok((m.encode(), out))
}

/// What is wrong with an exported height field against its base (XVAL-TERR-2), in words; empty when it is right:
/// it reads as a main block of the base's size, bounds and depth map; its min / max tree is the one its heights give;
/// every sample the edit changed lies at or over the finest lower-envelope grid.
#[must_use]
pub fn check_heights(base: &[u8], edited: &[u8]) -> Vec<String> {
    let (b, e) = match (MainBlock::decode(base), MainBlock::decode(edited)) {
        (Ok(b), Ok(e)) => (b, e),
        (_, Err(e)) => return vec![format!("does not read as a main height-field block: {e}")],
        (Err(e), _) => return vec![format!("the base does not read: {e}")],
    };
    let mut out = Vec::new();
    if (e.size, e.min, [e.max[0], e.max[2]]) != (b.size, b.min, [b.max[0], b.max[2]]) || e.max[1] < b.max[1] {
        out.push("its size or bounds are not the base's (only its max y may grow: TEDIT-HF-2)".to_owned());
    }
    if e.depth != b.depth {
        out.push("its depth map is not the base's (a sculpt keeps it, TEDIT-HF-3)".to_owned());
    }
    let n = e.size as usize;
    if e.tree != minmax_tree(&e.heights, n) {
        out.push("its min / max tree is not the one its heights give".to_owned());
    }
    if let (Some(g), Some(&side)) = (e.envelope.last(), bc2_formats::terrain_codec::envelope_sides(n).last()) {
        let poke = e
            .heights
            .iter()
            .zip(&b.heights)
            .enumerate()
            .filter(|(_, (a, o))| a != o)
            .filter(|(k, (a, _))| {
                f64::from(**a) < bc2_formats::terrain_codec::envelope_at(g, side, n, k % n, k / n) - 0.5
            })
            .count();
        if poke > 0 {
            out.push(format!("{poke} edited samples lie under its lower envelope"));
        }
    }
    out
}

/// What is wrong with an exported material map against its base (XVAL-TERR-2), in words; empty when it is right: it
/// reads, has the base's width and names, and every cell names one of them.
#[must_use]
pub fn check_materials(base: &[u8], edited: &[u8]) -> Vec<String> {
    let (b, e) = match (MaterialMap::decode(base), MaterialMap::decode(edited)) {
        (Ok(b), Ok(e)) => (b, e),
        (_, Err(e)) => return vec![format!("does not read as a material map: {e}")],
        (Err(e), _) => return vec![format!("the base does not read: {e}")],
    };
    let mut out = Vec::new();
    if e.width != b.width || e.names.get(..b.names.len()) != Some(b.names.as_slice()) || e.names.len() > MAX_MATERIALS
    {
        out.push("its width or names are not the base's (the base's names first, then those added)".to_owned());
    }
    if e.cells.iter().any(|&c| usize::from(c) >= e.names.len()) {
        out.push("a cell names no material of the map".to_owned());
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// A 1,024-sample main block and its ring of 12 stitched blocks (DICE's layout, TERR-HF-12), all 10,000.
    fn ringed() -> (Vec<u8>, Vec<Vec<u8>>) {
        let n = 1024usize;
        let main = MainBlock::new([-512.0, 0.0, -512.0, 512.0, 500.0, 512.0], vec![10_000; n * n], None)
            .unwrap()
            .encode();
        let blocks = PLACES
            .iter()
            .map(|&(px, pz)| {
                let (x0, z0) = (px as f32 * 512.0, pz as f32 * 512.0);
                SubBlock::new([x0, 0.0, z0, x0 + 512.0, 500.0, z0 + 512.0], vec![10_000; RING_N * RING_N])
                    .unwrap()
                    .encode()
            })
            .collect();
        (main, blocks)
    }

    #[test]
    fn the_ring_is_one_raster_and_stays_stitched() {
        let (main, ring) = ringed();
        let m = MainBlock::decode(&main).unwrap();
        let blocks: Vec<Vec<u16>> = ring.iter().map(|b| SubBlock::decode(b).unwrap().heights).collect();
        let r = ring_raster(&m.heights, 1024, &blocks);
        assert_eq!((r.width(), r.height()), (RING_SIDE, RING_SIDE));
        assert_eq!(split_ring(&r), blocks, "split is the raster's inverse");
        // nothing changed: every byte as given
        let (m2, r2) = write_ring(&main, &main, &ring, None).unwrap();
        assert_eq!((m2 == main, r2 == ring), (true, true));
        // a hill on the main block's west edge: the ring's seams follow it, stitched as DICE stitches
        let mut hill = m.clone();
        for z in 400..600 {
            for x in 0..60 {
                hill.heights[z * 1024 + x] = 20_000;
            }
        }
        hill.rebuild(Region {
            x0: 0,
            z0: 399,
            x1: 61,
            z1: 601,
        });
        let (m3, r3) = write_ring(&main, &hill.encode(), &ring, None).unwrap();
        let mb = MainBlock::decode(&m3).unwrap();
        let rb: Vec<Vec<u16>> = r3.iter().map(|b| SubBlock::decode(b).unwrap().heights).collect();
        let s = bc2_terragen::ring::stitching(&mb.heights, 1024, &rb);
        assert_eq!(s.main_seams.0, s.main_seams.1);
        assert_eq!(s.ring_seams.0, s.ring_seams.1);
        assert!(s.straight <= 0.5, "{}", s.straight);
        assert!(r3.iter().zip(&ring).any(|(a, b)| a != b), "a ring block took the hill's edge");
        assert!(check_heights(&main, &m3).is_empty(), "{:?}", check_heights(&main, &m3));
        // a stroke on the ring itself: the dug block rewritten, the others stitched to it
        let mut dug = r.clone();
        for j in 0..40 {
            for i in 0..40 {
                dug.set(i, j, 2_000);
            }
        }
        let (_, r4) = write_ring(&main, &main, &ring, Some(&dug)).unwrap();
        assert_eq!(SubBlock::decode(&r4[0]).unwrap().heights[5 * 64 + 5], 2_000);
        // a widened main block: the ring's stored values scaled with it
        let wide = write_heights_in(&main, &rescale(&Raster::from_samples(1024, 1024, &m.heights), 500.0, 1000.0), Some(1000.0))
            .unwrap();
        let (_, r5) = write_ring(&main, &wide, &ring, None).unwrap();
        let b5 = SubBlock::decode(&r5[0]).unwrap();
        assert_eq!((b5.max[1], b5.heights[100]), (1000.0, 5_000));
    }

    #[test]
    fn heights_and_materials_written_and_checked() {
        let n = 64usize;
        let h: Vec<u16> = (0..n * n).map(|i| (1000 + (i % n) * 10) as u16).collect();
        let base = MainBlock::new([0.0, 0.0, 0.0, 64.0, 500.0, 64.0], h.clone(), None)
            .unwrap()
            .encode();
        let r = Raster::from_samples(n as u32, n as u32, &h);
        assert_eq!(
            write_heights(&base, &r).unwrap(),
            base,
            "nothing edited: the base's bytes"
        );
        let mut pit = r.clone();
        for z in 20..30 {
            for x in 20..30 {
                pit.set(x, z, 10);
            }
        }
        let out = write_heights(&base, &pit).unwrap();
        assert!(
            check_heights(&base, &out).is_empty(),
            "{:?}",
            check_heights(&base, &out)
        );
        // the heights written without the rebuild: the tree and the envelope are stale
        let mut stale = MainBlock::decode(&base).unwrap();
        stale.heights = pit.samples();
        let problems = check_heights(&base, &stale.encode());
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(write_heights(&base, &Raster::new(32, 32, 0)).is_err());

        let mut m = MaterialMap::new(512, vec!["defaultmaterial".into(), "grass".into()]).unwrap();
        m.set(3, 3, 1);
        let mb = m.encode();
        let mut cells = material_raster(&m);
        assert_eq!(write_materials(&mb, &cells).unwrap(), mb);
        cells.set(100, 200, 1);
        let out = write_materials(&mb, &cells).unwrap();
        assert_eq!(check_materials(&mb, &out), Vec::<String>::new());
        assert_eq!(MaterialMap::decode(&out).unwrap().get(100, 200), 1);
        cells.set(0, 0, 5);
        assert!(write_materials(&mb, &cells).is_err(), "no material 5");
        // round E5-rest: a material added after the base's names (TEDIT-MM-1), at most 16 names
        cells.set(0, 0, 2);
        let out = write_materials_named(&mb, &cells, &["tarmac".into()]).unwrap();
        let back = MaterialMap::decode(&out).unwrap();
        assert_eq!(back.names, ["defaultmaterial", "grass", "tarmac"]);
        assert_eq!(back.get(0, 0), 2);
        assert!(check_materials(&mb, &out).is_empty());
        let many: Vec<String> = (0..15).map(|k| format!("m{k}")).collect();
        assert!(write_materials_named(&mb, &cells, &many).is_err());
        assert_eq!(
            grid_terrain_materials(b"not a partition"),
            Vec::<String>::new()
        );
        // round E5-rest: the range widened (TEDIT-HF-2): the ground stays, the max y grows, the parts rebuilt
        let wide = rescale(&r, 500.0, 800.0);
        let out = write_heights_in(&base, &wide, Some(800.0)).unwrap();
        let e = MainBlock::decode(&out).unwrap();
        assert_eq!(e.max[1], 800.0);
        let worst = e
            .heights
            .iter()
            .zip(&h)
            .map(|(a, b)| (f64::from(*a) * 800.0 - f64::from(*b) * 500.0).abs() / 65535.0)
            .fold(0.0, f64::max);
        assert!(worst < 0.01, "{worst} m");
        assert!(check_heights(&base, &out).is_empty(), "{:?}", check_heights(&base, &out));
        assert!(write_heights_in(&base, &r, Some(400.0)).is_err(), "never narrowed");
        assert_eq!(
            changed_region(&[1, 2, 3, 4], &[1, 2, 9, 4], 2),
            Some(Region {
                x0: 0,
                z0: 1,
                x1: 1,
                z1: 2
            })
        );
    }
}
