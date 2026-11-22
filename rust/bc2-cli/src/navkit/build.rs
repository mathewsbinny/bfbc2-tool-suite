//! `navkit build` / `all` / `apply` on a NavKit base (round NK5): `navkit.build`, `build_inputs` (with
//! `detailed_sectors` and its cache, `stale_detailed_samples` and navregress's `level_water` / `reference_bounds` /
//! `order_points_of` / `area_orders_of` / `ladders_of`) and NavKit's build itself, all in Rust
//! (`bc2_navbuild::build`). What the port does not read as the Python does hands the line to the Python before
//! anything is printed (a level prepared before `area_orders.json`, a damaged cache the Python reads its own way).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bc2_navbuild::build::{self, DetailedInputs, Edits, Inputs};
use bc2_navbuild::stage::{BuildSettings, Switches};
use bc2_navbuild::walk::{Ladder, Region, WalkSettings};

use super::navkit_error;
use super::prepare::{heightfield_of, reference_bounds};
use super::project::{Log, Project};
use crate::navtools::detailed;
use crate::pyfmt::float_repr;
use crate::pyjson::{self, Value};
use crate::pyos::{self, join, join2};
use crate::pyproc::{Fail, Py, PyErr, PyResult};

/// `pr.samples(lv, suffix)`.
pub fn samples(pr: &Project, lv: &str, suffix: &str) -> PyResult<String> {
    join(&pr.work, &["navsample", &format!("{lv}_navkit{suffix}.bin")])
}

/// `pr.samples08(lv)`: `<lv>_navkit.bin`, or `<lv>_full.bin` of older campaign runs.
pub fn samples08(pr: &Project, lv: &str) -> PyResult<Option<String>> {
    for name in [format!("{lv}_navkit"), format!("{lv}_full")] {
        let q = join(&pr.work, &["navsample", &format!("{name}.bin")])?;
        if pyos::isfile(&q) {
            return Ok(Some(q));
        }
    }
    Ok(None)
}

fn int_of(v: Option<&Value>) -> PyResult<i64> {
    match v {
        Some(Value::Int(i)) => i64::try_from(*i).map_err(|_| Fail::Python),
        _ => Err(Fail::Python),
    }
}

/// `source_stamps(folder)`: a digest of every `.source.sha256` stamp under folder.
fn source_stamps(folder: &str) -> PyResult<String> {
    let mut walked = pyos::walk(folder)?;
    walked.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = sha1_smol::Sha1::new();
    for (dp, _d, fs) in walked {
        if fs.iter().any(|f| f == ".source.sha256") {
            let text = std::fs::read(join2(&dp, ".source.sha256")?).map_err(|_| Fail::Python)?;
            let text = String::from_utf8(text).map_err(|_| Fail::Python)?;
            let body = super::strip(&super::universal(&text)).to_owned();
            if !body.is_ascii() {
                return Err(Fail::Python);
            }
            h.update(pyos::relpath(&dp, folder)?.as_bytes());
            h.update(b"=");
            h.update(body.as_bytes());
            h.update(b";");
        }
    }
    Ok(h.digest().to_string())
}

/// `detailed_sectors(pr, lv)`: the sectors to build at pitch 0.2, chosen from the level alone, cached in
/// `work\navkit\<lv>\detailed.json` while the samples are the same.
fn detailed_sectors(pr: &Project, lv: &str, s: &Value) -> PyResult<Vec<(i64, i64)>> {
    let (xoff, zoff) = (int_of(s.get("xoff"))?, int_of(s.get("zoff"))?);
    let sp = samples08(pr, lv)?;
    let cache = join2(&pr.out_dir(lv)?, "detailed.json")?;
    let key = format!(
        "{}|{}|{}|{}|{xoff}|{zoff}|{}",
        sp.as_deref().map_or_else(|| Ok("None".to_owned()), pyos::basename)?,
        match &sp {
            Some(p) => float_repr(pyos::getmtime(p)?),
            None => "0".to_owned(),
        },
        float_repr(detailed::MARGIN),
        detailed::STACKED_MIN,
        source_stamps(&pr.level_work(lv)?)?
    );
    let cached = std::fs::read(&cache).ok().and_then(|d| pyjson::loads_utf8(&d));
    if let Some(c @ Value::Dict(_)) = &cached
        && c.get("key").and_then(Value::as_str) == Some(key.as_str())
        && let Some(Value::List(l)) = c.get("sectors")
    {
        return l
            .iter()
            .map(|x| match x.as_list() {
                Some([a, b]) => Ok((int_of(Some(a))?, int_of(Some(b))?)),
                _ => Err(Fail::Python),
            })
            .collect();
    }
    let common: Vec<String> = ["mp_common", "sp_common"]
        .iter()
        .map(|c| join(&pr.work, &["navkit", "common", c]))
        .collect::<PyResult<_>>()?;
    let flag = detailed::flagged_sectors(&pr.level_work(lv)?, &common, xoff, zoff)?;
    let stacked: BTreeSet<(i64, i64)> = match &sp {
        Some(p) => detailed::stacked_columns(p, xoff, zoff)?
            .into_iter()
            .filter(|&(_, n)| n >= detailed::STACKED_MIN)
            .map(|(k, _)| k)
            .collect(),
        None => BTreeSet::new(),
    };
    let chosen: Vec<(i64, i64)> = flag.union(&stacked).copied().collect();
    let why = Value::dict([
        ("flag", Value::Int(flag.len() as i128)),
        ("stacked", Value::Int(stacked.len() as i128)),
        ("stacked only", Value::Int(stacked.difference(&flag).count() as i128)),
    ]);
    let doc = Value::dict([
        ("key", Value::Str(key)),
        (
            "sectors",
            Value::List(
                chosen
                    .iter()
                    .map(|&(a, b)| Value::List(vec![Value::Int(i128::from(a)), Value::Int(i128::from(b))]))
                    .collect(),
            ),
        ),
        ("why", why),
    ]);
    // atomicio.write_json(cache, ...): json.dump's text (indent None), whole or not at all
    bc2_formats::atomic::write(Path::new(&cache), &[pyjson::dumps(&doc, None).as_bytes()])
        .map_err(|e| Fail::Err(PyErr::new("OSError", e.to_string())))?;
    Ok(chosen)
}

/// `sample_grid(path)`: (pitch, min x, min z, nx, nz) from a navsample file's header.
fn sample_grid(path: &str) -> PyResult<(f64, f64, f64, i64, i64)> {
    let data = std::fs::read(path).map_err(|e| crate::pyproc::os_error(path, &e))?;
    let name = pyos::basename(path)?;
    if data.len() < 56 {
        return Err(navkit_error(format!("{name}: not a navsample file (too short)")));
    }
    if data.get(0..4) != Some(b"ONS1".as_slice()) {
        return Err(navkit_error(format!("{name}: not a navsample file")));
    }
    let u = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let f = |o: usize| f64::from(f32::from_bits(u(o)));
    Ok((f(8), f(12), f(16), i64::from(u(20) as i32), i64::from(u(24) as i32)))
}

/// `stale_detailed_samples(...)`: pitch-0.2 samples that do not fit the build refused.
fn stale_detailed_samples(
    lv: &str,
    p02: &str,
    hi02: Option<&str>,
    sectors: &[(i64, i64)],
    (xoff, zoff): (i64, i64),
) -> PyResult<()> {
    let redo = format!(" - sample them again (navkit.py sample {} --pitch 0.2)", lv.to_uppercase());
    let g = sample_grid(p02)?;
    if (g.0 - 0.2).abs() > 1e-4 {
        return Err(navkit_error(format!(
            "{lv}: {} is at pitch {}, not 0.2{redo}",
            pyos::basename(p02)?,
            bc2_navbuild::stage::py_g(g.0)
        )));
    }
    if let Some(h) = hi02
        && sample_grid(h)? != g
    {
        return Err(navkit_error(format!(
            "{lv}: {} and {} are on different grids (one is from an older box){redo}",
            pyos::basename(h)?,
            pyos::basename(p02)?
        )));
    }
    let (pitch, min_x, min_z, nx, nz) = g;
    let (max_x, max_z) = (min_x + (nx - 1) as f64 * pitch, min_z + (nz - 1) as f64 * pitch);
    let out: Vec<&(i64, i64)> = sectors
        .iter()
        .filter(|&&(i, j)| {
            let (x0, z0) = ((i + xoff) as f64 * 100.0, (j + zoff) as f64 * 100.0);
            x0 < min_x - 1e-3 || x0 + 100.0 > max_x + 1e-3 || z0 < min_z - 1e-3 || z0 + 100.0 > max_z + 1e-3
        })
        .collect();
    if let Some(first) = out.first() {
        return Err(navkit_error(format!(
            "{lv}: {} (x {min_x:.1}..{max_x:.1}, z {min_z:.1}..{max_z:.1}) does not cover {} detailed sector(s), e.g. \
             ({}, {}) - made for another box or other sector offsets{redo}",
            pyos::basename(p02)?,
            out.len(),
            first.0,
            first.1
        )));
    }
    Ok(())
}

/// A JSON list of points (x, y, z), each as Python's `json.load` gives the numbers.
fn points(v: &Value) -> PyResult<Vec<(f64, f64, f64)>> {
    let l = v.as_list().ok_or(Fail::Python)?;
    l.iter()
        .map(|p| match p.as_list() {
            Some([x, y, z]) => Ok((x.as_f64().ok_or(Fail::Python)?, y.as_f64().ok_or(Fail::Python)?, z.as_f64().ok_or(Fail::Python)?)),
            _ => Err(Fail::Python),
        })
        .collect()
}

fn load_json(path: &str) -> PyResult<Value> {
    let data = std::fs::read(path).map_err(|e| crate::pyproc::os_error(path, &e))?;
    pyjson::loads_ansi(&data).ok_or(Fail::Python)
}

/// `level.water_of(root, lv)` as `navregress.level_water` takes it: (vertices, triangles), or `None` (no water).
fn level_water(py: &mut Py, pr: &Project, lv: &str) -> PyResult<Option<bc2_navbuild::walk::WaterMesh>> {
    struct Quiet;
    impl Log for Quiet {
        fn log(&mut self, _py: &mut Py, _text: &str) -> PyResult<()> {
            Ok(())
        }
    }
    let name = heightfield_of(py, &pr.root, lv, &mut Quiet)?.to_lowercase();
    let dest = join(&pr.root, &["work", "levels", lv, "terrain"])?;
    let mut mesh = None;
    if pyos::isdir(&dest) {
        for (root, _, files) in pyos::walk(&dest)? {
            for f in files {
                if f.to_lowercase() == format!("{name}.water.mesh.res") {
                    mesh = Some(join2(&root, &f)?);
                }
            }
        }
    }
    let Some(mesh) = mesh else { return Ok(None) };
    let data = pyos::read(&mesh)?;
    let w = bc2_formats::water::parse_mesh(&data)
        .map_err(|e| Fail::Err(PyErr::new("ValueError", format!("{mesh}: {e}"))))?;
    let tris: Vec<[i64; 3]> = w.triangles.iter().map(|t| t.map(i64::from)).collect();
    if tris.is_empty() {
        return Ok(None);
    }
    let verts = w.vertices.iter().map(|v| v.map(f64::from)).collect();
    Ok(Some((verts, tris)))
}

/// What `build_inputs` gives: the inputs, the lines it logs (each printed after `== build`).
pub struct Gathered {
    pub inputs: Inputs,
    pub lines: Vec<String>,
}

/// `build_inputs(pr, lv, log)`: navbuild.build's inputs for a level (the same for build and apply).
pub fn build_inputs(py: &mut Py, pr: &Project, lv: &str) -> PyResult<Gathered> {
    let s = pr.settings(lv)?;
    let l = pr.level_work(lv)?;
    let (xoff, zoff) = (int_of(s.get("xoff"))?, int_of(s.get("zoff"))?);
    let Some(s08) = samples08(pr, lv)?.filter(|p| pyos::isfile(&format!("{}_hi.bin", &p[..p.len() - 4]))) else {
        return Err(navkit_error(format!(
            "{lv}: samples missing ({} and its _hi run) - run sample first",
            pyos::basename(&samples(pr, lv, "")?)?
        )));
    };
    let mut lines = Vec::new();
    let mut inp = Inputs::new(PathBuf::from(&s08), xoff, zoff);
    let p02 = samples(pr, lv, "_p02")?;
    if pyos::isfile(&p02) {
        let hi02 = samples(pr, lv, "_hi_p02")?;
        let high = pyos::isfile(&hi02).then_some(hi02);
        let sectors = detailed_sectors(pr, lv, &s)?;
        let set: BTreeSet<(i64, i64)> = sectors.iter().copied().collect();
        let sorted: Vec<(i64, i64)> = set.iter().copied().collect();
        stale_detailed_samples(lv, &p02, high.as_deref(), &sorted, (xoff, zoff))?;
        lines.push(format!(
            "  detailed zones: {} sectors at pitch 0.2{}",
            set.len(),
            if high.is_some() {
                ""
            } else {
                " (no high-capsule run at 0.2 - stairs use the normal capsule there)"
            }
        ));
        inp.detailed = Some(DetailedInputs {
            samples: PathBuf::from(p02),
            high: high.map(PathBuf::from),
            sectors: set,
        });
    }
    let npy = |name: &str| -> PyResult<Option<bc2_navbuild::walk::Tris>> {
        let p = join2(&l, name)?;
        if !pyos::isfile(&p) {
            return Err(Fail::Err(PyErr::new(
                "FileNotFoundError",
                format!("[Errno 2] No such file or directory: {}", crate::pyfmt::str_repr(&p)),
            )));
        }
        build::read_tris_npy(Path::new(&p)).map_err(|_| Fail::Python)
    };
    inp.high = Some(PathBuf::from(format!("{}_hi.bin", &s08[..s08.len() - 4])));
    inp.seeds = points(&load_json(&join2(&l, "soldier_spawns.json")?)?)?;
    inp.water = level_water(py, pr, lv)?;
    inp.bounds = reference_bounds(pr, lv)?;
    let op = join2(&l, "order_points.json")?;
    inp.order_points = if pyos::isfile(&op) {
        Some(points(&load_json(&op)?)?)
    } else {
        None
    };
    let ao = join2(&l, "area_orders.json")?;
    if !pyos::isfile(&ao) {
        return Err(Fail::Python); // read from the level's XML (levelobjects.area_orders): the Python's
    }
    inp.area_orders = Some(points(&load_json(&ao)?)?);
    let lp = join2(&l, "ladders.json")?;
    if pyos::isfile(&lp) {
        let v = load_json(&lp)?;
        let list = v.as_list().ok_or(Fail::Python)?;
        let mut ladders = Vec::new();
        for lad in list {
            let p3 = |k: &str| -> PyResult<[f64; 3]> {
                match lad.get(k).and_then(Value::as_list) {
                    Some([a, b, c]) => Ok([
                        a.as_f64().ok_or(Fail::Python)?,
                        b.as_f64().ok_or(Fail::Python)?,
                        c.as_f64().ok_or(Fail::Python)?,
                    ]),
                    _ => Err(Fail::Python),
                }
            };
            ladders.push(Ladder {
                lo: p3("lo")?,
                hi: p3("hi")?,
            });
        }
        inp.ladders = Some(ladders);
    }
    inp.forbid = npy("forbid_tris.npy")?;
    inp.dynamic = npy("dynamic_tris.npy")?;
    inp.stairs = npy("stair_tris.npy")?;
    Ok(Gathered { inputs: inp, lines })
}

/// `time.time()`.
fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// The build's log: each line printed with two spaces before it (and kept, for apply's report).
struct Lines<'a> {
    py: &'a mut Py,
    t0: f64,
    kept: Option<&'a mut Vec<String>>,
    err: Option<Fail>,
}

impl bc2_navbuild::stage::Log for Lines<'_> {
    fn line(&mut self, text: &str) {
        let msg = format!("  {text}");
        if let Err(e) = self.py.print(&msg) {
            self.err.get_or_insert(e);
        }
        if let Some(k) = self.kept.as_deref_mut() {
            k.push(msg);
        }
    }

    fn seconds(&self) -> f64 {
        now_seconds() - self.t0
    }
}

/// The build's settings (`--no-detail`: no D lines or F cells).
pub fn settings(no_detail: bool) -> BuildSettings {
    BuildSettings {
        detail_features: !no_detail,
        ..BuildSettings::default()
    }
}

/// `navbuild.build(...)` with the level's inputs, its lines through `py` (two spaces before each).
pub fn run_build(
    py: &mut Py,
    inp: &Inputs,
    out: &str,
    no_detail: bool,
    kept: Option<&mut Vec<String>>,
) -> PyResult<()> {
    let t0 = now_seconds();
    let now = crate::clock::now();
    let clock = build::clock_words(
        u32::from(now.year),
        u32::from(now.month),
        u32::from(now.day),
        u32::from(now.hour),
        u32::from(now.minute),
        u32::from(now.second),
    );
    let mut lg = Lines {
        py,
        t0,
        kept,
        err: None,
    };
    let got = build::build(
        inp,
        Path::new(out),
        &settings(no_detail),
        &Switches::default(),
        &WalkSettings::navkit(40.0),
        clock,
        &mut lg,
    );
    if let Some(e) = lg.err {
        return Err(e);
    }
    got.map(drop).map_err(|e| Fail::Err(PyErr::new("RuntimeError", e.to_string())))
}

/// `navkit.build(pr, lv, cfg=cfg)`.
pub fn build_level(py: &mut Py, pr: &Project, lv: &str, no_detail: bool) -> PyResult<()> {
    let out = join2(&pr.out_dir(lv)?, "pathdata.hpd")?;
    let t0 = Instant::now();
    let gathered = build_inputs(py, pr, lv);
    if matches!(gathered, Err(Fail::Python)) {
        return Err(Fail::Python);
    }
    py.print(&format!("== build {lv} -> {}", pyos::relpath(&out, &pr.root)?))?;
    let g = gathered?;
    for l in &g.lines {
        py.print(l)?;
    }
    let trav = join2(&pyos::dirname(&out)?, &format!("{lv}.traversal.json"))?;
    if pyos::isfile(&trav) {
        std::fs::remove_file(&trav).map_err(|e| crate::pyproc::os_error(&trav, &e))?;
    }
    run_build(py, &g.inputs, &out, no_detail, None)?;
    let size = pyos::getsize(&out)? as f64 / 1e6;
    py.print(&format!(
        "  built in {:.0} s ({size:.1} MB)",
        t0.elapsed().as_secs_f64()
    ))
}

/// A side file's walkability operations as the build takes them (`navedits.regions_of`).
pub fn edits_of(ops: &[Value]) -> PyResult<Edits> {
    let mut e = Edits::default();
    for o in ops {
        let Some(name) = o.get("op").and_then(Value::as_str) else { continue };
        if !matches!(name, "walkable" | "unwalkable" | "seed" | "unseed") {
            continue;
        }
        match e.counts.iter_mut().find(|(k, _)| k == name) {
            Some(c) => c.1 += 1,
            None => e.counts.push((name.to_owned(), 1)),
        }
        let at = || -> PyResult<(f64, f64, f64)> {
            match o.get("at").and_then(Value::as_list) {
                Some([x, y, z]) => Ok((
                    x.as_f64().ok_or(Fail::Python)?,
                    y.as_f64().ok_or(Fail::Python)?,
                    z.as_f64().ok_or(Fail::Python)?,
                )),
                _ => Err(Fail::Python),
            }
        };
        match name {
            "walkable" | "unwalkable" => e.regions.push((name == "walkable", region_of(o.get("region"))?)),
            "seed" => e.seeds.push(at()?),
            _ => e.unseeds.push(at()?),
        }
    }
    Ok(e)
}

/// `navedits.region_of(region)`.
fn region_of(r: Option<&Value>) -> PyResult<Region> {
    let r = r.ok_or(Fail::Python)?;
    let f = |v: &Value| v.as_f64().ok_or(Fail::Python);
    if let Some(b) = r.get("box") {
        let Some([a, b2, c, d, e, g]) = b.as_list() else { return Err(Fail::Python) };
        return Ok(Region::Box([f(a)?, f(b2)?, f(c)?, f(d)?, f(e)?, f(g)?]));
    }
    let Some([lo, hi]) = r.get("y").and_then(Value::as_list) else {
        return Err(Fail::Python);
    };
    let poly = r.get("polygon").and_then(Value::as_list).ok_or(Fail::Python)?;
    if poly.is_empty() {
        return Err(Fail::Python);
    }
    let poly = poly
        .iter()
        .map(|p| match p.as_list() {
            Some([a, b]) => Ok((f(a)?, f(b)?)),
            _ => Err(Fail::Python),
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(Region::Polygon {
        y: (f(lo)?, f(hi)?),
        poly,
    })
}
