//! `stockmp.py summary` and `generate` (round O3; the full layer round O5): the level's stock bundle unpacked into the
//! work folder as `pipeline.unpacked` caches it (FbrbTool's unpack in `bc2_formats`, its `.source.sha256` stamp), the
//! level partition read into `bc2_stockmp::model::Model` (`levelmodel.model_of`), `mp_common`'s kits
//! (`pipeline.kit_refs`), the layer placed on the path data (`generate.rs`) and the configuration written as
//! `config.save` writes it (`json.dump(indent=1)`).
//!
//! Handed to the Python: a model the Rust core refuses (the Python's `levelmodel` reads it), text that is not UTF-8
//! or not ASCII where Python's `lower()` or path rules would decide, a capture point without an owner or position (the
//! reference raises its `TypeError` while printing), path data the Rust reader refuses or that has no route nodes
//! (NavKit's `ordercheck.PathData`, which `config.generate` loads first, raises its own error).

use std::fs;

use bc2_formats::dbx::edit::EditDbx;
use bc2_formats::{fbrb, hpd};
use bc2_pyrt::json::{self as pyjson, Value};
use bc2_stockmp::install::{self, APPLIED, BACKUP, isfile, join};
use bc2_stockmp::model::{Model, Pos};
use bc2_stockmp::sha256;

use super::build::TEMPLATE_PARTS;
use super::generate::{self, KIT_NAMES, Snapper};
use super::{Paths, opt, system_exit};
use crate::pyfmt::str_repr;
use crate::pyproc::{Fail, Py, PyErr, PyResult, os_error};

/// `pipeline.BuildError(message)`.
pub(super) fn build_error(msg: impl Into<String>) -> Fail {
    Fail::Err(PyErr::new("BuildError", msg))
}

pub(super) fn sha(path: &str) -> PyResult<String> {
    sha256::file(path).map_err(|e| os_error(path, &e))
}

/// `pipeline.stock_file(p)`: the game file as before stock-MP's install (its backup; None where the install added
/// it or it is missing).
pub fn stock_file(p: &str) -> PyResult<Option<String>> {
    let backup = format!("{p}{BACKUP}");
    if isfile(&backup) {
        return Ok(Some(backup));
    }
    if !isfile(p) {
        return Ok(None);
    }
    let marker = format!("{p}{APPLIED}");
    if isfile(&marker) && install::marker_matches(&marker, p).map_err(super::fault)? {
        return Ok(None);
    }
    Ok(Some(p.to_owned()))
}

/// `pipeline.unpacked(bundle, d)`: the cached unpack (unpacked again when the bundle's hash changed).
pub(super) fn unpacked(bundle: &str, d: &str) -> PyResult<String> {
    let stamp = join(d, ".source.sha256");
    let h = sha(bundle)?;
    if isfile(&join(d, "fbrb.manifest")) && isfile(&stamp) {
        let got = fs::read(&stamp).map_err(|e| os_error(&stamp, &e))?;
        // open(stamp).read().strip(): a stamp that is not ASCII is read by the locale's code page
        let Ok(text) = std::str::from_utf8(&got) else {
            return Err(Fail::Python);
        };
        if !text.is_ascii() {
            return Err(Fail::Python);
        }
        if install::strip(text) == h {
            return Ok(d.to_owned());
        }
    }
    let _ = fs::remove_dir_all(d);
    if let Err(e) = fbrb::unpack_path(bundle, d) {
        return Err(build_error(format!("failed: unpack {bundle} -> {d}: {e}")));
    }
    fs::write(&stamp, h.as_bytes()).map_err(|e| os_error(&stamp, &e))?;
    Ok(d.to_owned())
}

/// fbrb.manifest's lines (`open(..., encoding='utf-8').read().splitlines()`); ASCII only (Python's `lower()`).
pub(super) fn manifest_lines(d: &str) -> PyResult<Vec<String>> {
    let p = join(d, "fbrb.manifest");
    let b = fs::read(&p).map_err(|e| os_error(&p, &e))?;
    let Ok(t) = String::from_utf8(b) else {
        return Err(Fail::Python);
    };
    if !t.is_ascii() {
        return Err(Fail::Python);
    }
    Ok(bc2_pyrt::py::splitlines(&t).into_iter().map(str::to_owned).collect())
}

/// `pipeline._level_name(d, lv)`: the level's name as the bundle spells it (`Levels/<NAME>.dbx`).
pub(super) fn level_name(d: &str, lv: &str) -> PyResult<String> {
    for line in manifest_lines(d)? {
        let name = line.split('|').next().unwrap_or("").replace('\\', "/");
        let parts: Vec<&str> = name.split('/').collect();
        if let [a, b] = parts.as_slice()
            && a.eq_ignore_ascii_case("levels")
            && b.eq_ignore_ascii_case(&format!("{lv}.dbx"))
            && !line.contains("|*deleted*|")
        {
            return Ok(b.get(..b.len() - 4).unwrap_or("").to_owned());
        }
    }
    Err(build_error(format!("no Levels/{lv}.dbx in {d}")))
}

/// `pipeline.level_entry(d, lv_upper)`: the level partition's entry name, or None.
pub(super) fn level_entry(d: &str, upper: &str) -> PyResult<Option<String>> {
    for line in manifest_lines(d)? {
        let name = line.split('|').next().unwrap_or("").to_owned();
        let fixed = name.replace('\\', "/");
        let parts: Vec<&str> = fixed.split('/').collect();
        if let [a, b] = parts.as_slice()
            && a.eq_ignore_ascii_case("levels")
            && b.eq_ignore_ascii_case(&format!("{upper}.dbx"))
            && !line.contains("|*deleted*|")
        {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

/// `pipeline.find_entry(folder, name)`.
pub(super) fn find_entry(folder: &str, name: &str) -> Option<String> {
    let p = name.split('/').fold(folder.to_owned(), |a, n| join(&a, n));
    isfile(&p).then_some(p)
}

/// A string of the file as Python holds it (UTF-8; else surrogate-escaped, which the port leaves to the Python).
fn text(b: &[u8]) -> PyResult<String> {
    String::from_utf8(b.to_vec()).map_err(|_| Fail::Python)
}

fn opt_repr(b: Option<&[u8]>) -> PyResult<String> {
    Ok(match b {
        Some(b) => str_repr(&text(b)?),
        None => "None".to_owned(),
    })
}

fn opt_str(b: Option<&[u8]>) -> PyResult<String> {
    Ok(match b {
        Some(b) => text(b)?,
        None => "None".to_owned(),
    })
}

/// `tuple(round(x) for x in p)`: Python's `round` of each (half to even) as an int.
#[allow(clippy::many_single_char_names, reason = "the reference's x, y, z")]
fn rounded(p: Option<Pos>) -> PyResult<String> {
    let Some((x, y, z)) = p else { return Err(Fail::Python) };
    let mut out = Vec::new();
    for v in [x, y, z] {
        let r = v.round_ties_even();
        if !r.is_finite() || r.abs() >= 1e18 {
            return Err(Fail::Python);
        }
        out.push(format!("{}", r as i64));
    }
    Ok(format!("({})", out.join(", ")))
}

/// The level's stock bundle unpacked, its level partition found and read: (the unpack folder, the partition's bytes,
/// the model, the parsed partition). As `summary` / `pipeline.generate` reach it.
fn read_level(
    paths: &Paths,
    kind: &str,
    none_error: impl FnOnce() -> Fail,
) -> PyResult<(Vec<u8>, EditDbx, Model)> {
    let bundles = paths.bundles()?;
    let Some((v, b)) = bundles.last() else {
        return Err(none_error());
    };
    let d = unpacked(b, &paths.src(v))?;
    let upper = level_name(&d, &paths.lv)?;
    let Some(entry) = level_entry(&d, &upper)? else {
        return Err(Fail::Err(PyErr::new(
            kind,
            format!("{}: no level partition in {d}", paths.lv),
        )));
    };
    let Some(lvl) = find_entry(&d, &entry) else {
        return Err(Fail::Err(PyErr::new(
            kind,
            format!("{}: the level partition {entry} is not in {d}", paths.lv),
        )));
    };
    let data = fs::read(&lvl).map_err(|e| os_error(&lvl, &e))?;
    // levelmodel.model_of: the Rust core's reading, else the Python's (handed off)
    let dbx = EditDbx::parse(&data).map_err(|_| Fail::Python)?;
    let model = Model::new(&dbx).map_err(|_| Fail::Python)?;
    Ok((data, dbx, model))
}

/// A `SystemExit(message)` the steps above raise: printed by the interpreter, exit code 1.
pub(super) fn exits(py: &mut Py, r: PyResult<i32>) -> PyResult<i32> {
    match r {
        Err(Fail::Err(e)) if e.kind == "SystemExit" => Ok(system_exit(py, &e.msg)),
        r => r,
    }
}

/// `stockmp.summary(paths)`.
pub fn summary(py: &mut Py, paths: &Paths) -> PyResult<i32> {
    let r = summary_inner(py, paths);
    exits(py, r)
}

fn summary_inner(py: &mut Py, paths: &Paths) -> PyResult<i32> {
    // paths.bundles()[-1] of none: IndexError
    let (_data, dbx, m) = read_level(paths, "SystemExit", || {
        Fail::Err(PyErr::new("IndexError", "list index out of range"))
    })?;
    let stages = m.rush_stages(&dbx).map_err(|_| Fail::Python)?;
    // everything the prints use, checked before the first print (a refusal hands off with nothing printed)
    let mut lines = Vec::new();
    let mut factions = Vec::new();
    for (t, f) in &m.factions {
        factions.push(format!("{}: {}", str_repr(&text(t)?), opt_repr(f.as_deref())?));
    }
    lines.push(format!(
        "{}: mode {}, factions {{{}}}, attacker {}, HasAI {}",
        opt_str(m.name.as_deref())?,
        text(&m.mode)?,
        factions.join(", "),
        opt_str(m.attacker.as_deref())?,
        if m.has_ai { "True" } else { "False" }
    ));
    for cp in &m.capture_points {
        let label = if cp.label.is_empty() { &cp.name } else { &cp.label };
        // `{cp.owner:11s}` of None raises (the reference's TypeError)
        let Some(owner) = &cp.owner else {
            return Err(Fail::Python);
        };
        lines.push(format!(
            "  capture point {:14} owner {:11} at {}, {} spawners",
            text(label)?,
            text(owner)?,
            rounded(cp.pos)?,
            cp.spawners.len()
        ));
    }
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for s in &m.spawners {
        let k = format!(
            "({}, {})",
            opt_repr(s.team.as_deref())?,
            opt_repr(s.player_type.as_deref())?
        );
        match kinds.iter_mut().find(|(x, _)| *x == k) {
            Some(e) => e.1 += 1,
            None => kinds.push((k, 1)),
        }
    }
    let kinds: Vec<String> = kinds.iter().map(|(k, n)| format!("{k}: {n}")).collect();
    lines.push(format!("  soldier spawners: {{{}}}", kinds.join(", ")));
    for st in &stages {
        let mut mc = Vec::new();
        for (_g, p) in &st.mcoms {
            mc.push(rounded(*p)?);
        }
        lines.push(format!(
            "  stage {}: {}, M-COMs [{}]",
            st.index,
            text(&st.trigger_name)?,
            mc.join(", ")
        ));
    }
    for l in &lines {
        py.print(l)?;
    }
    Ok(0)
}

// ------------------------------------------------------------------------------------------------ generate

/// `stockmp.SAME_WORLD`.
const SAME_WORLD: [(&str, &str); 10] = [
    ("mp_005", "ons_mp_005gr"),
    ("mp_005gr", "ons_mp_005gr"),
    ("mp_005sr", "ons_mp_005gr"),
    ("mp_002", "ons_mp_002"),
    ("mp_002sr", "ons_mp_002"),
    ("mp_004", "ons_mp_004"),
    ("mp_004sdm", "ons_mp_004"),
    ("mp_008", "ons_mp_008"),
    ("mp_008cq", "ons_mp_008"),
    ("mp_008sdm", "ons_mp_008"),
];

fn same_world(lv: &str) -> Option<&'static str> {
    SAME_WORLD.iter().find(|(a, _)| *a == lv).map(|(_, b)| *b)
}

/// `stockmp.pathdata_for(paths, folder)`: (infantry, vehicle, source), or its `SystemExit`.
fn pathdata_for(paths: &Paths, folder: Option<&str>) -> PyResult<(String, String, String)> {
    let base = join(&join(&join(&join(&paths.game, "Output"), "win32"), "ai"), "levels");
    let cands: Vec<Option<&str>> = match folder {
        Some(f) => vec![Some(f)],
        None => vec![Some(paths.lv.as_str()), same_world(&paths.lv)],
    };
    for f in cands.into_iter().flatten() {
        // os.path.join with an absolute or drive path, or an empty part: the Python's
        if f.is_empty() || f.contains(':') || f.starts_with(['\\', '/']) {
            return Err(Fail::Python);
        }
        let inf = stock_file(&join(&join(&join(&base, f), "infantry"), "pathdata.hpd"))?;
        if let Some(inf) = inf {
            let veh = join(&join(&join(&base, f), "vehicle"), "pathdata.hpd");
            let src = if f == paths.lv {
                "the level's own path data".to_owned()
            } else {
                format!("path data of {f} (the same world)")
            };
            let veh = stock_file(&veh)?.unwrap_or(veh);
            return Ok((inf, veh, src));
        }
    }
    Err(Fail::Err(PyErr::new(
        "SystemExit",
        format!(
            "{}: no path data found (looked in {base} for {} / {}); build one with NavKit and pass --pathdata",
            paths.lv,
            folder.unwrap_or(&paths.lv),
            same_world(&paths.lv).unwrap_or("None")
        ),
    )))
}

/// `int(text)` of the plain forms (an optional sign and ASCII digits); None where Python's `int` decides.
fn plain_int(t: &str) -> Option<i64> {
    let d = t.strip_prefix(['-', '+']).unwrap_or(t);
    if d.is_empty() || !d.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

/// `str.lower()` of ASCII text (anything else: the Python's).
pub(super) fn py_lower(t: &str) -> PyResult<String> {
    if t.is_ascii() {
        Ok(t.to_ascii_lowercase())
    } else {
        Err(Fail::Python)
    }
}

/// `stockmp.main`'s generate: `pipeline.generate` (the configuration written, its problems raised) and the lines
/// printed after it. A `SystemExit` comes back as the error the caller turns into its exit (`exits`).
#[allow(
    clippy::too_many_lines,
    reason = "pipeline.generate and config.generate, in order"
)]
pub fn generate(py: &mut Py, paths: &Paths, args: &[String]) -> PyResult<()> {
    let pd = pathdata_for(paths, opt(args, "--pathdata"))?;
    let minimal = args.iter().any(|a| a == "--minimal");
    let keep = minimal || args.iter().any(|a| a == "--keep-stock-spawners");
    let bots = match opt(args, "--bots") {
        Some(b) => plain_int(b).ok_or(Fail::Python)?,
        None => 8,
    };
    // pipeline.generate
    let config = paths.config();
    let (_data, dbx, m) = read_level(paths, "BuildError", || {
        build_error(format!("{}: no level-00 bundle in {}", paths.lv, paths.game))
    })?;
    let kits = kit_refs(paths)?;
    // config.generate: Snapper(pathdata['infantry']) loads NavKit's path data first
    let snap = if minimal {
        snapper_loads(&pd.0)?;
        None
    } else {
        Some(Snapper::new(&pd.0)?)
    };
    let mut teams = Vec::new();
    for t in ["Team1", "Team2"] {
        let faction = m
            .factions
            .iter()
            .find(|(x, _)| x.as_slice() == t.as_bytes())
            .and_then(|(_, f)| f.clone());
        let faction = match faction {
            Some(f) => Some(text(&f)?),
            None => None,
        };
        let prefix = match faction.as_deref() {
            Some("FactionUS") => "us",
            Some("FactionRUS") => "ru",
            other => {
                let shown = other.map_or_else(|| "None".to_owned(), str::to_owned);
                return Err(Fail::Err(PyErr::new(
                    "ValueError",
                    format!("no multiplayer kits known for {shown}"),
                )));
            }
        };
        let k: Vec<(String, Value)> = KIT_NAMES
            .iter()
            .map(|n| {
                (
                    (*n).to_owned(),
                    Value::Str(
                        kits.iter()
                            .find(|(x, _)| *x == format!("{prefix}/{n}"))
                            .map_or_else(String::new, |(_, r)| r.clone()),
                    ),
                )
            })
            .collect();
        teams.push((
            t.to_owned(),
            Value::dict([
                ("faction", faction.map_or(Value::Null, Value::Str)),
                ("kits", Value::Dict(k)),
            ]),
        ));
    }
    let (spawners, orders, stages, notes) = match &snap {
        None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        Some(snap) => generate::layer(&m, &dbx, snap, &generate::Layer { bots })?,
    };
    let name = match &m.name {
        Some(n) => Value::Str(text(n)?),
        None => Value::Null,
    };
    let s = |x: &str| Value::Str(x.to_owned());
    let counts = (spawners.len(), orders.len(), stages.len(), notes.len());
    let c = Value::dict([
        ("format", s("bfbc2-stockmp-ai")),
        ("version", Value::Int(1)),
        ("lv", s(&paths.lv)),
        ("level", name),
        ("mode", Value::Str(text(&m.mode)?)),
        ("layer", s(if minimal { "minimal" } else { "full" })),
        ("stock_spawners", s(if keep { "keep" } else { "human" })),
        (
            "pathdata",
            Value::dict([("infantry", s(&pd.0)), ("vehicle", s(&pd.1)), ("source", s(&pd.2))]),
        ),
        ("template", s(TEMPLATE)),
        (
            "template_parts",
            Value::List(TEMPLATE_PARTS.iter().map(|p| s(p)).collect()),
        ),
        ("teams", Value::Dict(teams)),
        ("defaults", defaults()),
        ("spawners", Value::List(spawners)),
        ("orders", Value::List(orders)),
        ("stages", Value::List(stages)),
        ("notes", Value::List(notes.iter().map(|n| s(n)).collect())),
    ]);
    // config.save: makedirs, json.dump(indent=1) and a newline, '\n' line ends
    let abs = bc2_pyrt::py::abspath(&config);
    let dir = bc2_pyrt::py::dirname(&abs);
    fs::create_dir_all(&dir).map_err(|e| os_error(&dir, &e))?;
    let text_out = format!("{}\n", pyjson::dumps(&c, Some(1)));
    fs::write(&config, text_out.as_bytes()).map_err(|e| os_error(&config, &e))?;
    let problems = generate::check(&c)?;
    if !problems.is_empty() {
        return Err(build_error(format!(
            "{config}: the generated configuration has problems - edit it before building: {}",
            problems.join("; ")
        )));
    }
    py.print(&format!(
        "{config}: {} AI spawners, {} orders, {} stage switches; {} note(s)",
        counts.0, counts.1, counts.2, counts.3
    ))?;
    for n in &notes {
        py.print(&format!("  note: {n}"))?;
    }
    Ok(())
}

/// `config.Snapper(hpd)`'s loading of the path data (`ordercheck.PathData`) for the minimal layer: it raises on a file
/// the reader refuses, an edge naming a node its sector lacks, or no route node at all (`max()` of nothing) - all
/// left to the Python.
fn snapper_loads(hpd_path: &str) -> PyResult<()> {
    let data = fs::read(hpd_path).map_err(|e| os_error(hpd_path, &e))?;
    let f = hpd::parse(&data).map_err(|_| Fail::Python)?;
    let mut nodes = 0usize;
    for s in &f.sectors {
        for e in &s.edges {
            if e.src as usize >= s.nodes.len() || e.dst as usize >= s.nodes.len() {
                return Err(Fail::Python);
            }
        }
        for n in &s.nodes {
            if n.pos.iter().any(|v| !v.is_finite()) {
                return Err(Fail::Python);
            }
        }
        nodes += s.nodes.len();
    }
    if nodes == 0 {
        return Err(Fail::Python);
    }
    Ok(())
}

/// `config.TEMPLATE`.
const TEMPLATE: &str = "objects/soldiers/templates/spawntemplate_enemy_grunt/2eedbef1-52de-4b20-819c-e71097c15cbe";

/// `config.DEFAULTS`.
fn defaults() -> Value {
    let s = |v: &str| Value::Str(v.to_owned());
    let spawner = Value::dict([
        ("Amount", Value::Int(0)),
        ("SpawnDelay", Value::Float(15.0)),
        ("InitializedSpawnDelay", Value::Float(0.0)),
        ("SpawnSoldierFromBeginning", Value::Bool(true)),
        ("AllowMulipleSpawn", Value::Bool(false)),
        ("Immortal", Value::Bool(false)),
        ("FakeImmortal", Value::Bool(false)),
        ("SpawnEnemyRadius", Value::Float(20.0)),
        ("LodDistance", Value::Float(0.0)),
        ("LocationNameSid", s("")),
        ("UseAsAlternateSpawnPoint", Value::Bool(false)),
        ("TryToSpawnAwayFromSight", Value::Bool(false)),
        ("SoldierCreationPoint", Value::Bool(false)),
        ("OnSpottedType", s("OSPlayer")),
        ("SoldierSquadId", s("SquadNone")),
        ("Enumeration", Value::Int(0)),
    ]);
    let order = Value::dict([
        ("IssueOrderDelay", Value::Float(2.0)),
        ("OnCompleteTimeOut", Value::Float(0.0)),
        ("OnFailedTimeOut", Value::Float(0.0)),
        ("IsUrgent", Value::Bool(true)),
        ("ReissueIfSoldierIsKilled", Value::Bool(true)),
        ("ReadinessOnStart", s("RLNone")),
        ("ReadinessOnComplete", s("RLNone")),
        ("ReadinessOnFail", s("RLNone")),
        ("UnspawnOnComplete", Value::Bool(false)),
        ("UnspawnOnFail", Value::Bool(false)),
        ("AllowStunAnimations", s("SASNoChange")),
        ("Stance", s("SONoChange")),
        ("SpeedState", s("SSORunning")),
        ("AimState", s("AsoNoChange")),
        ("AnimationIndex", Value::Int(-1)),
        ("IdleAnimation", s("null")),
        ("Enumeration", Value::Int(0)),
    ]);
    Value::dict([
        ("spawner", spawner),
        ("order", order),
        ("issue_on", Value::List(vec![s("spawn"), s("player_enter"), s("trigger")])),
    ])
}

/// `pipeline.common(paths, name)`: NavKit's shared unpack while it is at least as new as the game's bundle, else this
/// tool's own cached unpack.
pub(super) fn common(paths: &Paths, name: &str) -> PyResult<String> {
    let bundle = join(
        &join(&join(&join(&join(&paths.game, "Dist"), "win32"), "levels"), name),
        "level-00.fbrb",
    );
    let d = join(&paths.common(), name);
    let manifest = join(&d, "fbrb.manifest");
    if isfile(&manifest) {
        let mt = |p: &str| fs::metadata(p).and_then(|m| m.modified());
        let newer = if isfile(&bundle) {
            match (mt(&manifest), mt(&bundle)) {
                (Ok(a), Ok(b)) => a >= b,
                _ => return Err(Fail::Python),
            }
        } else {
            true
        };
        if newer {
            return Ok(d);
        }
    }
    unpacked(&bundle, &join(&paths.cache(), name))
}

/// `pipeline.kit_refs(paths)`: `us/assault` -> `objects/soldiers/us/assault/<SoldierAsset guid>` from `mp_common`.
fn kit_refs(paths: &Paths) -> PyResult<Vec<(String, String)>> {
    let mp = common(paths, "mp_common")?;
    let mut out = Vec::new();
    for fac in ["US", "RU"] {
        for kit in KIT_NAMES {
            let cap = format!(
                "{}{}",
                kit.get(..1).unwrap_or("").to_uppercase(),
                kit.get(1..).unwrap_or("")
            );
            let p = join(
                &join(&join(&join(&mp, "Objects"), "Soldiers"), fac),
                &format!("{cap}.dbx"),
            );
            if !isfile(&p) {
                return Err(build_error(format!("mp_common has no {fac}/{kit} kit ({p})")));
            }
            let data = fs::read(&p).map_err(|e| os_error(&p, &e))?;
            let d = EditDbx::parse(&data).map_err(|_| Fail::Python)?;
            if !d.has_partition() {
                return Err(Fail::Python);
            }
            let asset: Vec<_> = d
                .instances()
                .into_iter()
                .filter(|&e| d.attr(e, b"type").unwrap_or_default().ends_with(b".SoldierAsset"))
                .collect();
            let [one] = asset.as_slice() else {
                return Err(build_error(format!("{p}: expected one SoldierAsset")));
            };
            let Some(guid) = d.attr(*one, b"guid") else {
                return Err(build_error(format!("{p}: its SoldierAsset has no guid")));
            };
            let guid = text(guid)?;
            if !guid.is_ascii() {
                return Err(Fail::Python);
            }
            out.push((
                format!("{}/{kit}", fac.to_lowercase()),
                format!(
                    "objects/soldiers/{}/{kit}/{}",
                    fac.to_lowercase(),
                    guid.to_ascii_lowercase()
                ),
            ));
        }
    }
    Ok(out)
}
