//! `navkit sample <level> --offline` (round NKS-deep C): NavKit's offline sampler in Rust (`bc2_navbuild::sampler`,
//! the port of `navsample_offline.py`) - the in-game sampler's files computed from the level's own collision and
//! terrain, without starting the game.
//!
//! ```text
//! navkit sample <level> --offline [--pitch 0.8|0.2] [--dice] [--vehicle] [--out <folder>]
//!     the 0.8 m pair over the level's box, then the 0.2 m pair over its detailed sectors (--pitch: one of them;
//!     --dice: DICE's own detailed sectors too, KYG-DET-1); --vehicle: a vehicle-shaped run at pitch 1.0
//!     (KYG-PAR-2's settings) as <level>_navkit_veh.bin. Into work\navsample_offline (or --out).
//! navkit sample <level> --offline --like <in-game file>...
//!     each in-game file's grid sampled offline (<level>_like_<file>) and compared with it; the measures printed
//!     and saved as <level>_validate.json
//! navkit sample <staged level> --offline --stage <stage folder> --base <base level>
//!     an export's stage (its path-data step): the staged bundle prepared like a level into
//!     work\navkit_stage\<level> and sampled there at 0.8 m and over its detailed sectors at 0.2 m
//! ```
//!
//! The in-game sampler (`navkit sample <level>` without `--offline`) stays the Python's: it starts the game.

use bc2_navbuild::ntpath as nos;
use bc2_navbuild::pyjson::Value as NValue;
use bc2_navbuild::sampler::compare::{FileColumns, SampleColumns, compare};
use bc2_navbuild::sampler::level::{self, Detailed, LevelOptions, Settings, Source};

use super::build::{root, tool};
use super::navkit_error;
use super::project::{Print, Project};
use crate::navshared::fail;
use crate::pyos::{self, join, join2};
use crate::pyproc::{Fail, Py, PyResult};

/// The library's error as the command line's (an input it cannot read is said in words: there is no Python twin to
/// hand it to).
fn lib<T>(r: nos::Res<T>) -> PyResult<T> {
    r.map_err(|e| match e {
        nos::Fail::Handoff => navkit_error(
            "the offline sampler cannot read this level's inputs (a damaged settings or \
                                            object list?)",
        ),
        other => fail(other),
    })
}

/// The command line's options.
pub struct Opts<'a> {
    /// `--pitch`.
    pub pitch: Option<&'a str>,
    /// `--out`.
    pub out: Option<&'a str>,
    /// `--stage`.
    pub stage: Option<&'a str>,
    /// `--base`.
    pub base: Option<&'a str>,
}

/// `navkit sample <args> --offline`: `args` from the level on (the flags among them).
pub fn offline(py: &mut Py, pr: &Project, args: &[String], o: &Opts<'_>) -> PyResult<i32> {
    let flag = |f: &str| args.iter().any(|a| a == f);
    let like_at = args.iter().position(|a| a == "--like");
    let before = &args[..like_at.unwrap_or(args.len())];
    let Some(name) = before.iter().find(|a| !a.starts_with("--")) else {
        return Err(navkit_error("sample --offline: which level?"));
    };
    let r = root(pr);
    if let Some(stage) = o.stage {
        let base = o
            .base
            .ok_or_else(|| navkit_error("sample --offline --stage: which base level? (--base)"))?;
        return stage_level(py, pr, stage, &name.to_lowercase(), &pr.level(base)?).map(|()| 0);
    }
    let lv = pr.level(name)?;
    let out = match o.out {
        Some(d) => d.to_owned(),
        None => lib(level::out_dir(&r))?,
    };
    if let Some(at) = like_at {
        let files: Vec<&String> = args[at + 1..].iter().filter(|a| !a.starts_with("--")).collect();
        if files.is_empty() {
            return Err(navkit_error("sample --offline --like: which in-game sample files?"));
        }
        return validate(py, pr, &lv, &files, &out).map(|()| 0);
    }
    let (coarse, fine) = match o.pitch {
        None => (true, true),
        Some(p) => match super::py_float(p)? {
            v if (v - 0.8).abs() < 1e-6 => (true, false),
            v if (v - 0.2).abs() < 1e-6 => (false, true),
            _ => {
                return Err(navkit_error(format!(
                    "{lv}: the offline sampler runs at pitch 0.8 or 0.2, not {p}"
                )));
            }
        },
    };
    let opt = LevelOptions {
        coarse,
        detailed: fine.then_some(if flag("--dice") {
            Detailed::WithDice
        } else {
            Detailed::Own
        }),
        vehicle: flag("--vehicle"),
        out: Some(out),
        ..LevelOptions::default()
    };
    let mut err = None;
    let rep = {
        let mut log = |s: &str| -> nos::Res<()> {
            py.print(s).map_err(|e| {
                err.get_or_insert(e);
                nos::Fail::Sink
            })
        };
        level::sample_level(&r, &lv, &opt, &mut log, &tool)
    };
    let rep = match rep {
        Err(nos::Fail::Sink) => return Err(err.unwrap_or(Fail::Python)),
        other => lib(other)?,
    };
    if fine && !rep.detailed.is_empty() {
        let ids: Vec<String> = rep.detailed.iter().map(|(i, j)| format!("({i}, {j})")).collect();
        py.print(&format!("  detailed sectors at 0.2: {}", ids.join(" ")))?;
        if !rep.dice_only.is_empty() {
            let d: Vec<String> = rep.dice_only.iter().map(|(i, j)| format!("({i}, {j})")).collect();
            py.print(&format!("  of them DICE's only: {}", d.join(" ")))?;
        }
    }
    Ok(0)
}

/// `validate`: each in-game file's grid sampled offline and compared; `<out>\<lv>_validate.json`.
fn validate(py: &mut Py, pr: &Project, lv: &str, files: &[&String], out: &str) -> PyResult<()> {
    let r = root(pr);
    let mut bx: Option<[f64; 4]> = None;
    for f in files {
        let g = level::grid_box(&lib(level::grid_of_file(f))?.0);
        bx = Some(bx.map_or(g, |b| [b[0].min(g[0]), b[1].min(g[1]), b[2].max(g[2]), b[3].max(g[3])]));
    }
    let (geo, info, terrain) = lib(level::load_level(&r, lv, bx, level::BODIES, &tool))?;
    py.print(&info.line())?;
    let mut summary: Vec<(String, NValue)> = Vec::new();
    for f in files {
        let name = pyos::basename(f)?;
        let dest = join2(out, &format!("{lv}_like_{name}"))?;
        py.print(&format!("== offline sample {lv} on the grid of {name}"))?;
        let s = lib(level::like(f, &geo, terrain.as_ref(), Some(&dest)))?;
        py.print(&format!("wrote {dest}"))?;
        let mut a = FileColumns::open(f).map_err(|e| navkit_error(format!("{f}: {e}")))?;
        let m = compare(&mut a, &mut SampleColumns::new(&s), 0.1).map_err(|e| navkit_error(format!("{f}: {e}")))?;
        for (k, v) in &m {
            if k.starts_with("blocked_") && !k.ends_with("matched")
                || *k == "columns"
                || k.starts_with("surfaces")
                || *k == "object_surf_a"
            {
                py.print(&format!("  {k:24} {}", level::thousands(*v as usize)))?;
            } else {
                py.print(&format!("  {k:24} {v:.2}"))?;
            }
        }
        let mut d: Vec<(String, NValue)> = m
            .iter()
            .map(|(k, v)| {
                (
                    (*k).to_owned(),
                    if v.fract() == 0.0 && v.abs() < 1e15 {
                        NValue::Int(*v as i128)
                    } else {
                        NValue::Float(*v)
                    },
                )
            })
            .collect();
        d.sort_by(|a, b| a.0.cmp(&b.0));
        summary.push((name, NValue::Dict(d)));
    }
    summary.sort_by(|a, b| a.0.cmp(&b.0));
    let p = join2(out, &format!("{lv}_validate.json"))?;
    let text = bc2_navbuild::pyjson::dumps(&NValue::Dict(summary), Some(1));
    bc2_gamefs::atomic::write(std::path::Path::new(&p), &[text.as_bytes()])
        .map_err(|e| navkit_error(format!("{p}: {e}")))?;
    py.print(&format!("wrote {p}"))
}

/// `stage_tree(root, lv)`: a staged level's NavKit folder, `<root>\work\navkit_stage\<level>`.
fn stage_tree(pr: &Project, lv: &str) -> PyResult<String> {
    join(&pr.work, &["navkit_stage", lv])
}

/// `stage_level(root, stage, level, base)`: the offline sampler on an export's stage - the staged bundle prepared as
/// `navkit prepare` does (unpacked and converted; objects, group members, spawns, orders, ladders and the triangle
/// lists from the staged partition, the base level's unpacks for the blueprints), the base level's settings and
/// terrain, both capsule runs at 0.8 m and over the detailed sectors at 0.2 m, into `stage_tree`.
fn stage_level(py: &mut Py, pr: &Project, stage: &str, lv: &str, base: &str) -> PyResult<()> {
    let r = root(pr);
    let lb = pr.level_work(base)?;
    let sb = lib(r.settings(base))?;
    let tree = stage_tree(pr, lv)?;
    let w = join2(&tree, "work")?;
    let l = join(&w, &["levels", lv])?;
    let bundle = join2(stage, "level-00.fbrb")?;
    if !pyos::isfile(&bundle) {
        return Err(navkit_error(format!("{stage}: no staged level-00.fbrb")));
    }
    py.print(&format!("== offline sample of the stage {lv} (base {base})"))?;
    pr.unpack(py, &bundle, &join2(&l, "dist")?, &mut Print)?;
    pyos::makedirs(&join2(&l, "pkg")?)?;
    let part = super::prepare::level_partition_name(&l, lv)?;
    // the base's static-group files the stage does not carry (an export writes only the groups it edits)
    let bpart = sb.get("level").and_then(NValue::as_str).ok_or(Fail::Python)?.to_owned();
    let src = join(&lb, &["dist", "levels", &bpart])?;
    let dst = join(&l, &["dist", "levels", &part])?;
    if pyos::isdir(&src) {
        pyos::makedirs(&dst)?;
        let mut names = pyos::listdir(&src)?;
        names.sort();
        for f in names {
            let to = join2(&dst, &f)?;
            if f.to_lowercase().ends_with("_havokasset.xml") && !pyos::isfile(&to) {
                let from = join2(&src, &f)?;
                std::fs::copy(&from, &to).map_err(|e| crate::pyproc::os_error(&from, &e))?;
            }
        }
    }
    let common = lib(level::common_bases(&r))?;
    let mut extra = vec![join2(&lb, "pkg")?, join2(&lb, "dist")?];
    extra.extend(common.iter().rev().cloned());
    extra.retain(|b| pyos::isdir(b));
    py.print("  extracting objects, collision and spawn points ...")?;
    if super::levelobjects::main(py, &l, &part, &extra)? != 0 {
        return Err(navkit_error(format!("{lv}: levelobjects failed on the stage")));
    }
    // json.dumps(lad, indent=1, sort_keys=True)
    let lad = sorted(super::ladders::extract(&l, &extra)?);
    let lp = join2(&l, "ladders.json")?;
    std::fs::write(&lp, crate::pyjson::dumps(&lad, Some(1)).replace('\n', "\r\n"))
        .map_err(|e| crate::pyproc::os_error(&lp, &e))?;
    // the base's settings for the stage (its box, heights and sector offsets), sorted keys
    let mut st: Vec<(String, NValue)> = match &sb {
        NValue::Dict(d) => d
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "level" | "box_source" | "offsets_source"))
            .cloned()
            .collect(),
        _ => return Err(Fail::Python),
    };
    st.push(("level".into(), NValue::Str(part.clone())));
    st.push(("box_source".into(), NValue::Str(format!("the base level {base}"))));
    st.push(("offsets_source".into(), NValue::Str(format!("the base level {base}"))));
    st.sort_by(|a, b| a.0.cmp(&b.0));
    let d = join(&w, &["navkit", lv])?;
    pyos::makedirs(&d)?;
    let sp = join2(&d, "navkit.json")?;
    std::fs::write(
        &sp,
        bc2_navbuild::pyjson::dumps(&NValue::Dict(st), Some(1)).replace('\n', "\r\n"),
    )
    .map_err(|e| crate::pyproc::os_error(&sp, &e))?;
    // the base's terrain: its unpack copied, so the height field reads; then the stage's own height field over it
    // (round E5: an export that sculpts writes it into the staged terrain bundle)
    let (tb, tt) = (join2(&lb, "terrain")?, join2(&l, "terrain")?);
    if pyos::isdir(&tb) && !pyos::isdir(&tt) {
        copy_tree(&tb, &tt)?;
    }
    staged_heightfield(py, stage, &tt)?;
    let settings = lib(Settings::of(&bc2_navbuild::inputs::Root::new(&tree).map_err(fail)?, lv))?;
    let src = Source {
        level_dir: l.clone(),
        extra,
        common: common.into_iter().rev().collect(),
        terrain_root: tree.clone(),
        settings,
        dice: Vec::new(),
    };
    let out = join2(&w, "navsample")?;
    pyos::makedirs(&out)?;
    let opt = LevelOptions {
        out: Some(out),
        ..LevelOptions::default()
    };
    let mut err = None;
    let rep = {
        let mut log = |s: &str| -> nos::Res<()> {
            py.print(s).map_err(|e| {
                err.get_or_insert(e);
                nos::Fail::Sink
            })
        };
        level::sample_source(&src, lv, opt.out.as_deref().unwrap_or_default(), &opt, &mut log, &tool)
    };
    match rep {
        Err(nos::Fail::Sink) => Err(err.unwrap_or(Fail::Python)),
        other => lib(other).map(|_| ()),
    }
}

/// The staged terrain bundle's main height field (`*.heightfield-000.res`) written over the stage tree's terrain unpack
/// (round E5): the sampler then reads the sculpted ground, not the base's. Nothing when the stage has no terrain
/// bundle.
fn staged_heightfield(py: &mut Py, stage: &str, tt: &str) -> PyResult<()> {
    let bundle = std::path::Path::new(stage).join("terrain-00.fbrb");
    if !bundle.is_file() {
        return Ok(());
    }
    let h = bc2_formats::fbrb::entries_path(&bundle).map_err(|e| navkit_error(format!("{}: {e}", bundle.display())))?;
    let names: Vec<String> = h
        .entries
        .iter()
        .filter(|e| !e.is_deleted() && e.name.to_ascii_lowercase().ends_with(".heightfield-000.res"))
        .map(|e| e.name.clone())
        .collect();
    let datas = bc2_formats::fbrb::read_path_uncached(&bundle, &names)
        .map_err(|e| navkit_error(format!("{}: {e}", bundle.display())))?;
    for (name, data) in names.iter().zip(datas) {
        let to = name.split('/').fold(std::path::PathBuf::from(tt), |p, s| p.join(s));
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| crate::pyproc::os_error(&dir.display().to_string(), &e))?;
        }
        bc2_gamefs::atomic::write(&to, &[data.as_slice()]).map_err(|e| navkit_error(format!("{}: {e}", to.display())))?;
        py.print(&format!("  the stage's height field {name}"))?;
    }
    Ok(())
}

/// A JSON value with every dict's keys sorted (`sort_keys=True`).
fn sorted(v: crate::pyjson::Value) -> crate::pyjson::Value {
    use crate::pyjson::Value as V;
    match v {
        V::List(l) => V::List(l.into_iter().map(sorted).collect()),
        V::Dict(d) => {
            let mut d: Vec<(String, V)> = d.into_iter().map(|(k, x)| (k, sorted(x))).collect();
            d.sort_by(|a, b| a.0.cmp(&b.0));
            V::Dict(d)
        }
        other => other,
    }
}

/// `shutil.copytree(src, dst)`.
fn copy_tree(src: &str, dst: &str) -> PyResult<()> {
    pyos::makedirs(dst)?;
    for e in std::fs::read_dir(src).map_err(|e| crate::pyproc::os_error(src, &e))? {
        let e = e.map_err(|e| crate::pyproc::os_error(src, &e))?;
        let name = e.file_name().to_string_lossy().into_owned();
        let (from, to) = (join2(src, &name)?, join2(dst, &name)?);
        if e.file_type().map_err(|e| crate::pyproc::os_error(&from, &e))?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| crate::pyproc::os_error(&from, &e))?;
        }
    }
    Ok(())
}
