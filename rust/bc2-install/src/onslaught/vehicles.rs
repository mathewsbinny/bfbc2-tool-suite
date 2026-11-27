//! Stage 5 of the console Onslaught port (round OR10): the port of Project OPFOR's `console_vehicles.py`'s `build`
//! and `apply` up to the ledger - every vehicle spawn whose vehicle has no physics asset (its mesh and physics were
//! console resources, so stage 1 left the blueprint with every `Asset` reference null) dropped from the level, with
//! the vehicle's own partitions the spawn alone reached.
//!
//! Why (automated load test 2026-10-30): Valparaiso crashed at 0x5A5A61 creating its console Mark V patrol boat
//! (`objects/vehicles/sea/markv/markv_ons`: its mesh and Havok data are console builds); the console's other Onslaught
//! variants name the PC vehicles' physics and are kept. DbxEdit's `drop` removes the spawner and every array item
//! naming it; single references to it become null. The loader processes every partition of a bundle, and the
//! mesh-less blueprint crashed it with its spawn gone (second test): the blueprint, then what it reaches that bears
//! the vehicle's name (firing data, wreck, destruction effect) once nothing else names it, go too.
//!
//! The work folder's files are the Python's byte for byte (rust/BENCHMARKS.md "OR10").

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use super::port::{StageOut, drop_xml, find, manifest_lines, set_sizes, under, write_manifest, xml_of};
use super::{R, Source, dbx, fbrb, join, joins, pack, rmtree, size, write};

fn re(p: &str) -> Regex {
    #[allow(
        clippy::expect_used,
        reason = "the patterns are constants, each compiled by the tests"
    )]
    Regex::new(p).expect("a constant pattern")
}

static VEHICLE: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([A-Za-z0-9_/\-]+)/[0-9A-Fa-f-]{36}" name="Vehicle""#));
static ASSET: LazyLock<Regex> = LazyLock::new(|| re(r#"<field ref="([^"]+)" name="Asset" />"#));
static REF: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([A-Za-z0-9_/\-]+)/[0-9A-Fa-f-]{36}""#));

/// What stage 5 is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder: its `work\console_vehicles\` is written.
    pub root: String,
    /// The game copy: read only.
    pub game: String,
    /// The Onslaught level (lower case).
    pub lv: String,
    /// The bundle to build from (the ledger's `source`: the stage's own prior when it redoes its write).
    pub src_bundle: String,
    /// `--apply` (the bundle packed when something was dropped) rather than `--plan`.
    pub write: bool,
}

fn refs(text: &str) -> HashSet<String> {
    REF.captures_iter(text).map(|c| c[1].to_lowercase()).collect()
}

/// The partitions `gone` with the spawns of `report` dropped: per vehicle the partitions named like it, then what they
/// reach that bears its name's first word once no partition still there names it (`console_vehicles.build`'s loop).
fn gone_partitions(refs_of: &HashMap<String, HashSet<String>>, report: &[String]) -> HashSet<String> {
    let mut gone: HashSet<String> = HashSet::new();
    for r in report {
        let stem = r.split(' ').next().unwrap_or("");
        let first = stem.split('_').next().unwrap_or("");
        let mut cand: HashSet<String> = refs_of
            .keys()
            .filter(|p| p.rsplit('/').next() == Some(stem))
            .cloned()
            .collect();
        while !cand.is_empty() {
            gone.extend(cand.iter().cloned());
            let mut reached: HashSet<&String> = HashSet::new();
            #[allow(clippy::iter_over_hash_type, reason = "collected into a set")]
            for p in &cand {
                if let Some(s) = refs_of.get(p) {
                    reached.extend(s.iter());
                }
            }
            #[allow(clippy::iter_over_hash_type, reason = "collected into a set")]
            let next: HashSet<String> = reached
                .into_iter()
                .filter(|p| {
                    refs_of.contains_key(*p)
                        && !gone.contains(*p)
                        && p.contains(first)
                        && !refs_of.iter().any(|(q, s)| !gone.contains(q) && s.contains(*p))
                })
                .cloned()
                .collect();
            cand = next;
        }
    }
    gone
}

/// `console_vehicles.build(lv)` and, with `write`, `apply`'s pack.
pub fn build(a: &Args) -> R<StageOut> {
    let lv = a.lv.as_str();
    let name = lv.to_uppercase();
    let work = joins(&a.root, &["work", "console_vehicles"]);
    let bundle = joins(&a.game, &["Package", "levels", lv, "level-00.fbrb"]);
    let src = joins(&work, &[lv, "pkg"]);
    rmtree(&src)?;
    fbrb(&["unpack", &a.src_bundle, &src])?;
    let level = find(&src, &format!("Levels/{name}.dbx"))?;
    let lt = xml_of(&level)?;
    let mut drops: Vec<String> = Vec::new();
    let mut report: Vec<String> = Vec::new();
    for c in super::mode::INST.captures_iter(&lt) {
        if !c[3].ends_with("VehicleSpawnEntityData") {
            continue;
        }
        let Some(v) = VEHICLE.captures(&c[4]) else {
            continue;
        };
        let part = v[1].to_lowercase();
        // a vehicle another bundle holds (a PC vehicle from a shared bundle) is fine: the Python's SystemExit
        // (no such file, or DbxEdit failing on it) skipped it
        let Ok(vx) = find(&src, &format!("{part}.dbx")).and_then(|f| xml_of(&f)) else {
            continue;
        };
        let assets: Vec<String> = ASSET.captures_iter(&vx).map(|m| m[1].to_owned()).collect();
        if !assets.is_empty() && assets.iter().all(|x| x == "null") {
            drops.push(c[2].to_owned());
            report.push(format!("{} spawn {}", part.rsplit('/').next().unwrap_or(""), &c[2]));
        }
    }
    let mut out = StageOut::default();
    out.printed.push(format!(
        "{lv}: {} vehicle spawns without a buildable vehicle{}",
        drops.len(),
        if drops.is_empty() {
            String::new()
        } else {
            format!(" - dropped: {}", report.join(", "))
        }
    ));
    // the line stays printed when what follows fails (the Python printed as it went)
    if let Err(e) = drop_spawns(a, &src, &level, &drops, &report, &mut out) {
        out.fail = Some(e.to_string());
        return Ok(out);
    }
    if a.write && !drops.is_empty() {
        let built = joins(&work, &[lv, "level-00.fbrb"]);
        if let Err(e) = pack(&src, &built) {
            out.fail = Some(e.to_string());
            return Ok(out);
        }
        out.writes.push((bundle, Source::File(built)));
    }
    Ok(out)
}

/// The spawns dropped from the level, the vehicles' partitions with them, the manifest; the XML deleted.
fn drop_spawns(a: &Args, src: &str, level: &str, drops: &[String], report: &[String], out: &mut StageOut) -> R<()> {
    let lv = a.lv.as_str();
    let src = src.to_owned();
    let level = level.to_owned();
    if !drops.is_empty() {
        let new = format!("{level}.new");
        let mut cmd: Vec<String> = vec!["edit".into(), level.clone(), new.clone()];
        cmd.extend(drops.iter().map(|g| format!("drop={g}")));
        let cmd: Vec<&str> = cmd.iter().map(String::as_str).collect();
        dbx(&cmd)?;
        write(&level, &super::read(&new)?)?;
        std::fs::remove_file(&new).map_err(|e| super::io_err(&new, &e))?;
        let man = join(&src, "fbrb.manifest");
        let mut lines = manifest_lines(&man)?;
        let level_rel = format!("levels/{lv}.dbx");
        let mut refs_of: HashMap<String, HashSet<String>> = HashMap::new();
        for l in &lines {
            let n = l.split('|').next().unwrap_or("");
            let low = n.to_lowercase();
            if low.ends_with(".dbx") && low != level_rel {
                let p = under(&src, n);
                let key = super::stem4(n).to_lowercase();
                let v = if std::path::Path::new(&p).is_file() {
                    refs(&xml_of(&p)?)
                } else {
                    HashSet::new()
                };
                refs_of.insert(key, v);
            }
        }
        refs_of.insert(format!("levels/{lv}"), refs(&xml_of(&level)?));
        let gone = gone_partitions(&refs_of, report);
        lines.retain(|l| {
            let n = l.split('|').next().unwrap_or("");
            !(n.to_lowercase().ends_with(".dbx") && gone.contains(&super::stem4(n).to_lowercase()))
        });
        let mut sorted: Vec<&String> = gone.iter().collect();
        sorted.sort();
        for p in sorted {
            let f = format!("{}.dbx", under(&src, p));
            if std::path::Path::new(&f).is_file() {
                std::fs::remove_file(&f).map_err(|e| super::io_err(&f, &e))?;
            }
        }
        // sorted(... for p in gone): a basename two partitions share is listed twice
        let mut shown: Vec<&str> = gone.iter().map(|p| p.rsplit('/').next().unwrap_or("")).collect();
        shown.sort_unstable();
        out.printed.push(format!("  partitions removed: {}", shown.join(", ")));
        let level_size = size(&level)?;
        set_sizes(&mut lines, &[level_rel.as_str()], &|_| Ok(level_size))?;
        write_manifest(&man, &lines)?;
    }
    drop_xml(&src)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        let _ = (&*VEHICLE, &*ASSET, &*REF);
        let body = r#"<field ref="objects/vehicles/sea/markv/markv_ons/0f2d8c1e-0000-0000-0000-000000000001" name="Vehicle" />"#;
        assert_eq!(
            &VEHICLE.captures(body).unwrap()[1],
            "objects/vehicles/sea/markv/markv_ons"
        );
    }

    #[test]
    fn gone_follows_the_vehicle_name() {
        let mut refs_of: HashMap<String, HashSet<String>> = HashMap::new();
        let set = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<HashSet<String>>();
        refs_of.insert(
            "objects/v/markv_ons".into(),
            set(&["objects/v/markv_wreck", "objects/w/gun"]),
        );
        refs_of.insert("objects/v/markv_wreck".into(), set(&[]));
        refs_of.insert("objects/w/gun".into(), set(&[]));
        refs_of.insert("objects/v/markv_fx".into(), set(&[]));
        refs_of.insert("levels/ons_mp_002".into(), set(&["objects/v/markv_fx"]));
        let gone = gone_partitions(&refs_of, &["markv_ons spawn X".to_owned()]);
        let mut g: Vec<&String> = gone.iter().collect();
        g.sort();
        assert_eq!(g, ["objects/v/markv_ons", "objects/v/markv_wreck"]);
    }
}
