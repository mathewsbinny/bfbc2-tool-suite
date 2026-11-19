//! `bfbc2-auto-navkit\tools\ladders.py`: the level's placed ladders as world boxes (`ladders.json`), for NavKit's
//! ladder links. NumPy's arithmetic: the float32 corners widened, `V = tris @ R + translation` in float64.

use bc2_scene::collision;

use super::levelobjects::triangles;
use crate::pyfmt;
use crate::pyjson::{self, Value};
use crate::pyos::{self, join2};
use crate::pyproc::{Fail, Py, PyErr, PyResult};

/// `extract(level_dir, extra_bases)`: [{'bp', 'lo', 'hi'}].
pub fn extract(level_dir: &str, extra: &[String]) -> PyResult<Value> {
    let mut bases = vec![join2(level_dir, "pkg")?, join2(level_dir, "dist")?];
    bases.extend(extra.iter().cloned());
    let bases: Vec<std::path::PathBuf> = bases
        .into_iter()
        .filter(|b| pyos::isdir(b))
        .map(std::path::PathBuf::from)
        .collect();
    let hv = collision::havok_index(&bases);
    let data = pyos::read(&join2(level_dir, "objects.json")?)?;
    let objects = pyjson::loads_ansi(&data).ok_or(Fail::Python)?;
    let mut objs: Vec<(&str, Vec<f64>)> = Vec::new();
    for o in objects.as_list().ok_or(Fail::Python)? {
        let bp = o.get("bp").and_then(Value::as_str).ok_or(Fail::Python)?;
        let low = bp.to_lowercase();
        if low.rsplit('/').next().unwrap_or("").contains("ladder") && hv.contains(&low) {
            let m: Vec<f64> = o
                .get("m")
                .and_then(Value::as_list)
                .ok_or(Fail::Python)?
                .iter()
                .map(|v| v.as_f64().ok_or(Fail::Python))
                .collect::<PyResult<_>>()?;
            objs.push((bp, m));
        }
    }
    // blueprint_triangles: each packfile read once, in the first ladder's order
    let mut local: Vec<(String, Option<Vec<[[f32; 3]; 3]>>)> = Vec::new();
    for (bp, _) in &objs {
        let low = bp.to_lowercase();
        if !local.iter().any(|(k, _)| *k == low) {
            let p = hv
                .get(&low)
                .map_or_else(String::new, |p| p.to_string_lossy().into_owned());
            local.push((low, None));
            let t = triangles(&p)?;
            if let Some(slot) = local.last_mut() {
                slot.1 = t;
            }
        }
    }
    let mut out = Vec::new();
    for (bp, m) in &objs {
        let low = bp.to_lowercase();
        let Some(Some(tris)) = local.iter().find(|(k, _)| *k == low).map(|(_, t)| t) else {
            continue;
        };
        if m.len() < 15 {
            return Err(Fail::Python);
        }
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        if tris.is_empty() {
            return Err(Fail::Err(PyErr::new(
                "ValueError",
                "zero-size array to reduction operation minimum which has no identity",
            )));
        }
        for p in tris.iter().flatten() {
            let (a, b, c) = (f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
            for j in 0..3 {
                let v = a * m[j] + b * m[4 + j] + c * m[8 + j] + m[12 + j];
                if v.is_nan() {
                    return Err(Fail::Python);
                }
                lo[j] = lo[j].min(v);
                hi[j] = hi[j].max(v);
            }
        }
        if hi[1] - lo[1] < 1.5 {
            continue;
        }
        let r = |v: [f64; 3]| Value::floats(&v.map(|x| pyfmt::round_to(x, 3)));
        out.push(Value::dict([
            ("bp", Value::Str((*bp).to_owned())),
            ("lo", r(lo)),
            ("hi", r(hi)),
        ]));
    }
    Ok(Value::List(out))
}

/// `ladders.py <level dir> [<more blueprint folders>...]`.
pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.is_empty() {
        py.print(&pyfmt::docstring(include_str!(
            "../../../../bfbc2-auto-navkit/tools/ladders.py"
        )))?;
        return Ok(1);
    }
    let lad = extract(&args[0], &args[1..])?;
    let path = join2(&args[0], "ladders.json")?;
    std::fs::write(&path, pyjson::dumps(&lad, Some(1)).replace('\n', "\r\n"))
        .map_err(|e| crate::pyproc::os_error(&path, &e))?;
    let n = lad.as_list().map_or(0, <[Value]>::len);
    py.print(&format!("{n} ladders -> {path}"))?;
    Ok(0)
}
