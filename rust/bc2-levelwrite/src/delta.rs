//! Brush-scale edits stored as delta tiles beside the project (round OP1, audit AU1 R24 / D19): a terrain or paint
//! stroke over a 2048 x 2048 raster is a handful of 64 x 64 tiles, each kept once as a small binary file named by its
//! sha1 (`<project>.mapedit.json.tiles\<sha1>.tile`), and the log's `stroke` operation names them - the tiles'
//! places and their sha1s before and after - instead of holding samples inline. The project file stays kilobytes; the
//! tiles are the user's own heights and paint, never game content (section 6, ruling 2).
//!
//! A [`Raster`] keeps its samples as tiles behind `Arc`s, so the editor's checkpoints share every tile a later
//! stroke does not touch (a copy is one `Arc` per tile), and a stroke copies only the tiles it changes. A stroke
//! applies only on the raster it was made on: every tile's sha1 must be its `before` (else nothing is written); its
//! inverse writes the `before` tiles back.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::write::LevelWriteError;

/// A tile's side in samples.
pub const TILE: u32 = 64;

/// A tile file's first bytes (then its width and height, little-endian `u16`s, and its samples, little-endian `u16`s
/// row by row).
pub const TILE_MAGIC: &[u8; 8] = b"BC2TILE1";

/// A tile file's extension.
pub const TILE_EXT: &str = "tile";

/// A raster of samples (a height field's stored values, a paint mask's weights), row by row, kept in tiles of
/// [`TILE`] x [`TILE`] (the last row and column of tiles may be smaller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    width: u32,
    height: u32,
    tiles: Vec<Arc<Vec<u16>>>,
}

impl Raster {
    /// A raster of `width` x `height` samples, all `fill`.
    #[must_use]
    pub fn new(width: u32, height: u32, fill: u16) -> Self {
        let mut r = Self {
            width,
            height,
            tiles: Vec::new(),
        };
        for ty in 0..r.tiles_y() {
            for tx in 0..r.tiles_x() {
                let (_, _, w, h) = r.tile_rect(tx, ty);
                r.tiles.push(Arc::new(vec![fill; (w * h) as usize]));
            }
        }
        r
    }

    /// A raster of `width` x `height` samples, row by row (missing samples are 0).
    #[must_use]
    pub fn from_samples(width: u32, height: u32, samples: &[u16]) -> Self {
        let mut r = Self::new(width, height, 0);
        for ty in 0..r.tiles_y() {
            for tx in 0..r.tiles_x() {
                let (x0, y0, w, h) = r.tile_rect(tx, ty);
                let mut t = Vec::with_capacity((w * h) as usize);
                for y in y0..y0 + h {
                    let start = (y * width + x0) as usize;
                    t.extend_from_slice(samples.get(start..start + w as usize).unwrap_or(&[]));
                    t.resize(((y - y0 + 1) * w) as usize, 0);
                }
                let k = (ty * r.tiles_x() + tx) as usize;
                if let Some(slot) = r.tiles.get_mut(k) {
                    *slot = Arc::new(t);
                }
            }
        }
        r
    }

    /// Samples per row.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Rows.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Tiles per row of tiles.
    #[must_use]
    pub fn tiles_x(&self) -> u32 {
        self.width.div_ceil(TILE)
    }

    /// Rows of tiles.
    #[must_use]
    pub fn tiles_y(&self) -> u32 {
        self.height.div_ceil(TILE)
    }

    /// Tile `(tx, ty)`'s first sample and size: x, y, width, height.
    #[must_use]
    pub fn tile_rect(&self, tx: u32, ty: u32) -> (u32, u32, u32, u32) {
        let (x0, y0) = (tx * TILE, ty * TILE);
        (
            x0,
            y0,
            TILE.min(self.width.saturating_sub(x0)),
            TILE.min(self.height.saturating_sub(y0)),
        )
    }

    fn slot(&self, tx: u32, ty: u32) -> Option<usize> {
        (tx < self.tiles_x() && ty < self.tiles_y()).then(|| (ty * self.tiles_x() + tx) as usize)
    }

    /// The sample at `(x, y)` (0 outside).
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> u16 {
        self.slot(x / TILE, y / TILE)
            .and_then(|k| self.tiles.get(k))
            .and_then(|t| {
                let (_, _, w, _) = self.tile_rect(x / TILE, y / TILE);
                t.get(((y % TILE) * w + x % TILE) as usize).copied()
            })
            .unwrap_or(0)
    }

    /// Sets the sample at `(x, y)` (nothing outside); its tile is copied first when a checkpoint shares it.
    pub fn set(&mut self, x: u32, y: u32, v: u16) {
        let (_, _, w, _) = self.tile_rect(x / TILE, y / TILE);
        if let Some(k) = self.slot(x / TILE, y / TILE)
            && let Some(t) = self.tiles.get_mut(k)
            && let Some(s) = Arc::make_mut(t).get_mut(((y % TILE) * w + x % TILE) as usize)
        {
            *s = v;
        }
    }

    /// Every sample, row by row.
    #[must_use]
    pub fn samples(&self) -> Vec<u16> {
        let mut out = vec![0; (self.width * self.height) as usize];
        for ty in 0..self.tiles_y() {
            for tx in 0..self.tiles_x() {
                let (x0, y0, w, h) = self.tile_rect(tx, ty);
                let Some(t) = self.slot(tx, ty).and_then(|k| self.tiles.get(k)) else {
                    continue;
                };
                for y in 0..h {
                    let at = ((y0 + y) * self.width + x0) as usize;
                    if let (Some(dst), Some(src)) = (
                        out.get_mut(at..at + w as usize),
                        t.get((y * w) as usize..((y + 1) * w) as usize),
                    ) {
                        dst.copy_from_slice(src);
                    }
                }
            }
        }
        out
    }

    /// Tile `(tx, ty)` as its file holds it ([`encode_tile`]).
    #[must_use]
    pub fn tile_file(&self, tx: u32, ty: u32) -> Option<Vec<u8>> {
        let (_, _, w, h) = self.tile_rect(tx, ty);
        let t = self.tiles.get(self.slot(tx, ty)?)?;
        Some(encode_tile(w, h, t))
    }

    /// Whether tile `(tx, ty)` is the very same (shared) tile in `other`: untouched since one was copied from the
    /// other, so its samples are equal without comparing them.
    fn shares(&self, other: &Self, k: usize) -> bool {
        matches!((self.tiles.get(k), other.tiles.get(k)), (Some(a), Some(b)) if Arc::ptr_eq(a, b))
    }

    fn put(&mut self, tx: u32, ty: u32, samples: Vec<u16>) -> bool {
        let (_, _, w, h) = self.tile_rect(tx, ty);
        match self.slot(tx, ty).and_then(|k| self.tiles.get_mut(k)) {
            Some(t) if samples.len() == (w * h) as usize => {
                *t = Arc::new(samples);
                true
            }
            _ => false,
        }
    }
}

/// A tile's file: [`TILE_MAGIC`], its width and height, its samples (little-endian `u16`s).
#[must_use]
pub fn encode_tile(w: u32, h: u32, samples: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + 2 * samples.len());
    out.extend_from_slice(TILE_MAGIC);
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// A tile file's width, height and samples (`None`: not a tile file, or cut short).
#[must_use]
pub fn decode_tile(bytes: &[u8]) -> Option<(u32, u32, Vec<u16>)> {
    let rest = bytes.strip_prefix(TILE_MAGIC.as_slice())?;
    let (wh, data) = rest.split_at_checked(4)?;
    let w = u32::from(u16::from_le_bytes([*wh.first()?, *wh.get(1)?]));
    let h = u32::from(u16::from_le_bytes([*wh.get(2)?, *wh.get(3)?]));
    if data.len() != 2 * (w * h) as usize {
        return None;
    }
    let samples = data.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    Some((w, h, samples))
}

/// One tile a stroke changed: its place in tiles, and its file's sha1 before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileDelta {
    /// The tile's column.
    pub x: u32,
    /// The tile's row.
    pub y: u32,
    /// Its file's sha1 before the stroke.
    pub before: String,
    /// Its file's sha1 after.
    pub after: String,
}

/// Where a project's tiles are kept: put once by sha1, read back checked.
pub trait TileStore {
    /// Keeps a tile file; its sha1 (kept once: a tile already there is not written again).
    ///
    /// # Errors
    /// The file cannot be written.
    fn put(&self, file: &[u8]) -> io::Result<String>;
    /// A tile file by its sha1, when it is there and still has that sha1.
    fn get(&self, sha1: &str) -> Option<Vec<u8>>;
}

/// Tiles in memory (tests, a project not saved anywhere yet).
#[derive(Debug, Default)]
pub struct MemStore(Mutex<HashMap<String, Vec<u8>>>);

impl TileStore for MemStore {
    fn put(&self, file: &[u8]) -> io::Result<String> {
        let sha1 = crate::project::sha1_hex(file);
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(sha1.clone())
            .or_insert_with(|| file.to_vec());
        Ok(sha1)
    }

    fn get(&self, sha1: &str) -> Option<Vec<u8>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).get(sha1).cloned()
    }
}

/// How many tile files a [`DirStore`] keeps in memory (about 32 MB of 64 x 64 tiles) before it lets them all go.
pub const CACHE_TILES: usize = 4096;

/// A project's tile folder: `<project file>.tiles` beside it, the tiles last written or read kept in memory (a
/// stroke's tiles are put and then applied at once: not read back from the disk). Clones share the memory.
#[derive(Debug, Clone)]
pub struct DirStore {
    /// The folder.
    pub dir: PathBuf,
    cache: Arc<Mutex<HashMap<String, Arc<Vec<u8>>>>>,
}

impl DirStore {
    /// The tile folder of the project file at `project`.
    #[must_use]
    pub fn of(project: &Path) -> Self {
        let mut s = project.as_os_str().to_owned();
        s.push(".tiles");
        Self {
            dir: PathBuf::from(s),
            cache: Arc::default(),
        }
    }

    /// The file of the tile with this sha1.
    #[must_use]
    pub fn path(&self, sha1: &str) -> PathBuf {
        self.dir.join(format!("{sha1}.{TILE_EXT}"))
    }

    fn remember(&self, sha1: &str, bytes: Arc<Vec<u8>>) {
        let mut c = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if c.len() >= CACHE_TILES {
            c.clear();
        }
        c.insert(sha1.to_owned(), bytes);
    }

    fn cached(&self, sha1: &str) -> Option<Arc<Vec<u8>>> {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(sha1)
            .cloned()
    }

    fn read(&self, sha1: &str) -> Option<Vec<u8>> {
        let bytes = std::fs::read(self.path(sha1)).ok()?;
        (crate::project::sha1_hex(&bytes) == sha1).then_some(bytes)
    }
}

impl TileStore for DirStore {
    fn put(&self, file: &[u8]) -> io::Result<String> {
        let sha1 = crate::project::sha1_hex(file);
        if self.cached(&sha1).is_some() {
            return Ok(sha1);
        }
        if self.read(&sha1).is_none() {
            // not synced one by one (a stroke writes several): a tile a power cut left short fails its sha1 when
            // read, and the stroke that names it is refused in words, never applied wrong
            put_unsynced(&self.path(&sha1), file)?;
        }
        self.remember(&sha1, Arc::new(file.to_vec()));
        Ok(sha1)
    }

    fn get(&self, sha1: &str) -> Option<Vec<u8>> {
        if let Some(b) = self.cached(sha1) {
            return Some(b.to_vec());
        }
        let bytes = self.read(sha1)?;
        self.remember(sha1, Arc::new(bytes.clone()));
        Some(bytes)
    }
}

/// A tile file written in place (not synced: [`DirStore::put`]); a file cut short by a crash fails its sha1 when read
/// and is written again by the next put of the tile.
fn put_unsynced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes)
}

/// The tiles that differ between `before` and `after` (rasters of one size), their files put into `store`: a
/// stroke's deltas, in row order. A tile `after` still shares with `before` is not compared.
///
/// # Errors
/// The rasters' sizes differ, or a tile file cannot be written.
pub fn stroke(before: &Raster, after: &Raster, store: &dyn TileStore) -> Result<Vec<TileDelta>, LevelWriteError> {
    if (before.width, before.height) != (after.width, after.height) {
        return Err(LevelWriteError::Refused(format!(
            "a stroke over {} x {} samples on a raster of {} x {}",
            after.width, after.height, before.width, before.height
        )));
    }
    let mut out = Vec::new();
    for ty in 0..before.tiles_y() {
        for tx in 0..before.tiles_x() {
            let k = (ty * before.tiles_x() + tx) as usize;
            if after.shares(before, k) || before.tiles.get(k) == after.tiles.get(k) {
                continue;
            }
            let (Some(b), Some(a)) = (before.tile_file(tx, ty), after.tile_file(tx, ty)) else {
                continue;
            };
            let io = |e: io::Error| LevelWriteError::Refused(format!("a tile was not kept: {e}"));
            out.push(TileDelta {
                x: tx,
                y: ty,
                before: store.put(&b).map_err(io)?,
                after: store.put(&a).map_err(io)?,
            });
        }
    }
    Ok(out)
}

/// A stroke applied (`forward`: its `after` tiles where its `before` ones are) or undone (its `before` tiles where its
/// `after` ones are): every tile is checked first, and nothing is written unless all are as expected.
///
/// # Errors
/// A tile is not the one the stroke was made on (or, undoing, made), outside the raster, or its file is missing or
/// damaged in the store.
pub fn apply_stroke(
    r: &mut Raster,
    tiles: &[TileDelta],
    tile: &dyn Fn(&str) -> Option<Vec<u8>>,
    forward: bool,
) -> Result<(), LevelWriteError> {
    let mut writes = Vec::with_capacity(tiles.len());
    for d in tiles {
        let (expect, put) = if forward {
            (&d.before, &d.after)
        } else {
            (&d.after, &d.before)
        };
        let now = r
            .tile_file(d.x, d.y)
            .ok_or_else(|| LevelWriteError::Refused(format!("tile ({}, {}) is outside the raster", d.x, d.y)))?;
        if crate::project::sha1_hex(&now) != *expect {
            return Err(LevelWriteError::Refused(format!(
                "tile ({}, {}) is not the one the stroke was {} on",
                d.x,
                d.y,
                if forward { "made" } else { "applied" }
            )));
        }
        let (w, h, samples) = tile(put)
            .and_then(|b| decode_tile(&b))
            .ok_or_else(|| LevelWriteError::NoFile(format!("the stroke's tile {put}")))?;
        if (w, h) != {
            let (_, _, tw, th) = r.tile_rect(d.x, d.y);
            (tw, th)
        } {
            return Err(LevelWriteError::Refused(format!(
                "tile {put} is {w} x {h} samples, not its place's"
            )));
        }
        writes.push((d.x, d.y, samples));
    }
    for (x, y, s) in writes {
        r.put(x, y, s);
    }
    Ok(())
}

#[cfg(test)]
impl Raster {
    fn samples_of(&self, tx: u32, ty: u32) -> Vec<u16> {
        self.slot(tx, ty)
            .and_then(|k| self.tiles.get(k))
            .map_or_else(Vec::new, |t| t.to_vec())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn rasters_tiles_and_strokes() {
        // 130 x 70: tiles of 64, the last column 2 wide, the last row 6 high
        let samples: Vec<u16> = (0..130 * 70).map(|i| (i % 65_521) as u16).collect();
        let r = Raster::from_samples(130, 70, &samples);
        assert_eq!((r.tiles_x(), r.tiles_y()), (3, 2));
        assert_eq!(r.tile_rect(2, 1), (128, 64, 2, 6));
        assert_eq!(r.samples(), samples);
        assert_eq!(r.get(129, 69), samples[69 * 130 + 129]);
        let f = r.tile_file(2, 1).unwrap();
        assert_eq!(decode_tile(&f).unwrap(), (2, 6, r.samples_of(2, 1)));
        assert!(decode_tile(&f[..f.len() - 1]).is_none(), "cut short");
        assert!(decode_tile(b"BC2TILE0xxxx").is_none());

        // a stroke: a copy shares every tile; two samples set touch two tiles, only those are stored
        let store = MemStore::default();
        let mut after = r.clone();
        after.set(1, 1, 7);
        after.set(129, 69, 9);
        let d = stroke(&r, &after, &store).unwrap();
        assert_eq!(d.iter().map(|t| (t.x, t.y)).collect::<Vec<_>>(), [(0, 0), (2, 1)]);
        assert_eq!(store.0.lock().unwrap().len(), 4);
        let mut x = r.clone();
        apply_stroke(&mut x, &d, &|s| store.get(s), true).unwrap();
        assert_eq!(x, after);
        // applied twice: refused, nothing written
        assert!(apply_stroke(&mut x, &d, &|s| store.get(s), true).is_err());
        assert_eq!(x, after);
        // undone: the raster as it was
        apply_stroke(&mut x, &d, &|s| store.get(s), false).unwrap();
        assert_eq!(x.samples(), samples);
        // a missing tile file: refused, nothing written
        let empty = MemStore::default();
        let e = apply_stroke(&mut x, &d, &|s| empty.get(s), true).unwrap_err();
        assert!(e.to_string().contains("is not among the level's files"), "{e}");
        assert_eq!(x.samples(), samples);
    }

    #[test]
    fn tiles_in_a_folder() {
        let dir = std::env::temp_dir().join(format!("bc2_levelwrite_tiles_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = DirStore::of(&dir.join("Map.mapedit.json"));
        assert!(store.dir.ends_with("Map.mapedit.json.tiles"));
        let t = encode_tile(2, 1, &[1, 2]);
        let sha1 = store.put(&t).unwrap();
        assert_eq!(store.get(&sha1).unwrap(), t);
        assert_eq!(store.put(&t).unwrap(), sha1, "kept once");
        // a damaged file is not read as the tile (by a store that does not hold it in memory), and is written again
        std::fs::write(store.path(&sha1), b"damaged").unwrap();
        let fresh = DirStore::of(&dir.join("Map.mapedit.json"));
        assert!(fresh.get(&sha1).is_none());
        assert_eq!(fresh.put(&t).unwrap(), sha1, "written again");
        assert_eq!(DirStore::of(&dir.join("Map.mapedit.json")).get(&sha1).unwrap(), t);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
