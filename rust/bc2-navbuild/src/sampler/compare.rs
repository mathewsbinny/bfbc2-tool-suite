//! `navsample_offline.compare`: per column, how far two sample runs on one grid agree - an in-game run (a) and an
//! offline one (b): layer counts, top heights, every surface's partner within 0.1 m, the walkable mask and the
//! blocked flag on matched surfaces, the object surfaces found. Both runs are read a column at a time (an in-game
//! detailed run is up to 84 million columns and 670 MB), so the comparison holds one column of each.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};

use super::Samples;
use crate::navsample::{HEADER_SIZE, Header, Runs};

/// The in-game files' terrain hit word (`TERRAIN_SHAPE_VTABLE`).
const TERRAIN: u32 = super::VTABLE_OF_KIND[0];

/// One surface as the comparison needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rec {
    /// Its height.
    pub y: f32,
    /// Its normal's y (`navsample.load`'s `component / 127` as `float32`).
    pub ny: f32,
    /// Its flags.
    pub flags: u8,
    /// A terrain hit (or a file without hit words).
    pub terrain: bool,
}

/// `byte / 127.0` as a Python float, then `float32` (`navsample.load`).
fn ny_of(b: u8) -> f32 {
    (f64::from(b as i8) / 127.0) as f32
}

/// A run read a column at a time.
pub trait Columns {
    /// Columns in all.
    fn ncols(&self) -> usize;
    /// The next column's surfaces into `out` (cleared first).
    ///
    /// # Errors
    /// The file's read error, or a file cut short.
    fn next_column(&mut self, out: &mut Vec<Rec>) -> std::io::Result<()>;
}

/// A sample file read a column at a time: version 3 streamed from disk (its counts read first), versions 1 and 2
/// (small, old runs) read whole.
pub struct FileColumns {
    /// The header.
    pub header: Header,
    counts: Vec<u8>,
    at: usize,
    v3: Option<BufReader<File>>,
    runs: Option<Runs>,
}

impl FileColumns {
    /// Opens a sample file.
    ///
    /// # Errors
    /// Not a sample file, or unreadable.
    pub fn open(path: &str) -> std::io::Result<Self> {
        let bad = |w: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{path}: {w}"));
        let mut f = File::open(path)?;
        let mut head = [0u8; HEADER_SIZE];
        f.read_exact(&mut head)?;
        let len = f.metadata()?.len();
        let version = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
        if &head[..4] != b"ONS1" {
            return Err(bad("not a navsample file"));
        }
        if version == 3 {
            let mut tail = [0u8; 12];
            f.seek(SeekFrom::Start(len.saturating_sub(12)))?;
            f.read_exact(&mut tail)?;
            if &tail[8..] != b"CNT3" {
                return Err(bad("no column counts"));
            }
            let at = u64::from_le_bytes([tail[0], tail[1], tail[2], tail[3], tail[4], tail[5], tail[6], tail[7]]);
            let (h, _) = super::level::header_of(&head).map_err(|e| bad(&e.to_string()))?;
            let n = usize::try_from(h.nx.max(0) * h.nz.max(0)).map_err(|_| bad("grid too large"))?;
            let mut counts = vec![0u8; n];
            f.seek(SeekFrom::Start(at))?;
            f.read_exact(&mut counts)?;
            f.seek(SeekFrom::Start(HEADER_SIZE as u64))?;
            return Ok(FileColumns {
                header: h,
                counts,
                at: 0,
                v3: Some(BufReader::with_capacity(1 << 20, f)),
                runs: None,
            });
        }
        let mut data = Vec::new();
        f.seek(SeekFrom::Start(0))?;
        f.read_to_end(&mut data)?;
        let (h, runs) = super::level::load_runs(&data).map_err(|e| bad(&e.to_string()))?;
        Ok(FileColumns {
            header: h,
            counts: Vec::new(),
            at: 0,
            v3: None,
            runs: Some(runs),
        })
    }
}

impl FileColumns {
    /// Surfaces per column.
    #[must_use]
    pub fn counts(&self) -> Vec<u8> {
        match &self.runs {
            Some(r) => r.start.windows(2).map(|w| (w[1] - w[0]).clamp(0, 255) as u8).collect(),
            None => self.counts.clone(),
        }
    }
}

impl Columns for FileColumns {
    fn ncols(&self) -> usize {
        match &self.runs {
            Some(r) => r.start.len().saturating_sub(1),
            None => self.counts.len(),
        }
    }

    fn next_column(&mut self, out: &mut Vec<Rec>) -> std::io::Result<()> {
        out.clear();
        let c = self.at;
        self.at += 1;
        if let Some(r) = &self.runs {
            let (a, b) = (r.start[c] as usize, r.start[c + 1] as usize);
            for k in a..b {
                out.push(Rec {
                    y: r.y[k],
                    ny: r.normal[3 * k + 1],
                    flags: r.flags[k],
                    terrain: r.hit.get(3 * k).is_none_or(|&h| h == TERRAIN),
                });
            }
            return Ok(());
        }
        let n = usize::from(self.counts[c]);
        let Some(f) = self.v3.as_mut() else { return Ok(()) };
        let mut rec = [0u8; 20];
        for _ in 0..n {
            f.read_exact(&mut rec)?;
            out.push(Rec {
                y: f32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]),
                ny: ny_of(rec[5]),
                flags: rec[7],
                terrain: u32::from_le_bytes([rec[8], rec[9], rec[10], rec[11]]) == TERRAIN,
            });
        }
        Ok(())
    }
}

/// An offline run's samples read a column at a time.
pub struct SampleColumns<'a> {
    s: &'a Samples,
    at: usize,
    k: usize,
}

impl<'a> SampleColumns<'a> {
    /// Its columns.
    #[must_use]
    pub fn new(s: &'a Samples) -> Self {
        SampleColumns { s, at: 0, k: 0 }
    }
}

impl Columns for SampleColumns<'_> {
    fn ncols(&self) -> usize {
        self.s.counts.len()
    }

    fn next_column(&mut self, out: &mut Vec<Rec>) -> std::io::Result<()> {
        out.clear();
        let n = usize::from(self.s.counts[self.at]);
        self.at += 1;
        for k in self.k..self.k + n {
            out.push(Rec {
                y: self.s.y[k],
                ny: ny_of(self.s.normal[k][1] as u8),
                flags: self.s.flags[k],
                terrain: self.s.kind[k] == super::KIND_TERRAIN,
            });
        }
        self.k += n;
        Ok(())
    }
}

/// `navsample.walkable(s, 40)`: no capsule overlap, no faulted query, the normal's y at least `cos(40 deg) - 0.01`.
fn walkable(r: &Rec) -> bool {
    let min_ny = (40f64.to_radians().cos() - 0.01) as f32;
    r.flags & 3 == 0 && r.ny >= min_ny
}

/// The lowest of `ys`' surfaces within `tol` of `y` (`matched`'s partner).
fn partner(y: f32, ys: &[Rec], tol: f32) -> Option<usize> {
    let yk = f64::from(y);
    (0..ys.len())
        .filter(|&q| (f64::from(ys[q].y) - yk).abs() <= f64::from(tol))
        .min_by(|&p, &q| ys[p].y.total_cmp(&ys[q].y))
}

fn pct(num: u64, den: u64) -> f64 {
    if den == 0 {
        f64::NAN
    } else {
        100.0 * num as f64 / den as f64
    }
}

/// `compare(a, b)`: the measures by the reference's names, in its order (a: in game; the columns a sampled).
///
/// # Errors
/// Not the same number of columns, or a read error.
#[allow(clippy::cast_precision_loss, reason = "counts")]
pub fn compare(a: &mut dyn Columns, b: &mut dyn Columns, tol: f32) -> std::io::Result<Vec<(&'static str, f64)>> {
    if a.ncols() != b.ncols() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not the same grid",
        ));
    }
    let (mut ca, mut cb) = (Vec::new(), Vec::new());
    let mut n = [0u64; 24];
    let mut dy: Vec<f32> = Vec::new();
    const COLS: usize = 0;
    const SA: usize = 1;
    const SB: usize = 2;
    const LAYERS: usize = 3;
    const TOP_IN: usize = 4;
    const MISSING: usize = 5;
    const A_FOUND: usize = 6;
    const B_REAL: usize = 7;
    const MATCHED: usize = 8;
    const WALK_AGREE: usize = 9;
    const WA: usize = 10;
    const WA_FOUND: usize = 11;
    const WB: usize = 12;
    const WB_REAL: usize = 13;
    const BLK_AGREE: usize = 14;
    const BLK_A: usize = 15;
    const BLK_B: usize = 16;
    const BLK_BOTH: usize = 17;
    const OBJ: usize = 18;
    const OBJ_FOUND: usize = 19;
    for _ in 0..a.ncols() {
        a.next_column(&mut ca)?;
        b.next_column(&mut cb)?;
        if ca.is_empty() {
            continue;
        }
        n[COLS] += 1;
        n[SA] += ca.len() as u64;
        n[SB] += cb.len() as u64;
        n[LAYERS] += u64::from(ca.len() == cb.len());
        n[MISSING] += u64::from(cb.is_empty());
        if let (Some(x), Some(y)) = (ca.first(), cb.first()) {
            let d = (x.y - y.y).abs();
            n[TOP_IN] += u64::from(d <= tol);
            dy.push(d);
        }
        for r in &ca {
            let p = partner(r.y, &cb, tol);
            n[A_FOUND] += u64::from(p.is_some());
            let wa = walkable(r);
            if let Some(p) = p {
                let q = &cb[p];
                n[MATCHED] += 1;
                n[WALK_AGREE] += u64::from(wa == walkable(q));
                let (ba, bb) = (r.flags & 1 > 0, q.flags & 1 > 0);
                n[BLK_AGREE] += u64::from(ba == bb);
                n[BLK_A] += u64::from(ba);
                n[BLK_B] += u64::from(bb);
                n[BLK_BOTH] += u64::from(ba && bb);
            }
            if wa {
                n[WA] += 1;
                n[WA_FOUND] += u64::from(p.is_some_and(|p| walkable(&cb[p])));
            }
            if !r.terrain {
                n[OBJ] += 1;
                n[OBJ_FOUND] += u64::from(p.is_some());
            }
        }
        for r in &cb {
            let p = partner(r.y, &ca, tol);
            n[B_REAL] += u64::from(p.is_some());
            if walkable(r) {
                n[WB] += 1;
                n[WB_REAL] += u64::from(p.is_some_and(|p| walkable(&ca[p])));
            }
        }
    }
    dy.sort_by(f32::total_cmp);
    let median = match dy.len() {
        0 => f64::NAN,
        k if k % 2 == 1 => f64::from(dy[k / 2]),
        k => f64::midpoint(f64::from(dy[k / 2 - 1]), f64::from(dy[k / 2])),
    };
    Ok(vec![
        ("columns", n[COLS] as f64),
        ("surfaces_a", n[SA] as f64),
        ("surfaces_b", n[SB] as f64),
        ("layers_equal", pct(n[LAYERS], n[COLS])),
        ("top_within_0.1", pct(n[TOP_IN], dy.len() as u64)),
        ("top_median_mm", median * 1000.0),
        ("top_missing_b", pct(n[MISSING], n[COLS])),
        ("surf_a_found", pct(n[A_FOUND], n[SA])),
        ("surf_b_real", pct(n[B_REAL], n[SB])),
        ("walk_agree_matched", pct(n[WALK_AGREE], n[MATCHED])),
        ("walk_a_found_walk_b", pct(n[WA_FOUND], n[WA])),
        ("walk_b_real_walk_a", pct(n[WB_REAL], n[WB])),
        ("blocked_agree_matched", pct(n[BLK_AGREE], n[MATCHED])),
        ("blocked_a", n[BLK_A] as f64),
        ("blocked_b", n[BLK_B] as f64),
        ("blocked_both", n[BLK_BOTH] as f64),
        ("object_surf_a", n[OBJ] as f64),
        ("object_surf_a_found", pct(n[OBJ_FOUND], n[OBJ])),
    ])
}
