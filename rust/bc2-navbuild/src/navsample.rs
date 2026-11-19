//! The column records of a NavKit sample run of format version 1 or 2 (`navsample.load`'s loop; OPFOR's `[dev]
//! navsample` writes them, `src/dll/navsample.cpp`): per column a count byte, then that many records of
//! `<f 3b B` (height, normal x / y / z in 1/127, flags) and, in version 2, `<3I` (the hit's shape words).
//!
//! Round F3: version 3 too ([`parse_v3`], `navsample.load`'s NumPy pass: every record `<f 3b B 3I`, then a count
//! byte per column, then `{u64 offset of the counts, "CNT3"}`), and its writer ([`write_v3`],
//! `navsample_offline.write`'s bytes). Where the reference raises this answers an error and the Python runs.

use crate::NavError;

/// The header's size (`navsample.HEADER`, `<4sI3f2i6fI`).
pub const HEADER_SIZE: usize = 56;

/// Version 3's records (`navsample.load`): `columns` count bytes at the offset the trailer names, the records after
/// the header.
///
/// # Errors
/// [`NavError::Shape`] where the reference raises (no trailer, counts or records past the end) or for a negative
/// column count (NumPy would read every remaining byte as counts).
#[allow(clippy::many_single_char_names)] // the record's fields
pub fn parse_v3(data: &[u8], nx: i64, nz: i64) -> Result<Runs, NavError> {
    let v = V3::new(data, nx, nz)?;
    let mut start = vec![0u8; 8 * (v.counts.len() + 1)];
    v.fill_start(&mut start);
    let mut out = Runs {
        start: start
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| i64::from_le_bytes(*c))
            .collect(),
        read: v.counts.len(),
        ..Runs::default()
    };
    for r in v.records {
        let [y0, y1, y2, y3, a, b, d, f, h @ ..] = *r;
        out.y.push(f32::from_le_bytes([y0, y1, y2, y3]));
        for x in [a, b, d] {
            out.normal.push(normal(x));
        }
        out.flags.push(f);
        for w in h.as_chunks::<4>().0 {
            out.hit.push(u32::from_le_bytes(*w));
        }
    }
    Ok(out)
}

/// A stored normal component as `navsample.load` gives it: `(component / 127.0)` rounded to `float32`.
fn normal(x: u8) -> f32 {
    (f64::from(x as i8) / 127.0) as f32
}

/// A version 3 file's counts and records, checked as [`parse_v3`] checks them, for filling the arrays' bytes in
/// place (the bindings fill Python's buffers: no copy of a 600 MB run held twice).
#[derive(Debug, Clone, Copy)]
pub struct V3<'a> {
    /// One count byte per column.
    pub counts: &'a [u8],
    /// The 20-byte records.
    pub records: &'a [[u8; 20]],
}

impl<'a> V3<'a> {
    /// The counts and records of `data`.
    ///
    /// # Errors
    /// As [`parse_v3`].
    pub fn new(data: &'a [u8], nx: i64, nz: i64) -> Result<Self, NavError> {
        let (counts, n) = v3_counts(data, nx, nz)?;
        let recs = data
            .get(HEADER_SIZE..HEADER_SIZE + 20 * n)
            .ok_or_else(|| NavError::Shape("navsample version 3: records past the end".into()))?;
        Ok(Self {
            counts,
            records: recs.as_chunks::<20>().0,
        })
    }

    /// `start` (int64, columns + 1) as little-endian bytes into `out`.
    pub fn fill_start(&self, out: &mut [u8]) {
        let mut total = 0i64;
        let mut it = out.as_chunks_mut::<8>().0.iter_mut();
        if let Some(w) = it.next() {
            *w = 0i64.to_le_bytes();
        }
        for (w, &c) in it.zip(self.counts) {
            total += i64::from(c);
            *w = total.to_le_bytes();
        }
    }

    /// `y` (float32) into `out`.
    pub fn fill_y(&self, out: &mut [u8]) {
        for (w, r) in out.as_chunks_mut::<4>().0.iter_mut().zip(self.records) {
            let [y0, y1, y2, y3, ..] = *r;
            *w = [y0, y1, y2, y3];
        }
    }

    /// `normal` (float32, n x 3) into `out`.
    pub fn fill_normal(&self, out: &mut [u8]) {
        for (w, r) in out.as_chunks_mut::<12>().0.iter_mut().zip(self.records) {
            let [_, _, _, _, a, b, d, ..] = *r;
            let mut v = [0u8; 12];
            for (dst, x) in v.as_chunks_mut::<4>().0.iter_mut().zip([a, b, d]) {
                *dst = normal(x).to_le_bytes();
            }
            *w = v;
        }
    }

    /// `flags` (uint8) into `out`.
    pub fn fill_flags(&self, out: &mut [u8]) {
        for (w, r) in out.iter_mut().zip(self.records) {
            let [_, _, _, _, _, _, _, f, ..] = *r;
            *w = f;
        }
    }

    /// `hit` (uint32, n x 3) into `out`.
    pub fn fill_hit(&self, out: &mut [u8]) {
        for (w, r) in out.as_chunks_mut::<12>().0.iter_mut().zip(self.records) {
            let [_, _, _, _, _, _, _, _, h @ ..] = *r;
            *w = h;
        }
    }
}

/// The count bytes and the total of surfaces.
fn v3_counts(data: &[u8], nx: i64, nz: i64) -> Result<(&[u8], usize), NavError> {
    let bad = |what: &str| NavError::Shape(format!("navsample version 3: {what}"));
    if nx < 0 || nz < 0 {
        return Err(bad("a negative grid size"));
    }
    let columns =
        usize::try_from(nx.checked_mul(nz).ok_or_else(|| bad("grid too large"))?).map_err(|_| bad("grid too large"))?;
    if data.len() < HEADER_SIZE + 12 || !data.ends_with(b"CNT3") {
        return Err(bad("no column counts"));
    }
    let tail = data
        .get(data.len() - 12..data.len() - 4)
        .ok_or_else(|| bad("no trailer"))?;
    let at = usize::try_from(u64::from_le_bytes(tail.try_into().map_err(|_| bad("no trailer"))?))
        .map_err(|_| bad("counts past the end"))?;
    let counts = data
        .get(at..at.checked_add(columns).ok_or_else(|| bad("counts past the end"))?)
        .ok_or_else(|| bad("counts past the end"))?;
    let n = counts.iter().map(|&c| usize::from(c)).sum();
    Ok((counts, n))
}

/// The header's values (`navsample_offline.write`'s `HEADER.pack` arguments after the magic and version).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Header {
    /// Grid pitch.
    pub pitch: f64,
    /// Grid origin x.
    pub min_x: f64,
    /// Grid origin z.
    pub min_z: f64,
    /// Columns along x.
    pub nx: i64,
    /// Columns along z.
    pub nz: i64,
    /// The cast's top.
    pub y_top: f64,
    /// The cast's bottom.
    pub y_bottom: f64,
    /// The capsule's radius.
    pub radius: f64,
    /// The capsule's bottom.
    pub cap_bottom: f64,
    /// The capsule's top.
    pub cap_top: f64,
    /// The cast's ray radius.
    pub ray_radius: f64,
}

/// The arrays [`write_v3`] writes, as their little-endian bytes: `start` int64 (columns + 1), `y` float32, `normal`
/// float32 (n x 3), `flags` uint8, `hit` uint32 (n x 3; `None`: zeros).
#[derive(Debug, Clone, Copy)]
pub struct Columns<'a> {
    /// `start`.
    pub start: &'a [u8],
    /// `y`.
    pub y: &'a [u8],
    /// `normal`.
    pub normal: &'a [u8],
    /// `flags`.
    pub flags: &'a [u8],
    /// `hit`.
    pub hit: Option<&'a [u8]>,
}

/// `struct.pack('<f', v)`: `None` where it raises (a finite value past `f32`), or for a NaN (the interpreter's to
/// convert).
fn pack_f32(v: f64) -> Option<[u8; 4]> {
    let f = v as f32;
    if v.is_nan() || (f.is_infinite() && v.is_finite()) {
        return None;
    }
    Some(f.to_le_bytes())
}

/// `navsample_offline.write`'s file bytes (version 3): the header, a record per surface (`y`, the normal times 127
/// rounded half to even and clipped to +-127, the flags, the hit words or zeros), the count bytes per column (`start`'s
/// differences, wrapped to a byte as NumPy's cast does), the counts' offset and `CNT3`.
///
/// # Errors
/// [`NavError::Shape`] where the reference raises (a column of more than 255 surfaces, a header value `struct`
/// refuses, arrays of other lengths) or a normal is a NaN (NumPy's cast of a NaN to `int8` is the platform's).
#[allow(clippy::many_single_char_names)] // the reference's names
pub fn write_v3(h: &Header, a: &Columns<'_>) -> Result<Vec<u8>, NavError> {
    let bad = |what: &str| NavError::Shape(format!("navsample write: {what}"));
    let n = a.y.len() / 4;
    let whole = a.start.len().is_multiple_of(8) && a.y.len().is_multiple_of(4);
    if !whole || a.normal.len() != 12 * n || a.flags.len() != n || a.hit.is_some_and(|h| h.len() != 12 * n) {
        return Err(bad("arrays of other lengths"));
    }
    let start: Vec<i64> = a
        .start
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| i64::from_le_bytes(*c))
        .collect();
    let counts: Vec<i64> = start
        .windows(2)
        .map(|w| match w {
            [a, b] => b.wrapping_sub(*a),
            _ => 0,
        })
        .collect();
    if counts.iter().any(|&c| c > 255) {
        return Err(bad("more than 255 surfaces in a column"));
    }
    let i32_of = |v: i64| i32::try_from(v).map_err(|_| bad("a grid size past 32 bits"));
    let f = |v: f64| pack_f32(v).ok_or_else(|| bad("a header value past float32"));
    let mut out = Vec::with_capacity(HEADER_SIZE + 20 * n + counts.len() + 12);
    out.extend_from_slice(b"ONS1");
    out.extend_from_slice(&3u32.to_le_bytes());
    for v in [h.pitch, h.min_x, h.min_z] {
        out.extend_from_slice(&f(v)?);
    }
    out.extend_from_slice(&i32_of(h.nx)?.to_le_bytes());
    out.extend_from_slice(&i32_of(h.nz)?.to_le_bytes());
    for v in [h.y_top, h.y_bottom, h.radius, h.cap_bottom, h.cap_top, h.ray_radius] {
        out.extend_from_slice(&f(v)?);
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    let rows = a.hit.map(|h| h.as_chunks::<12>().0);
    let records =
        a.y.as_chunks::<4>()
            .0
            .iter()
            .zip(a.normal.as_chunks::<12>().0)
            .zip(a.flags);
    for (i, ((yv, nv), fl)) in records.enumerate() {
        out.extend_from_slice(yv);
        for c in nv.as_chunks::<4>().0 {
            let s = f32::from_le_bytes(*c) * 127.0;
            if s.is_nan() {
                return Err(bad("a NaN normal"));
            }
            out.push(s.round_ties_even().clamp(-127.0, 127.0) as i8 as u8);
        }
        out.push(*fl);
        match rows.and_then(|r| r.get(i)) {
            Some(row) => out.extend_from_slice(row),
            None => out.extend_from_slice(&[0; 12]),
        }
    }
    let at = out.len() as u64;
    out.extend(counts.iter().map(|&c| c as u8));
    out.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(b"CNT3");
    Ok(out)
}

/// A run's surfaces as `navsample.load` holds them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Runs {
    /// Per column its first surface; `columns + 1` values (the columns after the last read hold the end).
    pub start: Vec<i64>,
    /// Heights.
    pub y: Vec<f32>,
    /// Normals, three per surface: `(component / 127.0)` rounded to `float32`.
    pub normal: Vec<f32>,
    /// Flags.
    pub flags: Vec<u8>,
    /// Version 2: three words per surface; version 1: none.
    pub hit: Vec<u32>,
    /// The columns read (fewer than asked when the data ends first: an interrupted run).
    pub read: usize,
}

/// The records from `offset` on, for `columns` columns, as `navsample.load` reads them; an error where its
/// `struct.unpack_from` would fail (a record cut short).
///
/// # Errors
/// [`NavError::Shape`] for a record past the end of the data.
#[allow(clippy::many_single_char_names)] // the reference's names
pub fn parse_runs(data: &[u8], offset: usize, columns: usize, version: u32) -> Result<Runs, NavError> {
    let rec = if version == 2 { 20 } else { 8 };
    let short = || NavError::Shape("a sample record past the end of the data".into());
    let mut out = Runs {
        start: vec![0; columns + 1],
        ..Runs::default()
    };
    let mut pos = offset;
    let mut c = 0usize;
    while c < columns && pos < data.len() {
        let n = *data.get(pos).ok_or_else(short)?;
        pos += 1;
        for _ in 0..n {
            let r = data.get(pos..pos + 8).ok_or_else(short)?;
            let (yb, rest) = r.split_at(4);
            let y = f32::from_le_bytes(yb.try_into().map_err(|_| short())?);
            if let [a, b, d, f] = *rest {
                out.y.push(y);
                for v in [a, b, d] {
                    // a / 127.0 as a Python float, then np.array(..., float32)
                    out.normal.push((f64::from(v as i8) / 127.0) as f32);
                }
                out.flags.push(f);
            }
            if version == 2 {
                let h = data.get(pos + 8..pos + 20).ok_or_else(short)?;
                for w in h.as_chunks::<4>().0 {
                    out.hit.push(u32::from_le_bytes(*w));
                }
            }
            pos += rec;
        }
        c += 1;
        if let Some(s) = out.start.get_mut(c) {
            *s = out.y.len() as i64;
        }
    }
    let end = out.y.len() as i64;
    for s in out.start.iter_mut().skip(c) {
        *s = end;
    }
    out.read = c;
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn records_and_an_interrupted_run() {
        let mut d = vec![0u8; 4];
        d.push(2);
        for (y, n) in [(1.5f32, [0i8, 127, -127]), (-2.0, [5, 100, 3])] {
            d.extend(y.to_le_bytes());
            d.extend(n.iter().map(|&v| v as u8));
            d.push(3);
            d.extend([1u8; 12]);
        }
        d.push(0);
        let r = parse_runs(&d, 4, 4, 2).unwrap();
        assert_eq!(r.start, vec![0, 2, 2, 2, 2]);
        assert_eq!(r.read, 2);
        assert_eq!(r.y, vec![1.5, -2.0]);
        assert_eq!(r.normal[1], 1.0);
        assert_eq!(r.normal[2], -1.0);
        assert_eq!(r.normal[3], (5.0f64 / 127.0) as f32);
        assert_eq!(r.hit.len(), 6);
        assert!(parse_runs(&d[..d.len() - 3], 4, 4, 2).is_err());
        // version 1: 8-byte records, no hits
        let v1 = [1u8, 0, 0, 128, 63, 0, 127, 0, 2];
        let r = parse_runs(&v1, 0, 1, 1).unwrap();
        assert_eq!(
            (r.y.clone(), r.flags.clone(), r.hit.len(), r.read),
            (vec![1.0], vec![2], 0, 1)
        );
    }

    #[test]
    fn version_3_written_and_read() {
        let h = Header {
            pitch: 0.8,
            min_x: -10.0,
            min_z: 4.0,
            nx: 2,
            nz: 1,
            y_top: 100.0,
            y_bottom: -5.0,
            radius: 0.3,
            cap_bottom: 0.28,
            cap_top: 1.8,
            ray_radius: 0.05,
        };
        let le32 = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let start: Vec<u8> = [0i64, 2, 2].iter().flat_map(|x| x.to_le_bytes()).collect();
        let hit: Vec<u8> = (1u32..=6).flat_map(u32::to_le_bytes).collect();
        let (y, normal) = (le32(&[1.5, -3.0]), le32(&[0.0, 1.0, -1.0, 0.5, 0.25, 2.0]));
        let cols = Columns {
            start: &start,
            y: &y,
            normal: &normal,
            flags: &[1, 2],
            hit: Some(&hit),
        };
        let b = write_v3(&h, &cols).unwrap();
        assert_eq!(&b[b.len() - 4..], b"CNT3");
        assert_eq!(b.len(), HEADER_SIZE + 40 + 2 + 12);
        let r = parse_v3(&b, 2, 1).unwrap();
        assert_eq!(r.start, vec![0, 2, 2]);
        assert_eq!(r.y, vec![1.5, -3.0]);
        assert_eq!(r.hit, vec![1, 2, 3, 4, 5, 6]);
        // 0.5 * 127 = 63.5 rounds to 64 (half to even), 2.0 * 127 clips to 127
        assert_eq!(r.normal[3], (64.0f64 / 127.0) as f32);
        assert_eq!(r.normal[5], 1.0);
        let big: Vec<u8> = [0i64, 300].iter().flat_map(|x| x.to_le_bytes()).collect();
        let none = Columns {
            start: &big,
            y: &[],
            normal: &[],
            flags: &[],
            hit: None,
        };
        assert!(write_v3(&h, &none).is_err());
        assert!(parse_v3(&b[..b.len() - 1], 2, 1).is_err());
        assert!(parse_v3(&b, -1, 1).is_err());
    }
}
