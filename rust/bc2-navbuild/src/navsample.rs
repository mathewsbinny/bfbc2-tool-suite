//! The column records of a NavKit sample run of format version 1 or 2 (`navsample.load`'s loop; OPFOR's `[dev]
//! navsample` writes them, `src/dll/navsample.cpp`): per column a count byte, then that many records of
//! `<f 3b B` (height, normal x / y / z in 1/127, flags) and, in version 2, `<3I` (the hit's shape words).
//! Version 3's columns are read by NumPy in one pass and are not here.

use crate::NavError;

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
}
