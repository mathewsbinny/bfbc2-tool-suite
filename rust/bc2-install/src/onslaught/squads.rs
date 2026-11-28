//! A Napisal-level fix (round OR13): the port of Project OPFOR's `apply_combat_squads.py`'s plan and edit - combat
//! test B on Napisal's ONS_MP_002: its 28 enabled AI spawners put into squads with a combat readiness level, as
//! DICE's levels do.
//!
//! DICE's levels group AI spawners in `SquadEntityData` instances (Name, Enabled, SquadId, BaseReadiness,
//! SquadStartDispersed, Transform, Soldiers -> the spawners; each spawner's Squad array -> its squad) listed in
//! `LevelData.Objects`; Napisal's level has none. The edit: the enabled SSAiPlayer spawners in groups of [`GROUP`]
//! (nearest neighbours, deterministic); for each group the nearest of DICE's Xbox ONS_MP_002 squads, grafted from a
//! scratch copy of that file whose squads' soldier spawners were dropped (so the graft brings the squad alone),
//! BaseReadiness set to RLCombat, its Transform copied from the group's first spawner, the spawners added to its
//! Soldiers, the squad to each spawner's Squad array and to `LevelData.Objects`. The console level file is the user's
//! own (OPFOR's `work\xbox_dlc\o2`), never shipped.
//!
//! OPFOR's script keeps the command line, the install ledger's calls and the scratch clean-up; the work folder's
//! files are the retired Python's byte for byte (rust/BENCHMARKS.md "OR13").

use std::sync::LazyLock;

use regex::Regex;

use super::port::write_text_utf8;
use super::{OnsError, R, copy, dbx, fbrb, join, joins, pack, read_text};

/// Spawners per squad.
pub const GROUP: usize = 4;

#[allow(clippy::expect_used, reason = "constant patterns, compiled by the tests")]
static TRANSFORM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<complex name="Transform">([^<]*)</complex>"#).expect("pattern"));
#[allow(clippy::expect_used, reason = "constant patterns, compiled by the tests")]
static SOLDIERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<array name="Soldiers">(.*?)</array>"#).expect("pattern"));
#[allow(clippy::expect_used, reason = "constant patterns, compiled by the tests")]
static SQUAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<array name="Squad">(.*?)</array>"#).expect("pattern"));
#[allow(clippy::expect_used, reason = "constant patterns, compiled by the tests")]
static OBJECTS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<array name="Objects">(.*?)</array>"#).expect("pattern"));
#[allow(clippy::expect_used, reason = "constant patterns, compiled by the tests")]
static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"<item ref="([^"]*)""#).expect("pattern"));

/// The game copy's two level bundles of ONS_MP_002 (`BUNDLES`).
#[must_use]
pub fn bundles(game: &str) -> [String; 2] {
    [
        joins(game, &["Dist", "win32", "levels", "ons_mp_002", "level-00.fbrb"]),
        joins(game, &["Package", "levels", "ons_mp_002", "level-00.fbrb"]),
    ]
}

fn xml_of(path: &str) -> R<String> {
    dbx(&["xml", path])?;
    read_text(&format!("{}.xml", super::stem4(path)))
}

/// `float(s)` of a Transform's number.
fn py_float(s: &str) -> R<f64> {
    bc2_pyrt::py::strip(s)
        .parse::<f64>()
        .map_err(|_| OnsError::Data(format!("ValueError: could not convert string to float: '{s}'")))
}

/// `pos(body)`: the Transform's translation (0, 0, 0 without one).
fn pos(body: &str) -> R<[f64; 3]> {
    let Some(c) = TRANSFORM.captures(body) else {
        return Ok([0.0; 3]);
    };
    let v: Vec<f64> = c[1].split('/').map(py_float).collect::<R<_>>()?;
    let g = |i: usize| {
        v.get(i)
            .copied()
            .ok_or_else(|| OnsError::Data("IndexError: list index out of range".into()))
    };
    Ok([g(12)?, g(13)?, g(14)?])
}

/// A spawner: (guid, position).
pub type Spawner = (String, [f64; 3]);
/// A console squad: (guid, position, its soldiers' refs).
pub type Squad = (String, [f64; 3], Vec<String>);
/// One choice: (the squad, the group, the group's centre).
pub type Chosen = (Squad, Vec<Spawner>, [f64; 3]);

fn cmp_f(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// `plan(napisal_dbx)`: the level instance's GUID and each group's squad.
pub fn plan(napisal_dbx: &str, xbox_dbx: &str) -> R<(String, Vec<Chosen>)> {
    let t = xml_of(napisal_dbx)?;
    let inst = bc2_formats::xmlinst::instances(&t, None)
        .ok_or_else(|| OnsError::Data("an instance the reader does not read".into()))?;
    let level = inst
        .iter()
        .find(|(ty, _, _)| ty.ends_with(".LevelData"))
        .map(|(_, g, _)| (*g).to_owned())
        .ok_or_else(|| OnsError::Data("StopIteration".into()))?;
    let mut spawners: Vec<Spawner> = Vec::new();
    for (ty, g, b) in &inst {
        if ty.ends_with(".SoldierSpawnEntityData")
            && b.contains("SSAiPlayer")
            && b.contains("<field name=\"Enabled\">true</field>")
        {
            spawners.push(((*g).to_owned(), pos(b)?));
        }
    }
    // sorted by (z, x, guid): Python's sort is stable, so equal keys keep the document order
    spawners.sort_by(|a, b| cmp_f(a.1[2], b.1[2]).then(cmp_f(a.1[0], b.1[0])).then(a.0.cmp(&b.0)));
    if !spawners.len().is_multiple_of(GROUP) {
        return Err(OnsError::Exit(format!(
            "{} enabled AI spawners - not a multiple of {GROUP}",
            spawners.len()
        )));
    }
    let mut groups: Vec<Vec<Spawner>> = Vec::new();
    let mut left = spawners;
    while !left.is_empty() {
        let seed = left.remove(0);
        left.sort_by(|a, b| {
            cmp_f(bc2_pyrt::math::dist3(a.1, seed.1), bc2_pyrt::math::dist3(b.1, seed.1)).then(a.0.cmp(&b.0))
        });
        let mut grp = vec![seed];
        grp.extend(left.drain(..(GROUP - 1).min(left.len())));
        groups.push(grp);
    }
    let xt = xml_of(xbox_dbx)?;
    let xinst = bc2_formats::xmlinst::instances(&xt, None)
        .ok_or_else(|| OnsError::Data("an instance the reader does not read".into()))?;
    let mut squads: Vec<Squad> = Vec::new();
    for (ty, g, b) in &xinst {
        if ty.ends_with(".SquadEntityData") {
            let soldiers = SOLDIERS.captures(b).map_or_else(Vec::new, |m| {
                ITEM.captures_iter(&m[1]).map(|c| c[1].to_owned()).collect()
            });
            squads.push(((*g).to_owned(), pos(b)?, soldiers));
        }
    }
    let mut chosen: Vec<Chosen> = Vec::new();
    let mut used: Vec<String> = Vec::new();
    for grp in groups {
        #[allow(clippy::cast_precision_loss, reason = "a group of four")]
        let n = grp.len() as f64;
        let mut cx = [0.0f64; 3];
        for (k, c) in cx.iter_mut().enumerate() {
            // sum(p[1][k] for p in grp) / len(grp): from the int 0, in order
            *c = grp.iter().fold(0.0, |s, p| s + p.1[k]) / n;
        }
        // min((s for s in squads if s[0] not in used), key=(dist, guid)): the first of equal keys
        let mut best: Option<(&Squad, f64)> = None;
        for s in squads.iter().filter(|s| !used.contains(&s.0)) {
            let d = bc2_pyrt::math::dist3(s.1, cx);
            let better = match best {
                None => true,
                // Python's tuple order: the distance, then the GUID
                Some((b, bd)) => cmp_f(d, bd).then_with(|| s.0.cmp(&b.0)).is_lt(),
            };
            if better {
                best = Some((s, d));
            }
        }
        let Some((sq, _)) = best else {
            return Err(OnsError::Data("ValueError: min() iterable argument is empty".into()));
        };
        used.push(sq.0.clone());
        chosen.push((sq.clone(), grp, cx));
    }
    Ok((level, chosen))
}

/// `edit(napisal_dbx, out_dbx)`: the scratch console copy, the commands, DbxEdit's edit, the check; the choices.
pub fn edit(root: &str, napisal_dbx: &str, out_dbx: &str) -> R<Vec<Chosen>> {
    let xbox = xbox_dbx(root);
    let (level, chosen) = plan(napisal_dbx, &xbox)?;
    let work = joins(root, &["work", "combat_b"]);
    std::fs::create_dir_all(&work).map_err(|e| super::io_err(&work, &e))?;
    let scratch = join(&work, "xbox_squads.dbx");
    let mut prep: Vec<String> = Vec::new();
    for (sq, grp, _) in &chosen {
        let Some(first) = sq.2.first() else {
            return Err(OnsError::Data("IndexError: list index out of range".into()));
        };
        prep.extend(sq.2[1..].iter().map(|s| format!("drop={s}")));
        prep.push(format!("replace={}|{}", first.to_lowercase(), grp[0].0.to_lowercase()));
        prep.push(format!("replace={}|{}", first.to_uppercase(), grp[0].0.to_uppercase()));
    }
    let mut a: Vec<&str> = vec!["edit", &xbox, &scratch];
    a.extend(prep.iter().map(String::as_str));
    dbx(&a)?;
    let mut cmds: Vec<String> = Vec::new();
    for (sq, grp, _) in &chosen {
        let g = &sq.0;
        cmds.push(format!("graft={scratch}|{g}"));
        cmds.push(format!("setfield={g}|BaseReadiness|RLCombat"));
        cmds.push(format!("additem={level}|Objects|{g}"));
        cmds.push(format!("copyfield={napisal_dbx}|{}|Transform|{g}", grp[0].0));
        cmds.extend(grp[1..].iter().map(|(s, _)| format!("additem={g}|Soldiers|{s}")));
        cmds.extend(
            grp.iter()
                .map(|(s, _)| format!("copyfield={scratch}|{}|Squad|{s}", grp[0].0)),
        );
    }
    let cmd_file = join(&work, "commands.txt");
    write_text_utf8(&cmd_file, &(cmds.join("\n") + "\n"))?;
    let out = dbx(&["edit", napisal_dbx, out_dbx, &format!("@{cmd_file}")])?;
    if out.contains("NOT FOUND") {
        return Err(OnsError::Exit(format!("edit: {out}")));
    }
    check(out_dbx, &level, &chosen)?;
    Ok(chosen)
}

/// `check(dbx, level, chosen)`: every squad present with RLCombat, its soldiers, in `LevelData.Objects`; every
/// spawner's Squad array holding its squad.
fn check(path: &str, level: &str, chosen: &[Chosen]) -> R<()> {
    let x = xml_of(path)?;
    let inst = bc2_formats::xmlinst::instances(&x, None)
        .ok_or_else(|| OnsError::Data("an instance the reader does not read".into()))?;
    // {guid upper: body}: a later instance of a GUID wins, as the dict did
    let body = |g: &str| {
        let up = g.to_uppercase();
        inst.iter()
            .rev()
            .find(|(_, ig, _)| ig.to_uppercase() == up)
            .map(|(_, _, b)| *b)
    };
    let lv = body(level).ok_or_else(|| OnsError::Data(format!("KeyError: '{}'", level.to_uppercase())))?;
    let objects = OBJECTS
        .captures(lv)
        .ok_or_else(|| OnsError::Data("not found: Objects".into()))?[1]
        .to_uppercase();
    for (sq, grp, _) in chosen {
        let b = body(&sq.0);
        let Some(b) = b.filter(|b| b.contains("<field name=\"BaseReadiness\">RLCombat</field>")) else {
            return Err(OnsError::Exit(format!("check: squad {} missing or not RLCombat", sq.0)));
        };
        let s = SOLDIERS
            .captures(b)
            .ok_or_else(|| OnsError::Data("not found: Soldiers".into()))?;
        let mut soldiers: Vec<String> = ITEM.captures_iter(&s[1]).map(|c| c[1].to_uppercase()).collect();
        soldiers.sort();
        soldiers.dedup();
        let mut want: Vec<String> = grp.iter().map(|(g, _)| g.to_uppercase()).collect();
        want.sort();
        want.dedup();
        if soldiers != want {
            let shown: Vec<String> = soldiers.iter().map(|g| bc2_pyrt::fmt::str_repr(g)).collect();
            return Err(OnsError::Exit(format!(
                "check: squad {} soldiers {{{}}}",
                sq.0,
                shown.join(", ")
            )));
        }
        if !objects.contains(&sq.0.to_uppercase()) {
            return Err(OnsError::Exit(format!(
                "check: squad {} not in LevelData.Objects",
                sq.0
            )));
        }
        for (g, _) in grp {
            let sb = body(g).ok_or_else(|| OnsError::Data(format!("KeyError: '{}'", g.to_uppercase())))?;
            let has = SQUAD
                .captures(sb)
                .is_some_and(|m| m[1].to_uppercase().contains(&sq.0.to_uppercase()));
            if !has {
                return Err(OnsError::Exit(format!(
                    "check: spawner {g} Squad array does not hold {}",
                    sq.0
                )));
            }
        }
    }
    Ok(())
}

/// The user's console level file (`XBOX_DBX`).
#[must_use]
pub fn xbox_dbx(root: &str) -> String {
    joins(root, &["work", "xbox_dlc", "o2", "levels", "ONS_MP_002.dbx"])
}

/// `apply`'s bundle step after the ledger's check: the bundle unpacked into `work\combat_b\b<k>`, its level edited,
/// packed; (the file to install, the choices).
pub fn edit_bundle(root: &str, k: usize, bundle: &str) -> R<(String, Vec<Chosen>)> {
    let work = joins(root, &["work", "combat_b"]);
    let d = join(&work, &format!("b{k}"));
    let _ = std::fs::remove_dir_all(&d);
    fbrb(&["unpack", bundle, &d])?;
    let src = joins(&d, &["levels", "ONS_MP_002.dbx"]);
    let edited = join(&work, &format!("ONS_MP_002_b{k}.dbx"));
    let chosen = edit(root, &src, &edited)?;
    copy(&edited, &src)?;
    let packed = join(&work, &format!("level-00_b{k}.fbrb"));
    pack(&d, &packed)?;
    Ok((packed, chosen))
}

/// `format(x, '.0f')`.
fn f0(x: f64) -> String {
    format!("{x:.0}")
}

/// The lines `apply` prints after the install (`  squad ... <- ...`).
#[must_use]
pub fn chosen_lines(chosen: &[Chosen]) -> Vec<String> {
    chosen
        .iter()
        .map(|(sq, grp, c)| {
            let names: Vec<&str> = grp.iter().map(|(s, _)| s.as_str()).collect();
            format!(
                "  squad {} (Xbox, at {},{}; group centre {},{}) <- {}",
                sq.0,
                f0(sq.1[0]),
                f0(sq.1[2]),
                f0(c[0]),
                f0(c[2]),
                names.join(", ")
            )
        })
        .collect()
}

/// `--plan`: the Package bundle unpacked into `work\combat_b\plan`; the lines printed.
pub fn plan_lines(root: &str, game: &str) -> R<Vec<String>> {
    let d = joins(root, &["work", "combat_b", "plan"]);
    let _ = std::fs::remove_dir_all(&d);
    let [_, pkg] = bundles(game);
    fbrb(&["unpack", &pkg, &d])?;
    let (_, chosen) = plan(&joins(&d, &["levels", "ONS_MP_002.dbx"]), &xbox_dbx(root))?;
    Ok(chosen
        .iter()
        .map(|(sq, grp, c)| {
            let names: Vec<String> = grp.iter().map(|(s, _)| bc2_pyrt::fmt::str_repr(s)).collect();
            format!(
                "squad {} at {},{} ({} Xbox soldiers dropped in the scratch copy); group centre {},{}: [{}]",
                sq.0,
                f0(sq.1[0]),
                f0(sq.1[2]),
                sq.2.len(),
                f0(c[0]),
                f0(c[2]),
                names.join(", ")
            )
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn positions_and_lines() {
        let b = r#"<complex name="Transform">1/0/0/0/0/1/0/0/0/0/1/0/12.5/-3/-0.4/1</complex>"#;
        assert_eq!(pos(b).unwrap(), [12.5, -3.0, -0.4]);
        assert_eq!(pos("<field/>").unwrap(), [0.0; 3]);
        assert!(pos(r#"<complex name="Transform">1/2</complex>"#).is_err());
        let ch: Vec<Chosen> = vec![(
            ("SQ".into(), [2.5, 0.0, -0.4], vec![]),
            vec![("A".into(), [0.0; 3]), ("B".into(), [0.0; 3])],
            [3.5, 0.0, 1.5],
        )];
        assert_eq!(
            chosen_lines(&ch),
            ["  squad SQ (Xbox, at 2,-0; group centre 4,2) <- A, B"]
        );
    }
}
