//! The export's rules checked on a project as it is edited (round V1): the [`crate::rules`] of [`Phase::Edit`] and
//! L1's placement tendencies ([`crate::placement`]), before anything is written. The editor's Problems panel shows
//! them live (after every change), `mapexport validate` prints them; `mapexport build` refuses the errors again and
//! `mapexport check` proves the staged level.
//!
//! [`Phase::Edit`]: crate::rules::Phase::Edit

use std::collections::{HashMap, HashSet};

use crate::export::Game;
use crate::files::Target;
use crate::mpconvert::guid_of;
use crate::placement::{self, Mode};
use crate::project::{MapProject, Profile};
use crate::rules::Finding;
use crate::write::LevelWriter;

/// What the edit-time rules read.
#[derive(Clone, Copy)]
pub struct Input<'a> {
    /// The project.
    pub project: &'a MapProject,
    /// The level partition as the log leaves it (`None`: it does not parse).
    pub level: Option<&'a LevelWriter>,
    /// The base level's partition (`None`: not read).
    pub base: Option<&'a LevelWriter>,
    /// The mode choice's donor partition as the game copy has it now, and its sha1 (`None`: not read).
    pub donor: Option<(&'a LevelWriter, &'a str)>,
    /// The game copy, for the names' clashes (`None`: only their form is checked).
    pub game: Option<&'a crate::names::Taken>,
    /// The game copy's languages (`None`: plain ASCII).
    pub languages: Option<&'a crate::names::Languages>,
    /// The base level's own DBX check findings' words ([`dbx_check`] of `base`; `None`: made here when `base` is
    /// given). The editor keeps them: the base does not change while a project is open.
    pub base_check: Option<&'a HashSet<String>>,
    /// The mode L1's tendencies are checked for (`None`: the project's, else the level's own: [`mode_of`]).
    pub mode: Option<Mode>,
}

/// The mode a project is played as: its target's, else the base level's own layer's.
#[must_use]
pub fn mode_of(project: &MapProject, level: &LevelWriter) -> Option<Mode> {
    match project.mode() {
        Some(c) => Mode::of_key(&c.mode),
        None => placement::detect_mode(level),
    }
}

/// The blueprint of the 40 km/h sign whose borrowed shader entries crashed the renderer (XVAL-SHDR-2).
pub const SPEED_SIGN: &str = "objects/objects/roadsigns/speedsign_40";

/// Every edit-time finding of the project: its log's, its mode's, its names', its objects' and L1's placement
/// tendencies.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the rules in the registry's order")]
pub fn validate(i: Input<'_>) -> Vec<Finding> {
    let p = i.project;
    let mut out = Vec::new();
    // the log's files the export does not write
    for o in &p.operations {
        for t in o.op.files() {
            if matches!(t, Target::Terrain { .. } | Target::Paint { .. }) {
                out.push(Finding::new(
                    "XVAL-EDIT-5",
                    format!(
                        "Operation {} ({}) edits {}: the export does not write terrain yet (round E5).",
                        o.n,
                        o.name(),
                        t.describe()
                    ),
                ));
            }
        }
    }
    // the mode
    if let Some(c) = p.mode() {
        if let Some(why) = crate::modelayer::not_offered(&c.mode) {
            out.push(Finding::new("XVAL-MODE-1", why));
        }
        if let Some((d, sha1)) = i.donor {
            if sha1 != c.donor_sha1 {
                out.push(Finding::new(
                    "XVAL-MODE-4",
                    format!(
                        "{}'s partition changed since the mode was chosen (sha1 {} then, {sha1} now): the layer is \
                         grafted from it as it is now.",
                        c.donor, c.donor_sha1
                    ),
                ));
            }
            if let Some(w) = i.level {
                let miss = crate::modelayer::missing(
                    &c.mode,
                    &crate::modelayer::signature(w),
                    &crate::modelayer::signature(d),
                );
                for m in miss {
                    out.push(Finding::new(
                        "XVAL-MODE-3",
                        format!("The mode layer is incomplete: {m}."),
                    ));
                }
            }
        }
    }
    // the names (the export's own choice of them, in the log)
    if let Some(n) = p.names() {
        let mode = p
            .mode()
            .map_or_else(String::new, |c| crate::export::mode_title(&c.mode));
        for x in crate::names::check(n, Some(&p.id), &mode, i.game, i.languages) {
            out.push(Finding::new(x.code, x.message));
        }
    }
    let Some(w) = i.level else {
        out.push(Finding::new(
            "XVAL-EDIT-4",
            "The level partition does not parse: no edit is written to it.",
        ));
        return out;
    };
    // the partition's own consistency (DbxEditGUI's Level > Check, the DBX inspector's)
    // (what the base level itself already has is DICE's, not an edit's: only what the edits brought is reported)
    let made: HashSet<String>;
    let before = match (i.base_check, i.base) {
        (Some(b), _) => Some(b),
        (None, Some(b)) => {
            made = base_check(b);
            Some(&made)
        }
        (None, None) => None,
    };
    out.extend(
        dbx_check(w)
            .into_iter()
            // shared or other cull slots are the DBX inspector's to report: the export gives moved, copied and
            // placed objects slots of their own (round XF1)
            .filter(|f| !matches!(f.code.as_str(), "XVAL-CULL-5" | "XVAL-CULL-6"))
            .filter(|f| before.is_none_or(|b| !b.contains(&f.message))),
    );
    // objects the edits moved or made: within the cull tree's range, no borrowed 40 km/h sign
    out.extend(out_of_range(w, i.base));
    let base_bp: HashSet<String> = i.base.map_or_default(|b| {
        b.d.of_type(b".ReferenceObjectData")
            .into_iter()
            .filter_map(|e| crate::culltree::blueprint_of(b, e))
            .collect()
    });
    let base_placed = i.base.map(placed);
    for e in w.d.of_type(b".ReferenceObjectData") {
        let g = guid_of(&w.d, e);
        let Some(rows) = w.transform_at(e) else { continue };
        let before = base_placed.as_ref().and_then(|b| b.get(&g));
        if before == Some(&rows) {
            continue;
        }
        if before.is_none()
            && let Some(bp) = crate::culltree::blueprint_of(w, e)
            && bp.starts_with(SPEED_SIGN)
            && i.base.is_some()
            && !base_bp.contains(&bp)
        {
            out.push(
                Finding::new(
                    "XVAL-SHDR-2",
                    format!(
                        "{g} places the 40 km/h sign on a level that does not draw it: its shader entries would come \
                         from another level, which crashed the renderer at 0x98BD6E."
                    ),
                )
                .naming(vec![g]),
            );
        }
    }
    // vehicle spawners of one team on top of each other
    out.extend(stacked_spawners(w, i.base));
    // L1's placement tendencies
    if let Some(m) = i.mode.or_else(|| mode_of(p, w)) {
        out.extend(placement::check(w, m, p.target_profile()));
    }
    out
}

/// The words of a base level's own DBX check findings: what [`validate`] leaves out as DICE's.
#[must_use]
pub fn base_check(base: &LevelWriter) -> HashSet<String> {
    dbx_check(base).into_iter().map(|f| f.message).collect()
}

/// Every placed object's transform by guid (one pass: [`LevelWriter::find`] walks every instance).
fn placed(w: &LevelWriter) -> HashMap<String, crate::ops::Rows> {
    w.d.of_type(b".ReferenceObjectData")
        .into_iter()
        .filter_map(|e| w.transform_at(e).map(|r| (guid_of(&w.d, e), r)))
        .collect()
}

/// Placed objects the edits moved or made beyond the cull tree's +-2048 m (XVAL-CULL-4): the export cannot slot
/// them (`culltree::encode`).
#[must_use]
pub fn out_of_range(w: &LevelWriter, base: Option<&LevelWriter>) -> Vec<Finding> {
    let mut out = Vec::new();
    let before = base.map(placed);
    for e in w.d.of_type(b".ReferenceObjectData") {
        let g = guid_of(&w.d, e);
        let Some(rows) = w.transform_at(e) else { continue };
        if before.as_ref().and_then(|b| b.get(&g)) == Some(&rows) {
            continue;
        }
        let [x, y, z] = rows[3];
        if [x, y, z].iter().any(|v| v.abs() > 2047.9) {
            out.push(
                Finding::new(
                    "XVAL-CULL-4",
                    format!(
                        "{g} stands at ({x:.0}, {y:.0}, {z:.0}): beyond +-2048 m the export cannot give it a cull slot."
                    ),
                )
                .naming(vec![g]),
            );
        }
    }
    out
}

/// `DbxEditGUI`'s Level > Check (the DBX inspector's, round GR4) on a partition, each finding under its rule.
#[must_use]
pub fn dbx_check(w: &LevelWriter) -> Vec<Finding> {
    let Ok(bytes) = w.write() else { return Vec::new() };
    let Ok(f) = bc2_formats::dbxedit::EditFile::parse(bytes) else {
        return Vec::new();
    };
    let Ok(found) = bc2_formats::dbxedit::level_check(&f, None, &[]) else {
        return Vec::new();
    };
    found
        .into_iter()
        .map(|x| {
            let guid = x
                .place
                .split_whitespace()
                .nth(1)
                .filter(|g| g.len() == 36)
                .map(str::to_ascii_uppercase);
            Finding::new(
                crate::rules::dbx_check_code(&x.message),
                format!("{}: {}", x.place, x.message),
            )
            .naming(guid.into_iter().collect())
        })
        .collect()
}

/// The same place, to the bit (a spawner the edits left where the base has it).
fn same(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits())
}

/// A vehicle spawner: its guid, its team, where it stands.
type Spawner = (String, String, [f32; 3]);

/// Vehicle spawners of one team within 1 m of each other (XVAL-PLACE-1); pairs the base level already has are
/// DICE's own and left out.
#[must_use]
pub fn stacked_spawners(w: &LevelWriter, base: Option<&LevelWriter>) -> Vec<Finding> {
    let spawners: Vec<(String, String, [f32; 3])> =
        w.d.of_type(b".VehicleSpawnEntityData")
            .into_iter()
            .filter_map(|e| {
                let g = guid_of(&w.d, e);
                let team =
                    w.d.child(e, b"Team")
                        .and_then(|x| w.d.text(x))
                        .map_or_default(|t| String::from_utf8_lossy(t).into_owned());
                w.transform_at(e).map(|r| (g, team, r[3]))
            })
            .collect();
    let base_at: Option<HashMap<String, [f32; 3]>> = base.map(|bw| {
        bw.d.of_type(b".VehicleSpawnEntityData")
            .into_iter()
            .filter_map(|e| bw.transform_at(e).map(|r| (guid_of(&bw.d, e), r[3])))
            .collect()
    });
    // teams in name order: the findings come out the same way on every run
    let mut by_team: std::collections::BTreeMap<&str, Vec<&Spawner>> = std::collections::BTreeMap::new();
    for s in &spawners {
        by_team.entry(s.1.as_str()).or_default().push(s);
    }
    let mut out = Vec::new();
    for list in by_team.values() {
        for (k, a) in list.iter().enumerate() {
            for b in list.iter().skip(k + 1) {
                let d = (0..3)
                    .map(|j| (a.2.get(j).copied().unwrap_or(0.0) - b.2.get(j).copied().unwrap_or(0.0)).powi(2))
                    .sum::<f32>()
                    .sqrt();
                if d > 1.0 {
                    continue;
                }
                let dice = base_at.as_ref().is_some_and(|m| {
                    m.get(&a.0).is_some_and(|p| same(*p, a.2)) && m.get(&b.0).is_some_and(|p| same(*p, b.2))
                });
                if dice {
                    continue;
                }
                out.push(
                    Finding::new(
                        "XVAL-PLACE-1",
                        format!(
                            "The vehicle spawners {} and {} ({}) stand {d:.1} m apart: their vehicles spawn into one \
                             another.",
                            a.0,
                            b.0,
                            if a.1.is_empty() { "no team" } else { a.1.as_str() }
                        ),
                    )
                    .naming(vec![a.0.clone(), b.0.clone()]),
                );
            }
        }
    }
    out
}

/// A project validated against the game copy as `mapexport validate` does it: the log applied to the base (its
/// failures XVAL-EDIT-1 to 3), then [`validate`] with the donor and the names' clashes read from the game.
///
/// # Errors
/// The base level's partition is not in the game copy.
pub fn project(game: &Game, p: &MapProject) -> crate::export::Result<Vec<Finding>> {
    let base_bytes = crate::export::level_partition(game, &p.base.level)?;
    let mut out = Vec::new();
    let r = p.base_report(&base_bytes);
    if r.changed {
        out.push(Finding::new(
            "XVAL-EDIT-1",
            format!(
                "Levels/{} is not the partition the project was made on; {} operation(s) name objects it lacks.",
                p.base.level,
                r.missing.len()
            ),
        ));
    }
    for (n, name, guid) in &r.missing {
        out.push(
            Finding::new(
                "XVAL-EDIT-2",
                format!("Operation {n} ({name}) names {guid}, which the level does not have."),
            )
            .naming(vec![guid.clone()]),
        );
    }
    let base = LevelWriter::parse(&base_bytes).ok();
    let schema = crate::Schema::scan(base.iter().map(|b| &b.d));
    let donors = |lv: &str| crate::export::level_partition(game, lv).ok();
    let base_lv = p.base.level.to_ascii_lowercase();
    let base_bundles = game.loaded(&base_lv, "level");
    // the log's other files (static group files) as the export reads them: the base's own
    let sources = |t: &Target| crate::export::base_level_file(game, &base_bundles, &base_lv, t);
    let level = match crate::files::apply_files(&base_bytes, &p.operations, &schema, &donors, &sources) {
        Ok(a) => LevelWriter::parse(&a.partition.bytes).ok(),
        Err(e) => {
            let m = e.to_string();
            out.push(Finding::new(crate::rules::apply_code(&m), m));
            None
        }
    };
    let donor = p
        .mode()
        .and_then(|c| crate::export::level_partition(game, &c.donor).ok())
        .map(|b| (crate::project::sha1_hex(&b), LevelWriter::parse(&b).ok()));
    let donor_ref = donor.as_ref().and_then(|(s, w)| w.as_ref().map(|w| (w, s.as_str())));
    let taken = crate::names::Taken::read(game);
    let langs = crate::names::Languages::read(game);
    out.extend(validate(Input {
        project: p,
        level: level.as_ref(),
        base: base.as_ref(),
        donor: donor_ref,
        game: Some(&taken),
        languages: Some(&langs),
        base_check: None,
        mode: None,
    }));
    Ok(out)
}

/// The findings of a stock level as it is (V1's proof: no stock level raises an error): its own mode and era.
///
/// # Errors
/// The level's partition is not in the game copy.
pub fn stock(
    game: &Game,
    lv: &str,
    profile: Profile,
    mode: Option<Mode>,
) -> crate::export::Result<(Option<Mode>, Vec<Finding>)> {
    let bytes = crate::export::level_partition(game, lv)?;
    let w = LevelWriter::parse(&bytes)?;
    let p = MapProject::new(lv, lv, &format!("Levels/{}", lv.to_ascii_uppercase()), &bytes, profile);
    let mode = mode.or_else(|| placement::detect_mode(&w));
    let f = validate(Input {
        project: &p,
        level: Some(&w),
        base: Some(&w),
        donor: None,
        game: None,
        languages: None,
        base_check: None,
        mode,
    });
    Ok((mode, f))
}
