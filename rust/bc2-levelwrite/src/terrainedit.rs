//! The terrain's resources written from a project's strokes (round E5; spec `transforms/terrain_edit.md`
//! TEDIT-HF-3, TEDIT-MM-1): a height-field stroke's raster ([`crate::files::Target::Terrain`]) becomes the main block
//! with its derived parts rebuilt over the edited region, a material stroke's raster
//! ([`crate::files::Target::Material`]) the material map coded again. Nothing edited: the base's bytes, unchanged.
//!
//! The codec is [`bc2_formats::terrain_codec`] (shared with round TG1's generator); this module is the edit layer
//! above it: what changed, where, and the checks an export runs on what it wrote ([`check_heights`],
//! [`check_materials`]).

use bc2_formats::terrain_codec::{MainBlock, MaterialMap, Region, minmax_tree};

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
    let mut b = MainBlock::decode(base).map_err(|e| LevelWriteError::Refused(format!("the height field: {e}")))?;
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
    let mut m = MaterialMap::decode(base).map_err(|e| LevelWriteError::Refused(format!("the material map: {e}")))?;
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
    if cells == m.cells {
        return Ok(base.to_vec());
    }
    m.cells = cells;
    Ok(m.encode())
}

/// A material map's cells as a raster (indices as `u16`s), for the paint brush and the strokes' sources.
#[must_use]
pub fn material_raster(m: &MaterialMap) -> Raster {
    let cells: Vec<u16> = m.cells.iter().map(|&c| u16::from(c)).collect();
    Raster::from_samples(m.width, m.width, &cells)
}

/// A raster from a stroke target's bytes as [`crate::files::LevelFiles::edited`] gives them (little-endian `u16`s).
#[must_use]
pub fn raster_of(bytes: &[u8], width: u32, height: u32) -> Raster {
    let samples: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    Raster::from_samples(width, height, &samples)
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
    if (e.size, e.min, e.max) != (b.size, b.min, b.max) {
        out.push("its size or bounds are not the base's".to_owned());
    }
    if e.depth != b.depth {
        out.push("its depth map is not the base's (a sculpt keeps it, TEDIT-HF-3)".to_owned());
    }
    let n = e.size as usize;
    if e.tree != minmax_tree(&e.heights, n) {
        out.push("its min / max tree is not the one its heights give".to_owned());
    }
    if let (Some(g), Some(&side)) = (
        e.envelope.last(),
        bc2_formats::terrain_codec::envelope_sides(n).last(),
    ) {
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
    if e.width != b.width || e.names != b.names {
        out.push("its width or names are not the base's".to_owned());
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

    #[test]
    fn heights_and_materials_written_and_checked() {
        let n = 64usize;
        let h: Vec<u16> = (0..n * n).map(|i| (1000 + (i % n) * 10) as u16).collect();
        let base = MainBlock::new([0.0, 0.0, 0.0, 64.0, 500.0, 64.0], h.clone(), None)
            .unwrap()
            .encode();
        let r = Raster::from_samples(n as u32, n as u32, &h);
        assert_eq!(write_heights(&base, &r).unwrap(), base, "nothing edited: the base's bytes");
        let mut pit = r.clone();
        for z in 20..30 {
            for x in 20..30 {
                pit.set(x, z, 10);
            }
        }
        let out = write_heights(&base, &pit).unwrap();
        assert!(check_heights(&base, &out).is_empty(), "{:?}", check_heights(&base, &out));
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
        assert_eq!(changed_region(&[1, 2, 3, 4], &[1, 2, 9, 4], 2), Some(Region { x0: 0, z0: 1, x1: 1, z1: 2 }));
    }
}
