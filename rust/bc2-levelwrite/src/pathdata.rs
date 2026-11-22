//! The export's path-data step (round A2; `rust/BENCHMARKS.md` "A1", "Part 2: the plan"): an opt-in step that puts
//! the level's path data (Kynapse `pathdata.hpd`, the bots' walkable world and route graph) into the stage beside the
//! level's bundles, recorded in `stage.json` (`path_data`) and checked by `mapexport check` (XVAL-AI-1, and
//! XVAL-PLACE-5 on the staged file).
//!
//! **What it stages today.** NavKit's build of the project's **base level** - its newest: `work\navkit\<base>`
//! (in-game samples), else `work\navkit_offline\<base>` (the offline sampler) - or DICE's own file for a base that
//! has one (`Output\win32\ai\levels\<base>`, the backup an install kept first). The base's build is the level's path
//! data exactly when the project changes nothing a bot walks on: its edits move, add and remove gameplay (spawns,
//! flags, bases, M-COMs, combat areas, vehicle spawners: what the export's mode layer and the auto layout touch)
//! and nothing of the level's collision (static objects, static group members, the terrain). The step refuses in
//! words ([`blockers`]) when the project's edits change the collision, when it has navigation edits (applying them to
//! a NavKit base is still Python: NK5), or when the base has no build.
//!
//! **What it does not do yet** (blocked on NavKit's port, NK4 and NK5, and listed in `rust/BENCHMARKS.md` "A2"):
//! sampling the edited level without the game (`navsample_offline` with the stage's partition and group files, in
//! Rust with NK4), building it in process (NK5's `build_inputs` with the project's objectives as the seeds), applying
//! the project's navigation edits on a NavKit base (NK5), and the stock-MP AI layer for the staged level (`stockmp`
//! by a stock level's name today: `HasAI`, `PathfindingTypes`, `SectorOffsets`, AI spawners and orders).
#![allow(
    clippy::cast_precision_loss,
    reason = "node counts as f64 for shares: far below 2^52"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::files::FileEdit;
use crate::ops::Op;
use crate::project::MapProject;
use crate::write::LevelWriter;

/// Where a level's path data was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The infantry file.
    pub infantry: PathBuf,
    /// The vehicle file beside it, when there is one.
    pub vehicle: Option<PathBuf>,
    /// What it is, in words (`NavKit's build from in-game samples`).
    pub kind: String,
}

/// The path data a stage holds (`stage.json`'s `path_data`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathDataRecord {
    /// The staged files (`ai/infantry/pathdata.hpd`; installed under `Output/win32/ai/levels/<id>/`).
    pub files: Vec<crate::export::StagedFile>,
    /// What they were made from, in words, with the source file.
    pub source: String,
    /// The base level whose build it is.
    pub base: String,
}

/// The candidates for a level's path data, newest kind first: NavKit's build from in-game samples, NavKit's build
/// from the offline sampler, DICE's own file (the install ledger's original when an install replaced it).
#[must_use]
pub fn sources(opfor: &Path, level: &str) -> Vec<Source> {
    let lv = level.to_ascii_lowercase();
    let work = opfor.join("work");
    let mut out = Vec::new();
    for (dir, kind) in [
        (work.join("navkit").join(&lv), "NavKit's build from in-game samples"),
        (
            work.join("navkit_offline").join(&lv),
            "NavKit's build from the offline sampler",
        ),
    ] {
        let f = dir.join("pathdata.hpd");
        if f.is_file() {
            out.push(Source {
                infantry: f,
                vehicle: None,
                kind: kind.to_owned(),
            });
        }
    }
    let ai = opfor
        .join("game")
        .join("Output")
        .join("win32")
        .join("ai")
        .join("levels")
        .join(&lv);
    let original = |p: PathBuf| -> Option<PathBuf> {
        let backup = PathBuf::from(format!("{}.opfor-orig", p.display()));
        if backup.is_file() {
            Some(backup)
        } else {
            p.is_file().then_some(p)
        }
    };
    if let Some(inf) = original(ai.join("infantry").join("pathdata.hpd")) {
        out.push(Source {
            infantry: inf,
            vehicle: original(ai.join("vehicle").join("pathdata.hpd")),
            kind: "DICE's own file".to_owned(),
        });
    }
    out
}

/// The gameplay types whose edits leave the level's collision as it is (what a mode layer, the gameplay tools and the
/// auto layout move, add and remove).
const GAMEPLAY: &[&str] = &[
    "AlternateSpawnEntityData",
    "AreaGeometryEntityData",
    "CapturePointEntityData",
    "CombatAreaTriggerEntityData",
    "SoldierSpawnEntityData",
    "VehicleSpawnEntityData",
];

/// What a project changes that the path data depends on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    /// Static objects moved, added or deleted (their names, upper case).
    pub objects: BTreeSet<String>,
    /// Static group member edits.
    pub members: usize,
    /// Terrain strokes.
    pub terrain: usize,
    /// The navigation edits the log leaves.
    pub nav: usize,
}

impl Changes {
    /// Whether the level's collision changed (it must be sampled again).
    #[must_use]
    pub fn collision(&self) -> bool {
        !self.objects.is_empty() || self.members > 0 || self.terrain > 0
    }

    /// The collision changes in words (empty: none).
    #[must_use]
    pub fn collision_words(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.objects.is_empty() {
            let first: Vec<&str> = self.objects.iter().take(3).map(String::as_str).collect();
            out.push(format!(
                "the project moves, adds or deletes {} static object(s) ({}{}): the level's collision changed, so it \
                 must be sampled again (the offline sampler on the stage: `navsample_offline.py stage`, run by the export when the base level is prepared)",
                self.objects.len(),
                first.join(", "),
                if self.objects.len() > 3 { ", ..." } else { "" }
            ));
        }
        if self.members > 0 {
            out.push(format!(
                "the project edits {} static group member(s): the level's collision changed, so it must be sampled \
                 again (the offline sampler on the stage's group files: `navsample_offline.py stage`, run by the export when the base level is prepared)",
                self.members
            ));
        }
        if self.terrain > 0 {
            out.push(format!(
                "the project sculpts the terrain ({} stroke(s)): the level's ground changed, so it must be sampled \
                 again (the offline sampler on the stage: `navsample_offline.py stage`, run by the export when the base level is prepared)",
                self.terrain
            ));
        }
        out
    }
}

/// What the project changes that the path data depends on: the collision (a static object moved, added or deleted;
/// a static group member edited; the terrain sculpted) and its navigation edits.
#[must_use]
pub fn changes(project: &MapProject, cur: &LevelWriter, base: &LevelWriter) -> Changes {
    let layer = crate::modelayer::layer(cur);
    let base_layer = crate::modelayer::layer(base);
    let gameplay = |g: &str| -> bool {
        let up = g.to_ascii_uppercase();
        if layer.contains(&up) || base_layer.contains(&up) {
            return true;
        }
        let ty = cur
            .find(&up)
            .or_else(|| base.find(&up))
            .map_or_default(|e| cur.find(&up).map_or_else(|| base.type_of(e), |c| cur.type_of(c)));
        GAMEPLAY.iter().any(|t| ty.ends_with(&format!(".{t}")))
    };
    let mut c = Changes::default();
    for o in &project.operations {
        match &o.op {
            Op::Stroke {
                target: crate::files::Target::Terrain { .. },
                ..
            } => c.terrain += 1,
            Op::Files { edits } => {
                c.members += edits
                    .iter()
                    .filter(|e| {
                        matches!(
                            e,
                            FileEdit::MemberTransform { .. } | FileEdit::MemberDelete { .. } | FileEdit::MemberAdd { .. }
                        )
                    })
                    .count();
            }
            _ => {}
        }
        for part in o.op.parts() {
            let named: Vec<&str> = match part {
                Op::Move { .. } | Op::Rotate { .. } | Op::Delete { .. } | Op::Duplicate { .. } => part.targets(),
                Op::Place { .. } => part.creates(),
                _ => Vec::new(),
            };
            for g in named {
                if !gameplay(g) {
                    c.objects.insert(g.to_ascii_uppercase());
                }
            }
        }
    }
    c.nav = project.nav_edits().edits.len();
    c
}

/// Why the base's path data cannot simply be copied for the project's level (empty: it can): the collision changes
/// ([`Changes::collision_words`]) and the navigation edits. The step builds the level in process instead
/// ([`build`]) where it has the samples.
#[must_use]
pub fn blockers(project: &MapProject, cur: &LevelWriter, base: &LevelWriter) -> Vec<String> {
    let c = changes(project, cur, base);
    let mut out = c.collision_words();
    if c.nav > 0 {
        out.push(format!(
            "the project has {} navigation edit(s): they are built into NavKit's build of the level, which needs the \
             level's samples",
            c.nav
        ));
    }
    out
}

/// Where a staged path-data file goes in the game copy (relative to the game folder).
#[must_use]
pub fn install_path(level: &str, kind: &str) -> String {
    format!(
        "Output/win32/ai/levels/{}/{kind}/pathdata.hpd",
        level.to_ascii_lowercase()
    )
}

/// Where a staged traversal-annotation file goes in the game copy (beside the infantry path data, as `navkit
/// install` puts it).
#[must_use]
pub fn traversal_install_path(level: &str) -> String {
    let lv = level.to_ascii_lowercase();
    format!("Output/win32/ai/levels/{lv}/infantry/{lv}.traversal.json")
}

/// The stage's NavKit folder (`navsample_offline.py stage`: the staged level prepared and sampled offline): an
/// OPFOR-like tree, `<stage>\navkit\work\levels\<level>`, `work\navkit\<level>\navkit.json`, `work\navsample`.
#[must_use]
pub fn stage_tree(stage: &Path) -> PathBuf {
    stage.join("navkit")
}

/// The samples a build of the project's level reads, and in words what they are: the stage's own (the offline
/// sampler on the edited level, [`stage_tree`]), else - when the project changes no collision - the base level's
/// in-game samples (`work\navsample`), else its offline ones (`work\navsample_offline`). `None`: none fit.
#[must_use]
pub fn samples_for(opfor: &Path, base_level: &str, stage: &Path, level: &str, collision: bool) -> Option<Samples> {
    let has = |dir: &Path, lv: &str| -> bool {
        ["navkit", "full"].iter().any(|n| {
            dir.join(format!("{lv}_{n}.bin")).is_file() && dir.join(format!("{lv}_{n}_hi.bin")).is_file()
        })
    };
    let tree = stage_tree(stage);
    let own = tree.join("work").join("navsample");
    let lv = level.to_ascii_lowercase();
    let set = tree.join("work").join("navkit").join(&lv).join("navkit.json");
    if !stage.as_os_str().is_empty() && has(&own, &lv) && set.is_file() {
        return Some(Samples {
            dir: own,
            level: lv,
            kind: "the offline sampler on the stage".into(),
            tree: Some(tree),
        });
    }
    if collision {
        return None;
    }
    let b = base_level.to_ascii_lowercase();
    for (dir, kind) in [
        ("navsample", "the base level's in-game samples"),
        ("navsample_offline", "the base level's offline samples"),
    ] {
        let d = opfor.join("work").join(dir);
        if has(&d, &b) {
            return Some(Samples {
                dir: d,
                level: b,
                kind: kind.into(),
                tree: None,
            });
        }
    }
    None
}

/// The samples a build reads ([`samples_for`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Samples {
    /// Their folder.
    pub dir: PathBuf,
    /// The level name they carry (`<level>_navkit.bin`).
    pub level: String,
    /// What they are, in words.
    pub kind: String,
    /// The stage's prepared tree they belong to ([`stage_tree`]); `None`: the base level's in OPFOR's work folder.
    pub tree: Option<PathBuf>,
}

/// What the step will do for the project (asked before anything is built).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Build the level in process from these samples.
    Build(Samples),
    /// Sample the stage offline first (`navsample_offline.py stage`: the project changes the collision), then build.
    Sample,
    /// Copy the base's newest path data as it is (no samples to build from, and nothing that changes it).
    Copy(Source),
}

/// The path-data step's plan for the project ([`Plan`]), or why it cannot run (each reason in words).
///
/// # Errors
/// The reasons it cannot.
pub fn ready(opfor: &Path, project: &MapProject, cur: &LevelWriter, base: &LevelWriter) -> Result<Plan, Vec<String>> {
    ready_at(opfor, project, cur, base, None, None)
}

/// [`ready`] with the stage folder and level id the export writes (the stage's own samples count).
///
/// # Errors
/// The reasons it cannot.
pub fn ready_at(
    opfor: &Path,
    project: &MapProject,
    cur: &LevelWriter,
    base: &LevelWriter,
    stage: Option<&Path>,
    level: Option<&str>,
) -> Result<Plan, Vec<String>> {
    let c = changes(project, cur, base);
    let nowhere = PathBuf::new();
    let st = stage.unwrap_or(&nowhere);
    let lv = level.unwrap_or(&project.base.level);
    let prepared = opfor
        .join("work")
        .join("navkit")
        .join(project.base.level.to_ascii_lowercase())
        .join("navkit.json")
        .is_file();
    if prepared && let Some(s) = samples_for(opfor, &project.base.level, st, lv, c.collision()) {
        return Ok(Plan::Build(s));
    }
    if prepared && c.collision() && stage.is_some() {
        return Ok(Plan::Sample);
    }
    let mut why = c.collision_words();
    if !why.is_empty() && !prepared {
        why.push(format!(
            "the base level {} is not prepared for NavKit (`bfbc2.exe navkit prepare {}`), so the stage cannot be \
             sampled",
            project.base.level,
            project.base.level.to_ascii_uppercase()
        ));
    }
    if !why.is_empty() {
        return Err(why);
    }
    if c.nav > 0 {
        why.push(format!(
            "the project has {} navigation edit(s), but {} has no samples to build them into (sample it: \
             `python bfbc2-auto-navkit\\tools\\navsample_offline.py all {}`)",
            c.nav,
            project.base.level,
            project.base.level.to_ascii_lowercase()
        ));
        return Err(why);
    }
    match sources(opfor, &project.base.level).into_iter().next() {
        Some(src) => Ok(Plan::Copy(src)),
        None => Err(vec![format!(
            "{} has no path data: neither NavKit's build (work\\navkit\\{lv}, work\\navkit_offline\\{lv}), nor samples \
             to build it from, nor DICE's file; sample it first (`python bfbc2-auto-navkit\\tools\\navsample_offline.py \
             all {lv}`)",
            project.base.level,
            lv = project.base.level.to_ascii_lowercase()
        )]),
    }
}

/// The f32 of a partition as Python reads it back from the level's XML (`float()` of its shortest text).
fn xml_f64(v: f32) -> f64 {
    format!("{v}").parse().unwrap_or_else(|_| f64::from(v))
}

/// The translations of a level's instances of a type (`levelobjects.points_of`: the ones away from the origin).
fn points_of(w: &LevelWriter, suffix: &str) -> Vec<(f64, f64, f64)> {
    w.d.of_type(suffix.as_bytes())
        .into_iter()
        .filter_map(|e| w.transform_at(e))
        .map(|r| r[3])
        .filter(|t| t.iter().any(|c| f64::from(*c).abs() > 1e-3))
        .map(|t| (xml_f64(t[0]), xml_f64(t[1]), xml_f64(t[2])))
        .collect()
}

/// The staged level's own seeds and anchors, replacing the base's where the project changed them: the soldier
/// spawns (`.SoldierSpawnEntityData`), the AI orders (`Entity.AreaGeometryEntityData`) and the objectives
/// (`.AreaOrderEntityData`) as NavKit's prepare reads them; then the spawn points (seeds) and the flags and M-COMs
/// (objective floors) the project moved or made, so its gameplay stands on the main route network (XVAL-PLACE-5).
fn level_points(inp: &mut bc2_navbuild::build::Inputs, cur: &LevelWriter, base: &LevelWriter) {
    for (suffix, slot) in [
        (".SoldierSpawnEntityData", 0),
        ("Entity.AreaGeometryEntityData", 1),
        (".AreaOrderEntityData", 2),
    ] {
        let (now, before) = (points_of(cur, suffix), points_of(base, suffix));
        if now == before {
            continue; // the base's own lists (NavKit's prepare's) stand
        }
        match slot {
            0 => inp.seeds = now,
            1 => inp.order_points = Some(now),
            _ => inp.area_orders = Some(now),
        }
    }
    let p3 = |r: crate::ops::Rows| (xml_f64(r[3][0]), xml_f64(r[3][1]), xml_f64(r[3][2]));
    let moved = |guid: &str, r: crate::ops::Rows| base.transform(guid) != Some(r);
    for e in cur.d.of_type(b".AlternateSpawnEntityData") {
        let g = String::from_utf8_lossy(cur.d.attr(e, b"guid").unwrap_or_default()).into_owned();
        if let Some(r) = cur.transform_at(e)
            && moved(&g, r)
        {
            inp.seeds.push(p3(r));
        }
    }
    let mut objectives = Vec::new();
    for e in cur.d.of_type(b".CapturePointEntityData") {
        let g = String::from_utf8_lossy(cur.d.attr(e, b"guid").unwrap_or_default()).into_owned();
        if let Some(r) = cur.transform_at(e)
            && moved(&g, r)
        {
            objectives.push(p3(r));
        }
    }
    for (g, _, p) in crate::modelayer::anchors(cur).iter().filter(|a| a.1.contains("M-COM")) {
        let r = cur.transform(g).unwrap_or_else(|| crate::rows_at(*p));
        if moved(g, r) {
            objectives.push(p3(r));
        }
    }
    if !objectives.is_empty() {
        inp.area_orders.get_or_insert_with(Vec::new).extend(objectives);
    }
}

/// A navigation edit as the side file's JSON value (`navtune.op`'s fields, in order).
fn edit_value(e: &crate::navedit::NavEdit) -> Option<bc2_navbuild::pyjson::Value> {
    bc2_navbuild::pyjson::loads(&serde_json::to_string(e).ok()?)
}

/// The project's level built in process (round NK6): NavKit's build of the base's prepared inputs read from `samples`
/// (`bc2_navbuild::inputs::build_inputs`), with the staged level's spawns and objectives ([`level_points`]) and the
/// project's navigation edits (the walkability edits in the build; links, route edges and traversals by
/// `graphedit` after it, as `navkit apply` does). Its lines go to `out`. Returns the path data's bytes and the
/// traversal annotations' text (when the edits have traversals).
///
/// # Errors
/// The build's error, in words.
#[allow(clippy::too_many_arguments)]
pub fn build_bytes(
    opfor: &Path,
    project: &MapProject,
    cur: &LevelWriter,
    base: &LevelWriter,
    samples: &Samples,
    level: &str,
    clock: (u32, u32),
    out: &mut dyn bc2_navbuild::inputs::Out,
) -> Result<(Vec<u8>, Option<String>), String> {
    use bc2_navbuild::{graphedit, inputs};
    let words = |e: bc2_navbuild::ntpath::Fail| match e {
        bc2_navbuild::ntpath::Fail::Raised(r) => r.msg,
        e => e.to_string(),
    };
    let blv = project.base.level.to_ascii_lowercase();
    let opfor_root = inputs::Root::new(&opfor.display().to_string()).map_err(words)?;
    let mut g = match &samples.tree {
        None => {
            let mut root = opfor_root.clone();
            root.samples_dir = Some(samples.dir.display().to_string());
            inputs::build_inputs(&root, &blv, &inputs::quiet_tool).map_err(words)?
        }
        Some(tree) => {
            // the stage prepared and sampled offline: its own tree (objects, triangle lists, spawns, orders,
            // settings, terrain), the game copy's for the rest; DICE's extent is the base level's
            let t = tree.display().to_string();
            let root = inputs::Root {
                root: t.clone(),
                game: opfor_root.game.clone(),
                work: bc2_navbuild::ntpath::join2(&t, "work").map_err(words)?,
                samples_dir: None,
            };
            let mut g = inputs::build_inputs(&root, &samples.level, &inputs::quiet_tool).map_err(words)?;
            g.inputs.bounds = inputs::reference_bounds(&opfor_root, &blv).map_err(words)?;
            g
        }
    };
    for l in &g.lines {
        out.line(l).map_err(words)?;
    }
    level_points(&mut g.inputs, cur, base);
    let ops: Vec<bc2_navbuild::pyjson::Value> = project.nav_edits().edits.iter().filter_map(edit_value).collect();
    let is = |o: &bc2_navbuild::pyjson::Value, names: &[&str]| {
        o.get("op")
            .and_then(bc2_navbuild::pyjson::Value::as_str)
            .is_some_and(|n| names.contains(&n))
    };
    let walk: Vec<bc2_navbuild::pyjson::Value> = ops
        .iter()
        .filter(|o| is(o, &["walkable", "unwalkable", "seed", "unseed"]))
        .cloned()
        .collect();
    if !walk.is_empty() {
        g.inputs.edits = Some(inputs::edits_of(&walk).map_err(words)?);
    }
    let tmp = std::env::temp_dir().join(format!("bc2_pathdata_{}_{level}.hpd", std::process::id()));
    let run = inputs::Run {
        clock,
        no_detail: false,
    };
    inputs::run_build(&g.inputs, &tmp.display().to_string(), run, out, None).map_err(words)?;
    let read = std::fs::read(&tmp).map_err(|e| format!("{}: {e}", tmp.display()));
    let _ = std::fs::remove_file(&tmp);
    let mut data = read?;
    let ge = |e: graphedit::EditError| match e {
        graphedit::EditError::Raised { msg, .. } => msg,
        graphedit::EditError::Refused(m) => m,
    };
    if ops.iter().any(|o| is(o, &graphedit::GRAPH_OPS)) {
        let edited = graphedit::graph_edits(&data, &ops).map_err(ge)?;
        for l in &edited.lines {
            out.line(l).map_err(words)?;
        }
        out.line(&edited.summary).map_err(words)?;
        data = edited.data;
    }
    if !graphedit::read_back(&data).map_err(ge)? {
        return Err("the built path data does not read back exactly - not usable".into());
    }
    let trav = if ops.iter().any(|o| is(o, &["traversal"])) {
        let name = install_path(level, "infantry");
        let t = graphedit::traversal(&data, &name, &ops, &bc2_navbuild::pyjson::Value::Str(level.to_owned()))
            .map_err(ge)?;
        for l in &t.lines {
            out.line(l).map_err(words)?;
        }
        Some(t.text)
    } else {
        None
    };
    Ok((data, trav))
}

/// Lines kept (the export's report takes the last).
struct Kept(Vec<String>);

impl bc2_navbuild::inputs::Out for Kept {
    fn line(&mut self, text: &str) -> bc2_navbuild::ntpath::Res<()> {
        self.0.push(text.to_owned());
        Ok(())
    }
}

/// Stages the project's path data into `stage` (`ai/<infantry|vehicle>/pathdata.hpd`, and the traversal
/// annotations beside the infantry file) for the level `level`: built in process ([`build_bytes`]) when there are
/// samples to build from ([`ready_at`]), else the base level's newest path data as it is. Nothing in the game copy is
/// written (the build may unpack the base's terrain into OPFOR's work folder, as NavKit's prepare does).
///
/// # Errors
/// The reasons it cannot (each in words), or a file that cannot be read or written.
pub fn stage(
    opfor: &Path,
    project: &MapProject,
    cur: &LevelWriter,
    base: &LevelWriter,
    stage: &Path,
    level: &str,
) -> Result<PathDataRecord, Vec<String>> {
    let mut kept = Kept(Vec::new());
    stage_with(
        opfor,
        project,
        cur,
        base,
        stage,
        level,
        bc2_navbuild::inputs::local_clock(),
        &mut kept,
    )
}

/// The Python the suite runs its Python steps with: `BFBC2_PYTHON`, else `bin\python.txt`'s, else `python`.
fn python() -> PathBuf {
    if let Some(v) = std::env::var_os("BFBC2_PYTHON").filter(|p| !p.is_empty()) {
        return PathBuf::from(v);
    }
    let suite = bc2_pyrt::py::suite_root();
    std::fs::read_to_string(suite.join("bin").join("python.txt"))
        .ok()
        .and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| suite.join(l)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("python"))
}

/// The offline sampler on the stage (`navsample_offline.py stage <stage>`: the staged level prepared from its bundle
/// and sampled into [`stage_tree`]; Python - the sampler's port is NavKit's next step), its lines to `out`.
///
/// # Errors
/// It did not start or failed: its last lines.
pub fn sample_stage(
    opfor: &Path,
    stage: &Path,
    (level, base): (&str, &str),
    out: &mut dyn bc2_navbuild::inputs::Out,
) -> Result<(), String> {
    use std::io::BufRead as _;
    let script = bc2_pyrt::py::suite_root()
        .join("bfbc2-auto-navkit")
        .join("tools")
        .join("navsample_offline.py");
    let mut c = std::process::Command::new(python());
    c.arg(&script)
        .arg("stage")
        .arg(stage)
        .args(["--level", level, "--base", base])
        .arg("--opfor")
        .arg(opfor)
        .env("PYTHONUNBUFFERED", "1")
        .env("OPENBLAS_NUM_THREADS", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        c.creation_flags(0x0800_0000);
    }
    let mut child = c.spawn().map_err(|e| format!("{}: {e}", script.display()))?;
    let err = child.stderr.take().map(|s| {
        std::thread::spawn(move || {
            std::io::BufReader::new(s)
                .lines()
                .map_while(Result::ok)
                .collect::<Vec<String>>()
        })
    });
    let mut tail: Vec<String> = Vec::new();
    if let Some(o) = child.stdout.take() {
        for l in std::io::BufReader::new(o).lines().map_while(Result::ok) {
            let _ = out.line(&l);
            tail.push(l);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let errs = err.and_then(|h| h.join().ok()).unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    tail.extend(errs);
    let n = tail.len();
    Err(tail.get(n.saturating_sub(4)..).unwrap_or_default().join(" | "))
}

/// [`stage`] with the sectors' clock words and the build's lines going to `out`.
///
/// # Errors
/// The reasons it cannot (each in words), or a file that cannot be read or written.
#[allow(clippy::too_many_arguments)]
pub fn stage_with(
    opfor: &Path,
    project: &MapProject,
    cur: &LevelWriter,
    base: &LevelWriter,
    stage: &Path,
    level: &str,
    clock: (u32, u32),
    out: &mut dyn bc2_navbuild::inputs::Out,
) -> Result<PathDataRecord, Vec<String>> {
    let mut plan = ready_at(opfor, project, cur, base, Some(stage), Some(level))?;
    if plan == Plan::Sample {
        sample_stage(opfor, stage, (level, &project.base.level), out).map_err(|e| vec![format!("the offline sampler on the stage: {e}")])?;
        plan = ready_at(opfor, project, cur, base, Some(stage), Some(level))?;
        if plan == Plan::Sample {
            return Err(vec!["the offline sampler on the stage wrote no samples".into()]);
        }
    }
    let mut files = Vec::new();
    let mut put = |rel: String, install: String, bytes: &[u8], entries: usize| -> Result<(), Vec<String>> {
        let out = stage.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|e| vec![format!("{}: {e}", dir.display())])?;
        }
        bc2_formats::atomic::write(&out, &[bytes]).map_err(|e| vec![format!("{}: {e:?}", out.display())])?;
        files.push(crate::export::StagedFile {
            path: rel,
            install,
            sha1: crate::project::sha1_hex(bytes),
            size: bytes.len() as u64,
            entries,
        });
        Ok(())
    };
    let source = match plan {
        Plan::Build(samples) => {
            let (bytes, trav) = build_bytes(opfor, project, cur, base, &samples, level, clock, out)
                .map_err(|e| vec![format!("NavKit's build of the level: {e}")])?;
            let hpd = bc2_formats::hpd::parse(&bytes)
                .map_err(|e| vec![format!("the built path data is not Kynapse path data ({e})")])?;
            let n = hpd.sectors.len();
            put(
                "ai/infantry/pathdata.hpd".into(),
                install_path(level, "infantry"),
                &bytes,
                n,
            )?;
            if let Some(t) = trav {
                let lv = level.to_ascii_lowercase();
                put(
                    format!("ai/infantry/{lv}.traversal.json"),
                    traversal_install_path(level),
                    t.as_bytes(),
                    0,
                )?;
            }
            format!(
                "NavKit's build of the level in process from {} ({})",
                samples.kind,
                samples.dir.display()
            )
        }
        Plan::Sample => return Err(vec!["the offline sampler on the stage wrote no samples".into()]),
        Plan::Copy(src) => {
            for (kind, from) in [("infantry", Some(&src.infantry)), ("vehicle", src.vehicle.as_ref())] {
                let Some(from) = from else { continue };
                let bytes = std::fs::read(from).map_err(|e| vec![format!("{}: {e}", from.display())])?;
                let hpd = bc2_formats::hpd::parse(&bytes)
                    .map_err(|e| vec![format!("{}: not Kynapse path data ({e})", from.display())])?;
                put(
                    format!("ai/{kind}/pathdata.hpd"),
                    install_path(level, kind),
                    &bytes,
                    hpd.sectors.len(),
                )?;
            }
            format!("{} ({})", src.kind, src.infantry.display())
        }
    };
    Ok(PathDataRecord {
        files,
        source,
        base: project.base.level.to_ascii_lowercase(),
    })
}

/// The check lines of a stage's path data (`mapexport check`): each file the one recorded, Kynapse path data, every
/// sector's nodes sorted by id (HPD-NODE-2: unsorted nodes crash the game on leaving the level, AI-CRASH-4)
/// (XVAL-AI-1); and the staged level's spawn points, flags and M-COMs on the infantry file's main route network
/// (XVAL-PLACE-5). Each line: (code, passed, words).
#[must_use]
pub fn check(stage: &Path, rec: &PathDataRecord, level: Option<&LevelWriter>) -> Vec<(&'static str, bool, String)> {
    let mut out = Vec::new();
    for f in &rec.files {
        let p = stage.join(&f.path);
        let Ok(bytes) = std::fs::read(&p) else {
            out.push((
                "XVAL-AI-1",
                false,
                format!("{}: the staged path data is missing", f.path),
            ));
            continue;
        };
        if crate::project::sha1_hex(&bytes) != f.sha1 {
            out.push(("XVAL-AI-1", false, format!("{}: not the file recorded", f.path)));
            continue;
        }
        let hpd = match bc2_formats::hpd::parse(&bytes) {
            Ok(h) => h,
            Err(e) => {
                out.push(("XVAL-AI-1", false, format!("{}: not Kynapse path data ({e})", f.path)));
                continue;
            }
        };
        let unsorted: Vec<String> = hpd
            .sectors
            .iter()
            .filter(|s| s.nodes.windows(2).any(|w| matches!(w, [a, b] if a.id > b.id)))
            .map(|s| format!("{:04X}", s.id))
            .collect();
        out.push((
            "XVAL-AI-1",
            unsorted.is_empty(),
            format!(
                "{}: the path data recorded, {} sectors, {} route nodes, every sector's nodes sorted by id{} ({})",
                f.path,
                hpd.sectors.len(),
                hpd.sectors.iter().map(|s| s.nodes.len()).sum::<usize>(),
                if unsorted.is_empty() {
                    String::new()
                } else {
                    format!(" (not: sectors {})", unsorted.join(", "))
                },
                rec.source
            ),
        ));
        if f.path.contains("infantry")
            && let Some(w) = level
        {
            let routes = routes_of(&hpd, &f.path);
            let off = crate::validate::path_rules(w, None, &routes);
            out.push((
                "XVAL-PLACE-5",
                off.is_empty(),
                format!(
                    "the staged level's spawn points, flags and M-COMs on the staged path data's main route network \
                     ({} route nodes){}",
                    routes.len(),
                    if off.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " (not: {} - {})",
                            off.len(),
                            off.iter()
                                .take(3)
                                .map(|x| x.objects.first().cloned().unwrap_or_default())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                ),
            ));
        }
    }
    out
}

/// How far above the ground a route node stands in a path-data file (the AI workspace's `Y_OFFSET`).
pub const NODE_HEIGHT: f64 = 0.95;

/// A path-data file's route graph for the path rule (XVAL-PLACE-5): its nodes (each sector's border nodes once, by id;
/// on the ground: [`NODE_HEIGHT`] below) and their islands (the edges' connected parts, the largest first; equal sizes
/// in the order their first node comes), as the AI workspace makes them.
#[must_use]
pub fn routes_of(hpd: &bc2_formats::hpd::HpdFile<'_>, source: &str) -> crate::site::Routes {
    let mut index: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut edges: Vec<[usize; 2]> = Vec::new();
    for s in &hpd.sectors {
        let local: Vec<usize> = s
            .nodes
            .iter()
            .map(|nd| {
                *index.entry(nd.id).or_insert_with(|| {
                    nodes.push([
                        f64::from(nd.pos[0]),
                        f64::from(nd.pos[1]) - NODE_HEIGHT,
                        f64::from(nd.pos[2]),
                    ]);
                    nodes.len() - 1
                })
            })
            .collect();
        for e in &s.edges {
            if let (Some(&a), Some(&b)) = (local.get(e.src as usize), local.get(e.dst as usize)) {
                edges.push([a, b]);
            }
        }
    }
    let n = nodes.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let find = |parent: &mut Vec<usize>, mut a: usize| {
        while let Some(&p) = parent.get(a)
            && p != a
        {
            let gp = parent.get(p).copied().unwrap_or(p);
            if let Some(x) = parent.get_mut(a) {
                *x = gp;
            }
            a = gp;
        }
        a
    };
    for &[a, b] in &edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb
            && let Some(x) = parent.get_mut(ra)
        {
            *x = rb;
        }
    }
    let roots: Vec<usize> = (0..n).map(|k| find(&mut parent, k)).collect();
    let mut order: Vec<usize> = Vec::new();
    let mut size: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for &r in &roots {
        let c = size.entry(r).or_insert(0);
        if *c == 0 {
            order.push(r);
        }
        *c += 1;
    }
    order.sort_by_key(|r| std::cmp::Reverse(size.get(r).copied().unwrap_or(0)));
    let rank: std::collections::HashMap<usize, u32> = order.iter().enumerate().map(|(k, &r)| (r, k as u32)).collect();
    let island = roots.iter().map(|r| rank.get(r).copied().unwrap_or(0)).collect();
    crate::site::Routes::new(nodes, island, source)
}
