//! `bfbc2 detailed <level> --offsets X,Z [--samples <bin>] [--score]`: NavKit's `tools\detailed.py`. Round O4 answered
//! the usage and what fails before the choice; round O5 makes the choice itself (`choose`: the sectors near a
//! `NeedsDetailedPathfinding` object's collision box, through the suite's Havok index and triangle reader; the stacked
//! floors of the 0.8 m samples, through NavKit's sample reader). The run prints one line at its end, so anything the
//! port does not read as the Python does hands the whole run to the Python: `--score` (DICE's choice, evaluation
//! only), objects of another shape, a packfile the reader refuses, a sample file it refuses.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::verbose_bit_mask,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::format_push_string,
    clippy::type_complexity,
    reason = "a port of Python: indexes after their lengths are checked, ints as Python turns them into floats, == as Python compares floats, the Python's names and its functions' shape kept"
)]

use std::collections::{BTreeMap, BTreeSet};

use super::pycommon::{find_opfor_text, py_int};
use crate::pyfmt::{docstring, str_repr};
use crate::pyjson::{self, Value};
use crate::pyos::{self, join};
use crate::pyproc::{Fail, Py, PyErr, PyResult};

const SOURCE: &str = include_str!("../../../../bfbc2-auto-navkit/tools/detailed.py");

pub(crate) const MARGIN: f64 = 5.0;
pub(crate) const STACKED_MIN: u64 = 100;
const STACK_GAP: f64 = 2.0;
const SLOPE_MAX: f64 = 40.0;

/// A JSON value's truth as Python's `bool()` gives it.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::List(l) => !l.is_empty(),
        Value::Dict(d) => !d.is_empty(),
    }
}

/// A matrix entry as NumPy's array of Python numbers holds it (ints and floats only; anything else hands off).
fn num(v: &Value) -> PyResult<f64> {
    match v {
        Value::Int(i) if i.unsigned_abs() < (1 << 53) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        _ => Err(Fail::Python),
    }
}

/// `int(math.floor(x / 100))`.
fn sector_of(x: f64) -> PyResult<i64> {
    let s = (x / 100.0).floor();
    if !s.is_finite() || s.abs() > 9e15 {
        return Err(Fail::Python);
    }
    Ok(s as i64)
}

/// One flagged object: its blueprint (lower case), the rotation rows `m[0:3]`, `m[4:7]`, `m[8:11]`, `m[12:15]`.
type Flagged = (String, [[f64; 3]; 3], [f64; 3]);

/// The flagged objects of `objects.json`, in its order.
fn flagged_objects(objects: &[Value]) -> PyResult<Vec<Flagged>> {
    let mut flagged = Vec::new();
    for o in objects {
        let Value::Dict(_) = o else {
            return Err(Fail::Python);
        };
        let flag = match o.get("flags") {
            Some(f) if truthy(f) => match f {
                Value::List(l) => truthy(&l[0]),
                _ => return Err(Fail::Python), // a string's or dict's [0]: the Python's own answer
            },
            _ => false,
        };
        if !flag {
            continue;
        }
        let Some(Value::Str(bp)) = o.get("bp") else {
            return Err(Fail::Python);
        };
        if !bp.is_ascii() {
            return Err(Fail::Python); // str.lower beyond ASCII: the Python's
        }
        let Some(Value::List(m)) = o.get("m") else {
            return Err(Fail::Python);
        };
        if m.len() < 15 {
            return Err(Fail::Python);
        }
        let mut r = [[0.0; 3]; 3];
        for (row, base) in r.iter_mut().zip([0usize, 4, 8]) {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = num(&m[base + j])?;
            }
        }
        let t = [num(&m[12])?, num(&m[13])?, num(&m[14])?];
        flagged.push((bp.to_ascii_lowercase(), r, t));
    }
    Ok(flagged)
}

/// `flagged_sectors(level_dir, common, xoff, zoff)`.
pub(crate) fn flagged_sectors(
    level_dir: &str,
    common: &[String],
    xoff: i64,
    zoff: i64,
) -> PyResult<BTreeSet<(i64, i64)>> {
    let mut bases: Vec<String> = vec![join(level_dir, &["pkg"])?, join(level_dir, &["dist"])?];
    bases.extend(common.iter().cloned());
    bases.retain(|b| pyos::isdir(b));
    let hv = bc2_scene::collision::havok_index(&bases);
    let data = std::fs::read(join(level_dir, &["objects.json"])?).map_err(|_| Fail::Python)?;
    let Some(Value::List(objects)) = pyjson::loads_ansi(&data) else {
        return Err(Fail::Python);
    };
    let flagged = flagged_objects(&objects)?;
    // blueprint_triangles: each packfile read once; one the reader refuses raises in the Python
    let mut cache: BTreeMap<String, Option<Vec<[[f32; 3]; 3]>>> = BTreeMap::new();
    for (bp, _, _) in &flagged {
        if cache.contains_key(bp) {
            continue;
        }
        let tris = match hv.get(bp) {
            Some(p) => {
                let data = std::fs::read(p).map_err(|_| Fail::Python)?;
                match bc2_formats::havok::triangles(&data) {
                    Ok(t) => t.map(|v| v.into_iter().collect::<Vec<_>>()),
                    Err(_) => return Err(Fail::Python),
                }
            }
            None => None,
        };
        cache.insert(bp.clone(), tris);
    }
    let mut out = BTreeSet::new();
    for (bp, r, t) in &flagged {
        let (x0, z0, x1, z1) = match cache.get(bp).and_then(Option::as_ref) {
            None => (t[0], t[2], t[0], t[2]),
            Some(tris) if tris.is_empty() => return Err(Fail::Python), // .min() of nothing: ValueError
            Some(tris) => {
                // tris.reshape(-1, 3) @ R + t in float64; the x and z columns' extremes
                let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
                for p in tris.iter().flatten() {
                    let (u, v, w) = (f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
                    let x = u * r[0][0] + v * r[1][0] + w * r[2][0] + t[0];
                    let z = u * r[0][2] + v * r[1][2] + w * r[2][2] + t[2];
                    if x.is_nan() || z.is_nan() {
                        return Err(Fail::Python); // NumPy's min / max of a NaN
                    }
                    b = (b.0.min(x), b.1.min(z), b.2.max(x), b.3.max(z));
                }
                b
            }
        };
        let (xa, xb) = (sector_of(x0 - MARGIN)? - xoff, sector_of(x1 + MARGIN)? - xoff);
        let (za, zb) = (sector_of(z0 - MARGIN)? - zoff, sector_of(z1 + MARGIN)? - zoff);
        if (xb - xa).saturating_mul(zb - za) > 1_000_000 {
            return Err(Fail::Python);
        }
        for x in xa..=xb {
            for z in za..=zb {
                out.insert((x, z));
            }
        }
    }
    Ok(out)
}

/// `stacked_columns(samples_path, xoff, zoff)`: {sector: columns with standable surfaces `STACK_GAP` apart}.
pub(crate) fn stacked_columns(path: &str, xoff: i64, zoff: i64) -> PyResult<BTreeMap<(i64, i64), u64>> {
    use bc2_navbuild::navsample;
    let data = std::fs::read(path).map_err(|_| Fail::Python)?;
    // HEADER '<4sI3f2i6fI'
    if data.len() < 56 || &data[0..4] != b"ONS1" {
        return Err(Fail::Python);
    }
    let u32_at = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let f32_at = |o: usize| f64::from(f32::from_bits(u32_at(o)));
    let version = u32_at(4);
    let (pitch, min_x, min_z) = (f32_at(8), f32_at(12), f32_at(16));
    let (nx, nz) = (i64::from(u32_at(20) as i32), i64::from(u32_at(24) as i32));
    if nx <= 0 || nz <= 0 || nx.saturating_mul(nz) > 1 << 32 {
        return Err(Fail::Python);
    }
    let runs = match version {
        3 => navsample::parse_v3(&data, nx, nz).map_err(|_| Fail::Python)?,
        1 | 2 => navsample::parse_runs(&data, 56, (nx * nz) as usize, version).map_err(|_| Fail::Python)?,
        _ => return Err(Fail::Python),
    };
    let cols = (nx * nz) as usize;
    if runs.start.len() != cols + 1 || runs.normal.len() != 3 * runs.y.len() || runs.flags.len() != runs.y.len() {
        return Err(Fail::Python);
    }
    // NumPy 2: the float32 normals against a Python float, compared in float32
    let limit = (SLOPE_MAX.to_radians().cos() - 0.01) as f32;
    let mut out: BTreeMap<(i64, i64), u64> = BTreeMap::new();
    for c in 0..cols {
        let (a, b) = (runs.start[c], runs.start[c + 1]);
        if a < 0 || b < a || b as usize > runs.y.len() {
            return Err(Fail::Python);
        }
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for k in a as usize..b as usize {
            if runs.flags[k] & 3 == 0 && runs.normal[3 * k + 1] >= limit {
                let y = f64::from(runs.y[k]);
                // np.minimum.at / np.maximum.at: a NaN height stays
                lo = if y.is_nan() || lo.is_nan() { f64::NAN } else { lo.min(y) };
                hi = if y.is_nan() || hi.is_nan() { f64::NAN } else { hi.max(y) };
            }
        }
        if hi - lo >= STACK_GAP {
            let ci = c as i64;
            let x = min_x + (ci % nx) as f64 * pitch;
            let z = min_z + (ci / nx) as f64 * pitch;
            *out.entry((sector_of(x)? - xoff, sector_of(z)? - zoff)).or_insert(0) += 1;
        }
    }
    Ok(out)
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.is_empty() {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    // opts = dict(zip(argv[2:], argv[3:])): the last pair of a key wins
    let rest = &args[1..];
    let mut offsets: Option<&str> = None;
    let mut samples: Option<&str> = None;
    for w in rest.windows(2) {
        if w[0] == "--offsets" {
            offsets = Some(&w[1]);
        }
        if w[0] == "--samples" {
            samples = Some(&w[1]);
        }
    }
    // opfor_root(): OPFOR_ROOT as it is, else paths.find_opfor (GameCopyNotFound uncaught)
    let root = match std::env::var("OPFOR_ROOT").ok().filter(|v| !v.is_empty()) {
        Some(r) => r,
        None => match find_opfor_text(None, true)? {
            Ok(r) => r,
            Err(m) => return Err(PyErr::new("bfbc2core.paths.GameCopyNotFound", m).into()),
        },
    };
    let Some(off) = offsets else {
        return Err(PyErr::new("KeyError", str_repr("--offsets")).into());
    };
    let parts: Vec<&str> = off.split(',').collect();
    let mut nums = Vec::new();
    for p in parts.iter().take(3) {
        nums.push(py_int(p)?);
    }
    if parts.len() < 2 {
        return Err(PyErr::new(
            "ValueError",
            format!("not enough values to unpack (expected 2, got {})", parts.len()),
        )
        .into());
    }
    if parts.len() > 2 {
        return Err(PyErr::new("ValueError", "too many values to unpack (expected 2)").into());
    }
    if rest.iter().any(|a| a == "--score") {
        return Err(Fail::Python); // DICE's choice (navparity.need_reference, hpdtool, kymesh): evaluation only
    }
    let lv = &args[0];
    if !lv.is_ascii() {
        return Err(Fail::Python);
    }
    let lv = lv.to_ascii_lowercase();
    let (xoff, zoff) = (nums[0], nums[1]);
    let l = join(&root, &["work", "levels", &lv])?;
    let common: Vec<String> = ["mp_common", "sp_common"]
        .iter()
        .map(|c| join(&root, &["work", "navkit", "common", c]))
        .collect::<PyResult<_>>()?;
    let flag = flagged_sectors(&l, &common, xoff, zoff)?;
    let stacked: BTreeSet<(i64, i64)> = match samples {
        // `if samples_path`: an empty argument is no samples
        Some(p) if !p.is_empty() => stacked_columns(p, xoff, zoff)?
            .into_iter()
            .filter(|(_, n)| *n >= STACKED_MIN)
            .map(|(k, _)| k)
            .collect(),
        _ => BTreeSet::new(),
    };
    let chosen = flag.union(&stacked).count();
    let only = stacked.difference(&flag).count();
    py.print(&format!(
        "{lv}: {chosen} detailed sectors ({{'flag': {}, 'stacked': {}, 'stacked only': {only}}})",
        flag.len(),
        stacked.len()
    ))?;
    Ok(0)
}
