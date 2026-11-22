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
//! by a stock level's name today: HasAI, PathfindingTypes, SectorOffsets, AI spawners and orders).
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

/// Why the base's path data is not the project's level's (empty: it is): edits of the collision (a static object
/// moved, added or deleted; a static group member edited; the terrain sculpted) - the level must be sampled again,
/// NK4 - and navigation edits, which a NavKit base takes only through the Python (NK5).
#[must_use]
pub fn blockers(project: &MapProject, cur: &LevelWriter, base: &LevelWriter) -> Vec<String> {
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
            .map(|e| cur.find(&up).map_or_else(|| base.type_of(e), |c| cur.type_of(c)))
            .unwrap_or_default();
        GAMEPLAY.iter().any(|t| ty.ends_with(&format!(".{t}")))
    };
    let mut objects: BTreeSet<String> = BTreeSet::new();
    let mut members = 0usize;
    let mut terrain = 0usize;
    let mut nav = 0usize;
    for o in &project.operations {
        match &o.op {
            Op::NavAdd { .. } | Op::NavLoad { .. } => nav += 1,
            Op::Stroke {
                target: crate::files::Target::Terrain { .. },
                ..
            } => terrain += 1,
            Op::Files { edits } => {
                members += edits
                    .iter()
                    .filter(|e| {
                        matches!(
                            e,
                            FileEdit::MemberTransform { .. }
                                | FileEdit::MemberDelete { .. }
                                | FileEdit::MemberAdd { .. }
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
                    objects.insert(g.to_ascii_uppercase());
                }
            }
        }
    }
    let mut out = Vec::new();
    if !objects.is_empty() {
        let first: Vec<&str> = objects.iter().take(3).map(String::as_str).collect();
        out.push(format!(
            "the project moves, adds or deletes {} static object(s) ({}{}): the level's collision changed, so it must \
             be sampled again (the offline sampler on the edited partition: NK4)",
            objects.len(),
            first.join(", "),
            if objects.len() > 3 { ", ..." } else { "" }
        ));
    }
    if members > 0 {
        out.push(format!(
            "the project edits {members} static group member(s): the level's collision changed, so it must be sampled \
             again (with the stage's group files: NK4)"
        ));
    }
    if terrain > 0 {
        out.push(format!(
            "the project sculpts the terrain ({terrain} stroke(s)): the level's ground changed, so it must be sampled \
             again (NK4)"
        ));
    }
    if nav > 0 {
        out.push(format!(
            "the project has {nav} navigation edit(s): applying them to a NavKit build is still Python (`navkit \
             apply` on a NavKit base: NK5)"
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

/// Stages the project's path data into `stage` (`ai/<infantry|vehicle>/pathdata.hpd`) for the level `level`: the
/// base level's newest path data ([`sources`]) when nothing blocks it ([`blockers`]). Nothing in the game copy or
/// the OPFOR folder is written.
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
    let mut why = blockers(project, cur, base);
    let found = sources(opfor, &project.base.level);
    let Some(src) = found.first() else {
        why.push(format!(
            "{} has no path data: neither NavKit's build (work\\navkit\\{lv}, work\\navkit_offline\\{lv}) nor DICE's \
             file; build it first (`python bfbc2-auto-navkit\\tools\\navsample_offline.py all {lv}`, Python until \
             NK4 / NK5)",
            project.base.level,
            lv = project.base.level.to_ascii_lowercase()
        ));
        return Err(why);
    };
    if !why.is_empty() {
        return Err(why);
    }
    let mut files = Vec::new();
    for (kind, from) in [("infantry", Some(&src.infantry)), ("vehicle", src.vehicle.as_ref())] {
        let Some(from) = from else { continue };
        let bytes = std::fs::read(from).map_err(|e| vec![format!("{}: {e}", from.display())])?;
        let hpd = bc2_formats::hpd::parse(&bytes)
            .map_err(|e| vec![format!("{}: not Kynapse path data ({e})", from.display())])?;
        let rel = format!("ai/{kind}/pathdata.hpd");
        let out = stage.join("ai").join(kind).join("pathdata.hpd");
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|e| vec![format!("{}: {e}", dir.display())])?;
        }
        bc2_formats::atomic::write(&out, &[&bytes]).map_err(|e| vec![format!("{}: {e:?}", out.display())])?;
        files.push(crate::export::StagedFile {
            path: rel,
            install: install_path(level, kind),
            sha1: crate::project::sha1_hex(&bytes),
            size: bytes.len() as u64,
            entries: hpd.sectors.len(),
        });
    }
    Ok(PathDataRecord {
        files,
        source: format!("{} ({})", src.kind, src.infantry.display()),
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
