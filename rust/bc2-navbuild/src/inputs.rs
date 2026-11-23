//! NavKit's level driver as a library (round NK6): `navkit.build_inputs` - a prepared level's build inputs gathered
//! from OPFOR's work folder (the samples, the detailed sectors and their cache, the spawns, orders and objectives, the
//! water, DICE's extent, the ladders, the triangle lists) - and `navkit.build`, so the command line (`bfbc2.exe navkit
//! build / all / apply`), the editor's AI workspace (in process, round NK6) and the export's path-data step run one
//! code and write the same bytes and lines.
//!
//! Moved from `bc2-cli` (round NK5's `navkit/build.rs`, `prepare.rs`'s `heightfield_of` / `reference_bounds`,
//! `navtools/detailed.rs`'s choice, `navshared.rs`'s `original_file`) onto this crate's own [`crate::ntpath`] and
//! [`crate::pyjson`]: the JSON the side files hold is read and written with Python's numbers.
//!
//! **Errors** are [`Fail`]: the Python's exception, or a hand-off where the port does not read an input as the Python
//! did (the command line then runs the Python; the editor says so).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::many_single_char_names,
    clippy::verbose_bit_mask,
    reason = "a port of Python: ints as Python turns them into floats, indexes after their lengths are checked, == as \
              Python compares floats, the Python's functions' shape kept"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use bc2_shader::pyrepr::float_repr;

use crate::build::{self, DetailedInputs, Edits, Inputs, Report};
use crate::ntpath::{self as os, Fail, Res, join, join2, navkit_error, os_error, raise};
use crate::pyjson::{self, Value};
use crate::stage::{BuildSettings, Switches};
use crate::walk::{Ladder, Region, WalkSettings, WaterMesh};

/// The detailed choice's margin around a flagged object's collision (m).
pub const MARGIN: f64 = 5.0;
/// Columns with stacked floors that make a sector detailed.
pub const STACKED_MIN: u64 = 100;
const STACK_GAP: f64 = 2.0;
const SLOPE_MAX: f64 = 40.0;

/// Where a driver step's lines go, and whether to stop.
pub trait Out {
    /// One line as the command line prints it.
    ///
    /// # Errors
    /// The sink failed ([`Fail::Sink`]; the sink keeps its own error).
    fn line(&mut self, text: &str) -> Res<()>;
    /// Whether the run was cancelled (checked between the build's steps).
    fn cancelled(&self) -> bool {
        false
    }
}

/// An in-process tool run (`FbrbTool unpack`): its exit code. The command line logs it as the Python's tool runs are
/// logged; the editor runs it quietly.
pub type ToolRun<'a> = &'a dyn Fn(&[String]) -> i32;

/// [`ToolRun`] without a log.
#[must_use]
pub fn quiet_tool(args: &[String]) -> i32 {
    bc2_tools::fbrbtool::run(args, None).code
}

/// OPFOR's folders as `navkit.Project` names them (no check that a game copy is there: the caller's).
#[derive(Debug, Clone)]
pub struct Root {
    /// The OPFOR folder.
    pub root: String,
    /// `<root>\game`.
    pub game: String,
    /// `<root>\work`.
    pub work: String,
    /// The samples' folder when not `<work>\navsample` (the offline sampler's `<work>\navsample_offline`, or a
    /// stage's own samples: the export's path-data step).
    pub samples_dir: Option<String>,
}

impl Root {
    /// `Project(root)`'s folders.
    pub fn new(root: &str) -> Res<Self> {
        Ok(Self {
            root: root.to_owned(),
            game: join2(root, "game")?,
            work: join2(root, "work")?,
            samples_dir: None,
        })
    }

    /// `<work>\levels\<lv>`.
    pub fn level_work(&self, lv: &str) -> Res<String> {
        join(&self.work, &["levels", lv])
    }

    /// `out_dir(lv)` (made when missing).
    pub fn out_dir(&self, lv: &str) -> Res<String> {
        let d = join(&self.work, &["navkit", lv])?;
        os::makedirs(&d)?;
        Ok(d)
    }

    /// The level's installed path data in the game copy.
    pub fn pathdata(&self, lv: &str) -> Res<String> {
        join(
            &self.game,
            &["Output", "win32", "ai", "levels", lv, "infantry", "pathdata.hpd"],
        )
    }

    /// DICE's original path data of the level, or `None`.
    pub fn dice_pathdata(&self, lv: &str) -> Res<Option<String>> {
        original_file(&self.pathdata(lv)?)
    }

    /// `settings(lv)`: `work\navkit\<lv>\navkit.json`.
    pub fn settings(&self, lv: &str) -> Res<Value> {
        let p = join2(&self.out_dir(lv)?, "navkit.json")?;
        if !os::isfile(&p) {
            return Err(navkit_error(format!(
                "{lv}: not prepared yet (navkit.py prepare {})",
                lv.to_uppercase()
            )));
        }
        let data = os::read(&p)?;
        // bytes that are not UTF-8, a UTF-8 BOM and invalid JSON are all ValueError in Python: "damaged"
        let text = std::str::from_utf8(&data).unwrap_or("\u{0}");
        match pyjson::loads(&os::universal(text.strip_prefix('\u{feff}').map_or(text, |_| "\u{0}"))) {
            Some(v) => Ok(v),
            None => Err(navkit_error(format!(
                "{lv}: {p} is damaged - prepare the level again (navkit.py prepare {})",
                lv.to_uppercase()
            ))),
        }
    }

    /// `pr.samples(lv, suffix)`.
    pub fn samples(&self, lv: &str, suffix: &str) -> Res<String> {
        join2(&self.samples_folder()?, &format!("{lv}_navkit{suffix}.bin"))
    }

    /// The samples' folder: `<work>\navsample`, or [`Root::samples_dir`].
    pub fn samples_folder(&self) -> Res<String> {
        match &self.samples_dir {
            Some(d) => Ok(d.clone()),
            None => join2(&self.work, "navsample"),
        }
    }

    /// `pr.samples08(lv)`: `<lv>_navkit.bin`, or `<lv>_full.bin` of older campaign runs.
    pub fn samples08(&self, lv: &str) -> Res<Option<String>> {
        for name in [format!("{lv}_navkit"), format!("{lv}_full")] {
            let q = join2(&self.samples_folder()?, &format!("{name}.bin"))?;
            if os::isfile(&q) {
                return Ok(Some(q));
            }
        }
        Ok(None)
    }
}

/// `navkit.original_file(p)`: DICE's original of a game file, or None - from the game copy's install ledger when an
/// owner holds the file there, else by the markers of before the ledger (the earliest OPFOR backup).
pub fn original_file(p: &str) -> Res<Option<String>> {
    let path = Path::new(p);
    if let Some(game) = bc2_gamefs::game::find_game(path) {
        let led = bc2_gamefs::ledger::Ledger::new(&game);
        let rel = os::relpath(p, &game.to_string_lossy())?;
        let fail = |e: bc2_gamefs::ledger::LedgerError| raise("LedgerError", e.to_string());
        if !led.holders(&rel).map_err(fail)?.is_empty() {
            return Ok(led
                .original(&rel)
                .map_err(fail)?
                .map(|b| b.to_string_lossy().into_owned()));
        }
    }
    let (d, base) = os::split(p)?;
    let Ok(listing) = os::listdir(if d.is_empty() { "." } else { &d }) else {
        return Ok(None);
    };
    // {f.lower(): f}: the first position of a lower-case name, the last spelling
    let mut names: Vec<(String, String)> = Vec::new();
    for f in listing {
        let low = f.to_lowercase();
        if let Some(slot) = names.iter_mut().find(|(k, _)| *k == low) {
            slot.1 = f;
        } else {
            names.push((low, f));
        }
    }
    let pre = format!("{}.opfor-", base.to_lowercase());
    let ours: Vec<&(String, String)> = names.iter().filter(|(k, _)| k.starts_with(&pre)).collect();
    let has = |k: &str| ours.iter().any(|(o, _)| o == k);
    if has(&format!("{pre}new")) {
        let own = format!("{pre}orig");
        return Ok(match ours.iter().find(|(o, _)| *o == own) {
            Some((_, f)) => Some(join2(&d, f)?),
            None => None,
        });
    }
    if ours.iter().any(|(o, _)| {
        o.strip_suffix("-applied")
            .is_some_and(|stem| !has(&format!("{stem}-orig")))
    }) {
        return Ok(None);
    }
    let mut best: Option<((f64, bool), String)> = None;
    for (o, f) in &ours {
        if !o.ends_with("-orig") {
            continue;
        }
        let b = join2(&d, f)?;
        if !os::isfile(&b) {
            continue;
        }
        let key = (os::getmtime(&b)?, !b.to_lowercase().ends_with(".opfor-orig"));
        if best
            .as_ref()
            .is_none_or(|(k, _)| key.0 < k.0 || (key.0 == k.0 && !key.1 && k.1))
        {
            best = Some((key, b));
        }
    }
    if let Some((_, b)) = best {
        return Ok(Some(b));
    }
    Ok(os::isfile(p).then(|| p.to_owned()))
}

fn int_of(v: Option<&Value>) -> Res<i64> {
    match v {
        Some(Value::Int(i)) => i64::try_from(*i).map_err(|_| Fail::Handoff),
        _ => Err(Fail::Handoff),
    }
}

/// `source_stamps(folder)`: a digest of every `.source.sha256` stamp under folder.
fn source_stamps(folder: &str) -> Res<String> {
    let mut walked = os::walk(folder)?;
    walked.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = sha1_smol::Sha1::new();
    for (dp, _d, fs) in walked {
        if fs.iter().any(|f| f == ".source.sha256") {
            let text = std::fs::read(join2(&dp, ".source.sha256")?).map_err(|_| Fail::Handoff)?;
            let text = String::from_utf8(text).map_err(|_| Fail::Handoff)?;
            let body = os::strip(&os::universal(&text)).to_owned();
            if !body.is_ascii() {
                return Err(Fail::Handoff);
            }
            h.update(os::relpath(&dp, folder)?.as_bytes());
            h.update(b"=");
            h.update(body.as_bytes());
            h.update(b";");
        }
    }
    Ok(h.digest().to_string())
}

/// A JSON value's truth as Python's `bool()` gives it.
fn truthy(v: &Value) -> bool {
    v.truthy()
}

/// A matrix entry as NumPy's array of Python numbers holds it (ints and floats only; anything else hands off).
fn num(v: &Value) -> Res<f64> {
    match v {
        Value::Int(i) if i.unsigned_abs() < (1 << 53) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        _ => Err(Fail::Handoff),
    }
}

/// `int(math.floor(x / 100))`.
fn sector_of(x: f64) -> Res<i64> {
    let s = (x / 100.0).floor();
    if !s.is_finite() || s.abs() > 9e15 {
        return Err(Fail::Handoff);
    }
    Ok(s as i64)
}

/// One flagged object: its blueprint (lower case), the rotation rows, the translation.
type Flagged = (String, [[f64; 3]; 3], [f64; 3]);

/// The flagged objects of `objects.json`, in its order.
fn flagged_objects(objects: &[Value]) -> Res<Vec<Flagged>> {
    let mut flagged = Vec::new();
    for o in objects {
        let Value::Dict(_) = o else {
            return Err(Fail::Handoff);
        };
        let flag = match o.get("flags") {
            Some(f) if truthy(f) => match f {
                Value::List(l) => truthy(&l[0]),
                _ => return Err(Fail::Handoff), // a string's or dict's [0]: the Python's own answer
            },
            _ => false,
        };
        if !flag {
            continue;
        }
        let Some(Value::Str(bp)) = o.get("bp") else {
            return Err(Fail::Handoff);
        };
        if !bp.is_ascii() {
            return Err(Fail::Handoff); // str.lower beyond ASCII: the Python's
        }
        let Some(Value::List(m)) = o.get("m") else {
            return Err(Fail::Handoff);
        };
        if m.len() < 15 {
            return Err(Fail::Handoff);
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

/// `detailed.flagged_sectors(level_dir, common, xoff, zoff)`: the sectors near a `NeedsDetailedPathfinding`
/// object's collision box (its blueprint's Havok triangles through the suite's index), `MARGIN` around it.
pub fn flagged_sectors(level_dir: &str, common: &[String], xoff: i64, zoff: i64) -> Res<BTreeSet<(i64, i64)>> {
    let mut bases: Vec<String> = vec![join(level_dir, &["pkg"])?, join(level_dir, &["dist"])?];
    bases.extend(common.iter().cloned());
    bases.retain(|b| os::isdir(b));
    let hv = bc2_scene::collision::havok_index(&bases);
    let data = std::fs::read(join(level_dir, &["objects.json"])?).map_err(|_| Fail::Handoff)?;
    let Some(Value::List(objects)) = pyjson::loads_ansi(&data) else {
        return Err(Fail::Handoff);
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
                let data = std::fs::read(p).map_err(|_| Fail::Handoff)?;
                match bc2_formats::havok::triangles(&data) {
                    Ok(t) => t.map(|v| v.into_iter().collect::<Vec<_>>()),
                    Err(_) => return Err(Fail::Handoff),
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
            Some(tris) if tris.is_empty() => return Err(Fail::Handoff), // .min() of nothing: ValueError
            Some(tris) => {
                // tris.reshape(-1, 3) @ R + t in float64; the x and z columns' extremes
                let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
                for p in tris.iter().flatten() {
                    let (u, v, w) = (f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
                    let x = u * r[0][0] + v * r[1][0] + w * r[2][0] + t[0];
                    let z = u * r[0][2] + v * r[1][2] + w * r[2][2] + t[2];
                    if x.is_nan() || z.is_nan() {
                        return Err(Fail::Handoff); // NumPy's min / max of a NaN
                    }
                    b = (b.0.min(x), b.1.min(z), b.2.max(x), b.3.max(z));
                }
                b
            }
        };
        let (xa, xb) = (sector_of(x0 - MARGIN)? - xoff, sector_of(x1 + MARGIN)? - xoff);
        let (za, zb) = (sector_of(z0 - MARGIN)? - zoff, sector_of(z1 + MARGIN)? - zoff);
        if (xb - xa).saturating_mul(zb - za) > 1_000_000 {
            return Err(Fail::Handoff);
        }
        for x in xa..=xb {
            for z in za..=zb {
                out.insert((x, z));
            }
        }
    }
    Ok(out)
}

/// `detailed.stacked_columns(samples_path, xoff, zoff)`: {sector: columns with standable surfaces `STACK_GAP` apart}.
pub fn stacked_columns(path: &str, xoff: i64, zoff: i64) -> Res<BTreeMap<(i64, i64), u64>> {
    use crate::navsample;
    let data = std::fs::read(path).map_err(|_| Fail::Handoff)?;
    // HEADER '<4sI3f2i6fI'
    if data.len() < 56 || &data[0..4] != b"ONS1" {
        return Err(Fail::Handoff);
    }
    let u32_at = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let f32_at = |o: usize| f64::from(f32::from_bits(u32_at(o)));
    let version = u32_at(4);
    let (pitch, min_x, min_z) = (f32_at(8), f32_at(12), f32_at(16));
    let (nx, nz) = (i64::from(u32_at(20) as i32), i64::from(u32_at(24) as i32));
    if nx <= 0 || nz <= 0 || nx.saturating_mul(nz) > 1 << 32 {
        return Err(Fail::Handoff);
    }
    let runs = match version {
        3 => navsample::parse_v3(&data, nx, nz).map_err(|_| Fail::Handoff)?,
        1 | 2 => navsample::parse_runs(&data, 56, (nx * nz) as usize, version).map_err(|_| Fail::Handoff)?,
        _ => return Err(Fail::Handoff),
    };
    let cols = (nx * nz) as usize;
    if runs.start.len() != cols + 1 || runs.normal.len() != 3 * runs.y.len() || runs.flags.len() != runs.y.len() {
        return Err(Fail::Handoff);
    }
    // NumPy 2: the float32 normals against a Python float, compared in float32
    let limit = (SLOPE_MAX.to_radians().cos() - 0.01) as f32;
    let mut out: BTreeMap<(i64, i64), u64> = BTreeMap::new();
    for c in 0..cols {
        let (a, b) = (runs.start[c], runs.start[c + 1]);
        if a < 0 || b < a || b as usize > runs.y.len() {
            return Err(Fail::Handoff);
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

/// `detailed_sectors(pr, lv)`: the sectors to build at pitch 0.2, chosen from the level alone, cached in
/// `work\navkit\<lv>\detailed.json` while the samples are the same.
fn detailed_sectors(r: &Root, lv: &str, s: &Value) -> Res<Vec<(i64, i64)>> {
    let (xoff, zoff) = (int_of(s.get("xoff"))?, int_of(s.get("zoff"))?);
    let sp = r.samples08(lv)?;
    let cache = join2(&r.out_dir(lv)?, "detailed.json")?;
    let key = format!(
        "{}|{}|{}|{}|{xoff}|{zoff}|{}",
        sp.as_deref().map_or_else(|| Ok("None".to_owned()), os::basename)?,
        match &sp {
            Some(p) => float_repr(os::getmtime(p)?),
            None => "0".to_owned(),
        },
        float_repr(MARGIN),
        STACKED_MIN,
        source_stamps(&r.level_work(lv)?)?
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
                _ => Err(Fail::Handoff),
            })
            .collect();
    }
    let common: Vec<String> = ["mp_common", "sp_common"]
        .iter()
        .map(|c| join(&r.work, &["navkit", "common", c]))
        .collect::<Res<_>>()?;
    let flag = flagged_sectors(&r.level_work(lv)?, &common, xoff, zoff)?;
    let stacked: BTreeSet<(i64, i64)> = match &sp {
        Some(p) => stacked_columns(p, xoff, zoff)?
            .into_iter()
            .filter(|&(_, n)| n >= STACKED_MIN)
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
    bc2_gamefs::atomic::write(Path::new(&cache), &[pyjson::dumps(&doc, None).as_bytes()])
        .map_err(|e| raise("OSError", e.to_string()))?;
    Ok(chosen)
}

/// `sample_grid(path)`: (pitch, min x, min z, nx, nz) from a navsample file's header.
fn sample_grid(path: &str) -> Res<(f64, f64, f64, i64, i64)> {
    let data = std::fs::read(path).map_err(|e| os_error(path, &e))?;
    let name = os::basename(path)?;
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

/// `uncovered(g, sectors, s)`: the sectors whose 100 m square the sample grid g does not cover, in the order given.
fn uncovered(g: (f64, f64, f64, i64, i64), sectors: &[(i64, i64)], (xoff, zoff): (i64, i64)) -> Vec<(i64, i64)> {
    let (pitch, min_x, min_z, nx, nz) = g;
    let (max_x, max_z) = (min_x + (nx - 1) as f64 * pitch, min_z + (nz - 1) as f64 * pitch);
    sectors
        .iter()
        .copied()
        .filter(|&(i, j)| {
            let (x0, z0) = ((i + xoff) as f64 * 100.0, (j + zoff) as f64 * 100.0);
            x0 < min_x - 1e-3 || x0 + 100.0 > max_x + 1e-3 || z0 < min_z - 1e-3 || z0 + 100.0 > max_z + 1e-3
        })
        .collect()
}

/// `dice_detailed_sectors(pr, lv)`: the sectors DICE's path data builds at pitch 0.2, as (grid x, grid z) ids,
/// sorted; empty without DICE path data.
pub fn dice_detailed_sectors(r: &Root, lv: &str) -> Res<Vec<(i64, i64)>> {
    let Some(p) = r.dice_pathdata(lv)? else {
        return Ok(Vec::new());
    };
    if !os::isfile(&p) {
        return Ok(Vec::new());
    }
    let data = os::read(&p)?;
    let f = bc2_formats::hpd::parse(&data).map_err(|_| Fail::Handoff)?;
    let mut out = Vec::new();
    for s in &f.sectors {
        let m = bc2_formats::kymesh::parse(s.mesh).map_err(|_| Fail::Handoff)?;
        if (f64::from(m.pitch) - 0.2).abs() < 1e-3 {
            out.push((i64::from(s.id >> 8), i64::from(s.id & 0xff)));
        }
    }
    out.sort_unstable();
    Ok(out)
}

/// `navkit.DICE_DETAILED`: DICE's own detailed sectors on a level with DICE path data (KYG-DET-1), off - round NKG2
/// measured them worse on XNAV-BLD-3's measures (NavKit's 0.8 m build reproduces DICE's ground and graph less well
/// than its 0.2 m build where NavKit picks detailed sectors DICE built at 0.8).
pub const DICE_DETAILED: bool = false;

/// `build_detailed(pr, lv, p02)`: the sectors a build makes detailed and its log line's note: the level's own choice
/// ([`detailed_sectors`]); with [`DICE_DETAILED`], on a level with DICE path data, DICE's own detailed sectors
/// (KYG-DET-1) that the pitch-0.2 run sampled (the level's own choice, inside its grid).
fn build_detailed(r: &Root, lv: &str, s: &Value, p02: &str) -> Res<(BTreeSet<(i64, i64)>, String)> {
    let own: BTreeSet<(i64, i64)> = detailed_sectors(r, lv, s)?.into_iter().collect();
    let dice = if DICE_DETAILED {
        dice_detailed_sectors(r, lv)?
    } else {
        Vec::new()
    };
    if dice.is_empty() {
        return Ok((own, String::new()));
    }
    let off = (int_of(s.get("xoff"))?, int_of(s.get("zoff"))?);
    // not sampled at 0.2: outside the run's grid, or not the level's own choice (what sample --pitch 0.2 samples)
    let mut out: BTreeSet<(i64, i64)> = uncovered(sample_grid(p02)?, &dice, off).into_iter().collect();
    out.extend(dice.iter().filter(|x| !own.contains(x)));
    let note = if out.is_empty() {
        ", DICE's".to_owned()
    } else {
        format!(
            ", DICE's ({} of its {} not sampled at 0.2 stay at 0.8)",
            out.len(),
            dice.len()
        )
    };
    Ok((dice.into_iter().filter(|x| !out.contains(x)).collect(), note))
}

/// `stale_detailed_samples(...)`: pitch-0.2 samples that do not fit the build refused.
fn stale_detailed_samples(
    lv: &str,
    p02: &str,
    hi02: Option<&str>,
    sectors: &[(i64, i64)],
    (xoff, zoff): (i64, i64),
) -> Res<()> {
    let redo = format!(
        " - sample them again (navkit.py sample {} --pitch 0.2)",
        lv.to_uppercase()
    );
    let g = sample_grid(p02)?;
    if (g.0 - 0.2).abs() > 1e-4 {
        return Err(navkit_error(format!(
            "{lv}: {} is at pitch {}, not 0.2{redo}",
            os::basename(p02)?,
            crate::stage::py_g(g.0)
        )));
    }
    if let Some(h) = hi02
        && sample_grid(h)? != g
    {
        return Err(navkit_error(format!(
            "{lv}: {} and {} are on different grids (one is from an older box){redo}",
            os::basename(h)?,
            os::basename(p02)?
        )));
    }
    let (pitch, min_x, min_z, nx, nz) = g;
    let (max_x, max_z) = (min_x + (nx - 1) as f64 * pitch, min_z + (nz - 1) as f64 * pitch);
    let out = uncovered(g, sectors, (xoff, zoff));
    if let Some(first) = out.first() {
        return Err(navkit_error(format!(
            "{lv}: {} (x {min_x:.1}..{max_x:.1}, z {min_z:.1}..{max_z:.1}) does not cover {} detailed sector(s), e.g. \
             ({}, {}) - made for another box or other sector offsets{redo}",
            os::basename(p02)?,
            out.len(),
            first.0,
            first.1
        )));
    }
    Ok(())
}

/// A JSON list of points (x, y, z), each as Python's `json.load` gives the numbers.
pub fn points(v: &Value) -> Res<Vec<(f64, f64, f64)>> {
    let l = v.as_list().ok_or(Fail::Handoff)?;
    l.iter()
        .map(|p| match p.as_list() {
            Some([x, y, z]) => Ok((
                x.as_f64().ok_or(Fail::Handoff)?,
                y.as_f64().ok_or(Fail::Handoff)?,
                z.as_f64().ok_or(Fail::Handoff)?,
            )),
            _ => Err(Fail::Handoff),
        })
        .collect()
}

fn load_json(path: &str) -> Res<Value> {
    let data = std::fs::read(path).map_err(|e| os_error(path, &e))?;
    pyjson::loads_ansi(&data).ok_or(Fail::Handoff)
}

/// `level.terrain_name(entities)` of the level's binary partition.
fn terrain_name(part: &str) -> Res<Option<String>> {
    let f = bc2_formats::dbx::read(part).map_err(|_| Fail::Handoff)?;
    for el in f.instances() {
        if f.attr(el, "type").unwrap_or("").ends_with(".TerrainEntityData") {
            let r = f.child(el, "TerrainAsset").and_then(|a| f.attr(a, "ref")).unwrap_or("");
            let parts: Vec<&str> = r.split('/').collect();
            return Ok((parts.len() >= 3).then(|| parts[1].to_lowercase()));
        }
    }
    Ok(None)
}

/// `terrain.find(folder, name)`.
fn terrain_find(folder: &str, name: &str) -> Res<Option<String>> {
    let want = format!("{name}.heightfield-000.res").to_lowercase();
    for (root, _, files) in os::walk(folder)? {
        for f in files {
            if f.to_lowercase() == want {
                return Ok(Some(join2(&root, &f)?));
            }
        }
    }
    Ok(None)
}

/// `str(list_of_str)`.
fn str_list_repr(items: &[String]) -> String {
    let parts: Vec<String> = items.iter().map(|s| os::str_repr(s)).collect();
    format!("[{}]", parts.join(", "))
}

/// `level.heightfield_of(root, lv, log)` (its terrain name returned, for `water_of`): the terrain unpacked into
/// `work\levels\<lv>\terrain` when its unpack is missing or from another bundle (stamped, under a lock), then read
/// (its error raised).
pub fn heightfield_of(root: &str, lv: &str, log: &mut dyn FnMut(&str) -> Res<()>, tool: ToolRun<'_>) -> Res<String> {
    let mut part = None;
    for b in ["pkg", "dist"] {
        for f in os::glob(&join(root, &["work", "levels", lv, b, "levels", "*.dbx"])?)? {
            if os::splitext(&os::basename(&f)?).0.to_lowercase() == lv && part.is_none() {
                part = Some(f);
            }
        }
    }
    let name = match &part {
        Some(p) => terrain_name(p)?,
        None => None,
    }
    .filter(|n| !n.is_empty())
    .unwrap_or_else(|| lv.to_owned());
    let dest = join(root, &["work", "levels", lv, "terrain"])?;
    let mut bundle = None;
    for top in [
        join(root, &["game", "Dist", "win32", "levels"])?,
        join(root, &["game", "Package", "levels"])?,
    ] {
        for d in os::glob(&join2(&top, "*")?)? {
            if bundle.is_none() && os::basename(&d)?.to_lowercase() == lv && os::isfile(&join2(&d, "terrain-00.fbrb")?)
            {
                bundle = Some(join2(&d, "terrain-00.fbrb")?);
            }
        }
    }
    let mut path = if os::isdir(&dest) {
        terrain_find(&dest, &name)?
    } else {
        None
    };
    if let Some(bundle) = bundle {
        let stamp = join2(&dest, ".source.stamp")?;
        let want = format!("{} {}", os::getsize(&bundle)?, os::mtime_ns(&bundle)?);
        let current = |path: &Option<String>| -> bool {
            path.is_some()
                && std::fs::read(&stamp).is_ok_and(|d| d.is_ascii() && os::strip(&String::from_utf8_lossy(&d)) == want)
        };
        if !current(&path) {
            let lock = join(root, &["work", "levels", lv, ".terrain.lock"])?;
            os::makedirs(&os::dirname(&lock)?)?;
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lock)
                .map_err(|e| os_error(&lock, &e))?;
            while f.try_lock().is_err() {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            let r = (|| -> Res<()> {
                path = if os::isdir(&dest) {
                    terrain_find(&dest, &name)?
                } else {
                    None
                };
                if !current(&path) {
                    log(&format!("{lv}: unpacking {}", os::relpath(&bundle, root)?))?;
                    if os::isdir(&dest) {
                        std::fs::remove_dir_all(&dest).map_err(|e| os_error(&dest, &e))?;
                    }
                    let args = vec!["unpack".to_owned(), bundle.clone(), dest.clone()];
                    let code = tool(&args);
                    if code != 0 {
                        return Err(raise(
                            "CalledProcessError",
                            format!(
                                "Command '{}' returned non-zero exit status {code}.",
                                str_list_repr(&args)
                            ),
                        ));
                    }
                    os::atomic_write(&stamp, want.replace('\n', "\r\n").as_bytes())?;
                    path = terrain_find(&dest, &name)?;
                }
                Ok(())
            })();
            let _ = f.unlock();
            r?;
        }
    }
    if let Some(p) = path {
        let data = os::read(&p)?;
        if let Err(e) = bc2_formats::terrain::parse(&data) {
            return Err(raise("ValueError", format!("{p}: {e}")));
        }
    }
    Ok(name)
}

/// The level's terrain height field (`heightfield_of`'s unpack), or `None` (no terrain).
pub fn level_terrain(root: &str, lv: &str, tool: ToolRun<'_>) -> Res<Option<bc2_formats::terrain::Heightfield>> {
    let name = heightfield_of(root, lv, &mut |_| Ok(()), tool)?;
    let dest = join(root, &["work", "levels", lv, "terrain"])?;
    if !os::isdir(&dest) {
        return Ok(None);
    }
    let Some(p) = terrain_find(&dest, &name)? else {
        return Ok(None);
    };
    let data = os::read(&p)?;
    bc2_formats::terrain::parse(&data)
        .map(Some)
        .map_err(|e| raise("ValueError", format!("{p}: {e}")))
}

/// `level.water_of(root, lv)` as `navregress.level_water` takes it: (vertices, triangles), or `None` (no water).
fn level_water(root: &str, lv: &str, tool: ToolRun<'_>) -> Res<Option<WaterMesh>> {
    let name = heightfield_of(root, lv, &mut |_| Ok(()), tool)?.to_lowercase();
    let dest = join(root, &["work", "levels", lv, "terrain"])?;
    let mut mesh = None;
    if os::isdir(&dest) {
        for (root, _, files) in os::walk(&dest)? {
            for f in files {
                if f.to_lowercase() == format!("{name}.water.mesh.res") {
                    mesh = Some(join2(&root, &f)?);
                }
            }
        }
    }
    let Some(mesh) = mesh else { return Ok(None) };
    let data = os::read(&mesh)?;
    let w = bc2_formats::water::parse_mesh(&data).map_err(|e| raise("ValueError", format!("{mesh}: {e}")))?;
    let tris: Vec<[i64; 3]> = w.triangles.iter().map(|t| t.map(i64::from)).collect();
    if tris.is_empty() {
        return Ok(None);
    }
    let verts = w.vertices.iter().map(|v| v.map(f64::from)).collect();
    Ok(Some((verts, tris)))
}

/// The area outlines' points of every sector of a path-data file (and, `portals`, the portals' ends).
pub fn outline_points(path: &str, portals: bool) -> Res<Vec<[f64; 3]>> {
    let data = os::read(path)?;
    let f = bc2_formats::hpd::parse(&data).map_err(|_| Fail::Handoff)?;
    let mut pts = Vec::new();
    for s in &f.sectors {
        let m = bc2_formats::kymesh::parse(s.mesh).map_err(|_| Fail::Handoff)?;
        for a in &m.areas {
            for line in &a.outline {
                pts.extend(line.iter().map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]));
            }
            if portals {
                for pt in &a.portals {
                    for p in [pt.p0, pt.p1] {
                        pts.push([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
                    }
                }
            }
        }
    }
    Ok(pts)
}

/// `navregress.reference_bounds(root, lv)`: DICE's extent (min x, min z, max x, max z), or `None`.
pub fn reference_bounds(r: &Root, lv: &str) -> Res<Option<[f64; 4]>> {
    let Some(p) = r.dice_pathdata(lv)? else {
        return Ok(None);
    };
    if !os::isfile(&p) {
        return Ok(None);
    }
    let pts = outline_points(&p, true)?;
    if pts.is_empty() {
        return Ok(None);
    }
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for q in &pts {
        if q[0].is_nan() || q[2].is_nan() {
            return Err(Fail::Handoff);
        }
        b[0] = b[0].min(q[0]);
        b[1] = b[1].min(q[2]);
        b[2] = b[2].max(q[0]);
        b[3] = b[3].max(q[2]);
    }
    Ok(Some(b))
}

/// `navregress.SPAWN_REACH`: KYG-RECT-1's rectangle, the spawns' box grown by 25 m less the radius 0.4 and 0.1 m.
pub const SPAWN_REACH: f64 = 24.5;

/// The sorted entries of a folder (none when it does not read).
fn sorted_dir(d: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        std::fs::read_dir(d).map_or_else(|_| Vec::new(), |r| r.flatten().map(|e| e.path()).collect());
    v.sort();
    v
}

/// `navregress.spawn_bounds(root, lv)`: DICE's generator rectangle (KYG-RECT-1, verified on the 13 campaign levels):
/// the box of the level's soldier spawns (alternate spawns left out: round NKS-deep D) and vehicle spawns grown by
/// [`SPAWN_REACH`], as
/// (min x, min z, max x, max z); `None` without a spawn or a partition that loads (the level read as
/// `level.load(geometry=False)` reads it: Package, Dist, then NavKit's shared data).
pub fn spawn_bounds(r: &Root, lv: &str) -> Res<Option<[f64; 4]>> {
    let base = PathBuf::from(r.level_work(lv)?);
    let mut bases = vec![base.join("pkg"), base.join("dist")];
    bases.extend(
        sorted_dir(&PathBuf::from(join(&r.work, &["navkit", "common"])?))
            .into_iter()
            .filter(|p| p.is_dir()),
    );
    let want = format!("{lv}.dbx");
    let Some(partition) = bases
        .iter()
        .take(2)
        .flat_map(|b| sorted_dir(&b.join("levels")))
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase() == want)
        })
    else {
        return Ok(None);
    };
    let name = partition
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let Ok(sc) = bc2_scene::scene::load(&bc2_scene::scene::LoadOptions::new(name, partition, bases)) else {
        return Ok(None);
    };
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for e in &sc.entities {
        if (e.category == "soldier spawn" || e.category == "vehicle spawn")
            && e.short_type != ALTERNATE_SPAWN
            && let Some(p) = e.position()
        {
            b = [b[0].min(p[0]), b[1].min(p[2]), b[2].max(p[0]), b[3].max(p[2])];
        }
    }
    Ok((b[0] <= b[2]).then(|| {
        [
            b[0] - SPAWN_REACH,
            b[1] - SPAWN_REACH,
            b[2] + SPAWN_REACH,
            b[3] + SPAWN_REACH,
        ]
    }))
}

/// The type of the spawns DICE's rectangle leaves out (KYG-RECT-1, round NKS-deep D): the player's alternate spawn
/// positions (`GameSharedResources.AlternateSpawnEntityData`).
pub const ALTERNATE_SPAWN: &str = "AlternateSpawnEntityData";

/// `navregress.RECT_ON_DICE_LEVELS`: a campaign level with DICE path data is bounded by the spawns' rectangle
/// ([`spawn_bounds`]) instead of DICE's extent (the Onslaught levels, `ons_*`, keep DICE's: KYG-RECT-1's exception).
pub const RECT_ON_DICE_LEVELS: bool = false;

/// `navregress.build_bounds(root, lv)`: the extent a build keeps - DICE's ([`reference_bounds`]) on a level with DICE
/// path data (the spawns' rectangle instead on a campaign level with [`RECT_ON_DICE_LEVELS`]), else the spawns'
/// rectangle ([`spawn_bounds`]), else `None`.
pub fn build_bounds(r: &Root, lv: &str) -> Res<Option<[f64; 4]>> {
    match reference_bounds(r, lv)? {
        Some(b) if !(RECT_ON_DICE_LEVELS && !lv.to_lowercase().starts_with("ons_")) => Ok(Some(b)),
        Some(b) => Ok(spawn_bounds(r, lv)?.or(Some(b))),
        None => spawn_bounds(r, lv),
    }
}

/// `navregress.area_orders_of` for a level prepared before `area_orders.json` existed: `levelobjects.area_orders` of
/// the converted level partition (`work\levels\<lv>\dist\levels\<lv>.xml`, any case): the Transform translation of
/// every `.AreaOrderEntityData` away from the origin; `None` where the Python's `except Exception` gives `None` (no
/// partition, no folder).
fn area_orders_xml(l: &str, lv: &str) -> Res<Option<Vec<(f64, f64, f64)>>> {
    const OPEN: &str = "<complex name=\"Transform\">";
    let d = join(l, &["dist", "levels"])?;
    let Ok(names) = os::listdir(&d) else {
        return Ok(None);
    };
    let Some(name) = names.iter().find(|f| f.to_lowercase() == format!("{lv}.xml")) else {
        return Ok(None);
    };
    let Ok(data) = std::fs::read(join2(&d, name)?) else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&data);
    let found = bc2_formats::xmlinst::instances(&text, Some(".AreaOrderEntityData")).ok_or(Fail::Handoff)?;
    let mut pts = Vec::new();
    for (_t, _g, body) in found {
        // re.search(r'<complex name="Transform">([^<]*)</complex>', body): the first such element
        let mut from = 0;
        let mut vals = None;
        while let Some(p) = body.get(from..).and_then(|r| r.find(OPEN)) {
            let s = from + p + OPEN.len();
            let rest = body.get(s..).unwrap_or("");
            let end = rest.find('<').unwrap_or(rest.len());
            if rest.get(end..).is_some_and(|r| r.starts_with("</complex>")) {
                vals = Some(&rest[..end]);
                break;
            }
            from = from + p + 1;
        }
        let Some(vals) = vals else { continue };
        let v = vals
            .split('/')
            .map(|a| {
                let t = a.trim();
                // float(): plain decimal text only; anything else (a Python-only spelling) is the Python's
                if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b)) {
                    return Err(Fail::Handoff);
                }
                t.parse::<f64>().map_err(|_| Fail::Handoff)
            })
            .collect::<Res<Vec<f64>>>()?;
        if v.len() >= 15 && v[12..15].iter().any(|c| c.abs() > 1e-3) {
            pts.push((v[12], v[13], v[14]));
        }
    }
    Ok(Some(pts))
}

/// What `build_inputs` gives: the inputs, the lines it logs (each printed after `== build`).
#[derive(Debug, Clone)]
pub struct Gathered {
    /// The build's inputs.
    pub inputs: Inputs,
    /// Its lines.
    pub lines: Vec<String>,
}

/// `build_inputs(pr, lv, log)`: the build's inputs for a prepared level (the same for build and apply).
pub fn build_inputs(r: &Root, lv: &str, tool: ToolRun<'_>) -> Res<Gathered> {
    let s = r.settings(lv)?;
    let l = r.level_work(lv)?;
    let (xoff, zoff) = (int_of(s.get("xoff"))?, int_of(s.get("zoff"))?);
    let Some(s08) = r
        .samples08(lv)?
        .filter(|p| os::isfile(&format!("{}_hi.bin", &p[..p.len() - 4])))
    else {
        return Err(navkit_error(format!(
            "{lv}: samples missing ({} and its _hi run) - run sample first",
            os::basename(&r.samples(lv, "")?)?
        )));
    };
    let mut lines = Vec::new();
    let mut inp = Inputs::new(PathBuf::from(&s08), xoff, zoff);
    let p02 = r.samples(lv, "_p02")?;
    if os::isfile(&p02) {
        let hi02 = r.samples(lv, "_hi_p02")?;
        let high = os::isfile(&hi02).then_some(hi02);
        let (set, note) = build_detailed(r, lv, &s, &p02)?;
        let sorted: Vec<(i64, i64)> = set.iter().copied().collect();
        stale_detailed_samples(lv, &p02, high.as_deref(), &sorted, (xoff, zoff))?;
        lines.push(format!(
            "  detailed zones: {} sectors at pitch 0.2{note}{}",
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
    let npy = |name: &str| -> Res<Option<crate::walk::Tris>> {
        let p = join2(&l, name)?;
        if !os::isfile(&p) {
            return Err(raise(
                "FileNotFoundError",
                format!("[Errno 2] No such file or directory: {}", os::str_repr(&p)),
            ));
        }
        build::read_tris_npy(Path::new(&p)).map_err(|_| Fail::Handoff)
    };
    inp.high = Some(PathBuf::from(format!("{}_hi.bin", &s08[..s08.len() - 4])));
    inp.seeds = points(&load_json(&join2(&l, "soldier_spawns.json")?)?)?;
    inp.water = level_water(&r.root, lv, tool)?;
    inp.terrain = level_terrain(&r.root, lv, tool)?.map(std::sync::Arc::new);
    inp.bounds = build_bounds(r, lv)?;
    let op = join2(&l, "order_points.json")?;
    inp.order_points = if os::isfile(&op) {
        Some(points(&load_json(&op)?)?)
    } else {
        None
    };
    let ao = join2(&l, "area_orders.json")?;
    inp.area_orders = if os::isfile(&ao) {
        Some(points(&load_json(&ao)?)?)
    } else {
        area_orders_xml(&l, lv)?
    };
    let lp = join2(&l, "ladders.json")?;
    if os::isfile(&lp) {
        let v = load_json(&lp)?;
        let list = v.as_list().ok_or(Fail::Handoff)?;
        let mut ladders = Vec::new();
        for lad in list {
            let p3 = |k: &str| -> Res<[f64; 3]> {
                match lad.get(k).and_then(Value::as_list) {
                    Some([a, b, c]) => Ok([
                        a.as_f64().ok_or(Fail::Handoff)?,
                        b.as_f64().ok_or(Fail::Handoff)?,
                        c.as_f64().ok_or(Fail::Handoff)?,
                    ]),
                    _ => Err(Fail::Handoff),
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

/// The build's settings (`--no-detail`: no D lines or F cells).
#[must_use]
pub fn settings(no_detail: bool) -> BuildSettings {
    BuildSettings {
        detail_features: !no_detail,
        ..BuildSettings::default()
    }
}

/// `time.time()`.
fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// The build's log: each line with two spaces before it, to the sink (and kept, for apply's report).
struct Lines<'a> {
    out: &'a mut dyn Out,
    t0: f64,
    kept: Option<&'a mut Vec<String>>,
    err: Option<Fail>,
}

impl crate::stage::Log for Lines<'_> {
    fn line(&mut self, text: &str) {
        let msg = format!("  {text}");
        if self.err.is_none()
            && let Err(e) = self.out.line(&msg)
        {
            self.err = Some(e);
        }
        if let Some(k) = self.kept.as_deref_mut() {
            k.push(msg);
        }
    }

    fn seconds(&self) -> f64 {
        now_seconds() - self.t0
    }

    fn cancelled(&self) -> bool {
        self.out.cancelled()
    }
}

/// How a build is run: the sector clock words (`time.localtime()` of the build: [`build::clock_words`]) and
/// `--no-detail`.
#[derive(Debug, Clone, Copy)]
pub struct Run {
    /// The sectors' build date and clock words.
    pub clock: (u32, u32),
    /// No D lines or F cells.
    pub no_detail: bool,
}

/// `navbuild.build(...)` with the level's inputs, its lines to `out` (two spaces before each).
pub fn run_build(inp: &Inputs, path: &str, run: Run, out: &mut dyn Out, kept: Option<&mut Vec<String>>) -> Res<Report> {
    let t0 = now_seconds();
    let mut lg = Lines {
        out,
        t0,
        kept,
        err: None,
    };
    let got = build::build(
        inp,
        Path::new(path),
        &settings(run.no_detail),
        &Switches::default(),
        &WalkSettings::navkit(40.0),
        run.clock,
        &mut lg,
    );
    if let Some(e) = lg.err {
        return Err(e);
    }
    got.map_err(|e| match e {
        crate::NavError::Cancelled => Fail::Cancelled,
        e => raise("RuntimeError", e.to_string()),
    })
}

/// `navkit.build(pr, lv, cfg=cfg)`: the level built into `work\navkit\<lv>\pathdata.hpd`.
pub fn build_level(r: &Root, lv: &str, run: Run, out: &mut dyn Out, tool: ToolRun<'_>) -> Res<()> {
    let path = join2(&r.out_dir(lv)?, "pathdata.hpd")?;
    let t0 = Instant::now();
    let gathered = build_inputs(r, lv, tool);
    if matches!(gathered, Err(Fail::Handoff)) {
        return Err(Fail::Handoff);
    }
    out.line(&format!("== build {lv} -> {}", os::relpath(&path, &r.root)?))?;
    let g = gathered?;
    for l in &g.lines {
        out.line(l)?;
    }
    let trav = join2(&os::dirname(&path)?, &format!("{lv}.traversal.json"))?;
    if os::isfile(&trav) {
        std::fs::remove_file(&trav).map_err(|e| os_error(&trav, &e))?;
    }
    run_build(&g.inputs, &path, run, out, None)?;
    let size = os::getsize(&path)? as f64 / 1e6;
    out.line(&format!(
        "  built in {:.0} s ({size:.1} MB)",
        t0.elapsed().as_secs_f64()
    ))
}

/// A side file's walkability operations as the build takes them (`navedits.regions_of`).
pub fn edits_of(ops: &[Value]) -> Res<Edits> {
    let mut e = Edits::default();
    for o in ops {
        let Some(name) = o.get("op").and_then(Value::as_str) else {
            continue;
        };
        if !matches!(name, "walkable" | "unwalkable" | "seed" | "unseed") {
            continue;
        }
        match e.counts.iter_mut().find(|(k, _)| k == name) {
            Some(c) => c.1 += 1,
            None => e.counts.push((name.to_owned(), 1)),
        }
        let at = || -> Res<(f64, f64, f64)> {
            match o.get("at").and_then(Value::as_list) {
                Some([x, y, z]) => Ok((
                    x.as_f64().ok_or(Fail::Handoff)?,
                    y.as_f64().ok_or(Fail::Handoff)?,
                    z.as_f64().ok_or(Fail::Handoff)?,
                )),
                _ => Err(Fail::Handoff),
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
fn region_of(r: Option<&Value>) -> Res<Region> {
    let r = r.ok_or(Fail::Handoff)?;
    let f = |v: &Value| v.as_f64().ok_or(Fail::Handoff);
    if let Some(b) = r.get("box") {
        let Some([a, b2, c, d, e, g]) = b.as_list() else {
            return Err(Fail::Handoff);
        };
        return Ok(Region::Box([f(a)?, f(b2)?, f(c)?, f(d)?, f(e)?, f(g)?]));
    }
    let Some([lo, hi]) = r.get("y").and_then(Value::as_list) else {
        return Err(Fail::Handoff);
    };
    let poly = r.get("polygon").and_then(Value::as_list).ok_or(Fail::Handoff)?;
    if poly.is_empty() {
        return Err(Fail::Handoff);
    }
    let poly = poly
        .iter()
        .map(|p| match p.as_list() {
            Some([a, b]) => Ok((f(a)?, f(b)?)),
            _ => Err(Fail::Handoff),
        })
        .collect::<Res<Vec<_>>>()?;
    Ok(Region::Polygon {
        y: (f(lo)?, f(hi)?),
        poly,
    })
}

/// The sectors' build date and clock words of now in the local time zone (`time.localtime()`: Windows'
/// `GetLocalTime`, what the C runtime's `localtime` gives; elsewhere UTC).
#[must_use]
pub fn local_clock() -> (u32, u32) {
    let (y, mo, d, h, mi, s) = local_now();
    build::clock_words(y, mo, d, h, mi, s)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn local_now() -> (u32, u32, u32, u32, u32, u32) {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut t = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: GetLocalTime only writes the SYSTEMTIME it is given, which lives for the call.
    unsafe { GetLocalTime(&raw mut t) };
    (
        u32::from(t.wYear),
        u32::from(t.wMonth),
        u32::from(t.wDay),
        u32::from(t.wHour),
        u32::from(t.wMinute),
        u32::from(t.wSecond),
    )
}

#[cfg(not(windows))]
fn local_now() -> (u32, u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = (secs / 86_400) as i64;
    let rem = (secs % 86_400) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y as u32, m as u32, d as u32, rem / 3600, rem % 3600 / 60, rem % 60)
}
