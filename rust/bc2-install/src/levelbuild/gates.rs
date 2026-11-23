//! The gates a level passes before anything is installed (`wake_install.model_group_check`, `level_names`,
//! `icon_gate`): every member object of the level's model groups against what its group's physics indexes (a member
//! with other parts made the level load write out of range, 0xAB4E58), and every vehicle the level reaches with a
//! label in the HUD icon movie, no Name on a frame the icon build reclaimed.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

use super::index::{OurFile, StockIndex};
use super::{OMap, Result, bundle_read, read, rel_slash, to_xml, walk};

/// The physics packfile's magic, after a model group's wrapper.
pub const HAVOK_MAGIC: [u8; 8] = [0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10];

/// `GroupError`: a model group or member physics file without the expected shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupError(pub String);

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// `havok_parts(b)`: the part count of a physics resource's wrapper (its first two u32, equal; the packfile after
/// the part list).
///
/// # Errors
/// No wrapper.
pub fn havok_parts(b: &[u8]) -> std::result::Result<u32, GroupError> {
    if b.len() < 8 {
        return Err(GroupError("too short for a physics wrapper".into()));
    }
    let n = u32_at(b, 0).unwrap_or(0);
    let n2 = u32_at(b, 4).unwrap_or(0);
    let at = b.windows(8).position(|w| w == HAVOK_MAGIC);
    let at_shown = at.map_or(-1, |a| a as i64);
    if n != n2 || at.is_none() || 8 + 4 * u64::from(n) > at.unwrap_or(0) as u64 {
        return Err(GroupError(format!(
            "no physics wrapper (part counts {n} / {n2}, packfile at {at_shown})"
        )));
    }
    Ok(n)
}

/// `group_ids(b)`: the instance id per part of a model group's physics resource.
///
/// # Errors
/// No wrapper.
pub fn group_ids(b: &[u8]) -> std::result::Result<Vec<u32>, GroupError> {
    let n = havok_parts(b)? as usize;
    Ok((0..n).filter_map(|i| u32_at(b, 8 + 4 * i)).collect())
}

static ASSETS: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"(?s)<array name="Assets">(.*?)</array>"#).ok());
static ENUMS: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"<array name="Enums">([^<]*)</array>"#).ok());
static ITEM_REF: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"<item ref="([^"]+)""#).ok());

/// `group_members(x)`: (member partitions, instance ids) of a model group's `CompositeHavokAsset` XML.
///
/// # Errors
/// No `Assets` / `Enums` arrays, a count mismatch, or an id that is not a number (decided: a group problem; the
/// Python stopped the build with a `ValueError`).
pub fn group_members(x: &str) -> std::result::Result<(Vec<String>, Vec<i64>), GroupError> {
    let a = ASSETS.as_ref().and_then(|r| r.captures(x)).and_then(|c| c.get(1));
    let e = ENUMS.as_ref().and_then(|r| r.captures(x)).and_then(|c| c.get(1));
    let (Some(a), Some(e)) = (a, e) else {
        return Err(GroupError("no Assets / Enums arrays".into()));
    };
    let assets: Vec<String> = ITEM_REF.as_ref().map_or_else(Vec::new, |r| {
        r.captures_iter(a.as_str())
            .filter_map(|c| c.get(1))
            .map(|m| {
                let s = m.as_str();
                s.rsplit_once('/').map_or(s, |(h, _)| h).to_lowercase()
            })
            .collect()
    });
    let mut enums = Vec::new();
    for v in e.as_str().split('/') {
        let t = crate::pytext::strip(v);
        if t.is_empty() {
            continue;
        }
        enums.push(
            t.parse::<i64>()
                .map_err(|_| GroupError(format!("an instance id that is not a number ({t:?})")))?,
        );
    }
    if assets.len() != enums.len() {
        return Err(GroupError(format!(
            "{} assets but {} instance ids",
            assets.len(),
            enums.len()
        )));
    }
    Ok((assets, enums))
}

fn py_list(s: &BTreeSet<i64>) -> String {
    let v: Vec<String> = s.iter().map(i64::to_string).collect();
    format!("[{}]", v.join(", "))
}

/// `group_expects(assets, enums, ids)`: {member: parts per instance} the group's physics expects (in the order the
/// members first appear), and the problems of the group itself.
#[must_use]
pub fn group_expects(assets: &[String], enums: &[i64], ids: &[u32]) -> (Vec<(String, i64)>, Vec<String>) {
    let mut problems = Vec::new();
    let mut counts: HashMap<i64, i64> = HashMap::new();
    let mut order: Vec<i64> = Vec::new();
    for &i in ids {
        let i = i64::from(i);
        if order.last() != Some(&i) {
            order.push(i);
        }
        *counts.entry(i).or_insert(0) += 1;
    }
    if order != enums {
        problems.push(format!(
            "the physics part list is not the {} instances in order ({} runs)",
            enums.len(),
            order.len()
        ));
    }
    let mut want: OMap<BTreeSet<i64>> = OMap::new();
    for (a, e) in assets.iter().zip(enums) {
        let c = counts.get(e).copied().unwrap_or(0);
        if let Some(s) = want.get_mut(a) {
            s.insert(c);
        } else {
            want.insert(a, BTreeSet::from([c]));
        }
    }
    let mut out = Vec::new();
    for (a, s) in want.iter() {
        if s.len() != 1 || s.contains(&0) {
            problems.push(format!(
                "{a}: its instances have {} parts in the group physics",
                py_list(s)
            ));
        }
        out.push((a.to_owned(), s.iter().max().copied().unwrap_or(0)));
    }
    (out, problems)
}

/// `member_mismatch(...)`: `None` when a member's physics fits what its group expects (a forest group may index
/// fewer parts than the member has, never more), else the problem.
#[must_use]
pub fn member_mismatch(group: &str, forest: bool, member: &str, want: i64, have: u32, place: &str) -> Option<String> {
    let have = i64::from(have);
    if forest {
        return (want > have).then(|| {
            format!(
                "{group}: {member} ({place}) has {have} physics parts, the forest group indexes {want} per instance"
            )
        });
    }
    (want != have)
        .then(|| format!("{group}: {member} ({place}) has {have} physics parts, the group expects {want} per instance"))
}

/// `loaded_names(bundles, masked_by, masked)`: (bundle, {lower name: name}) of the loaded bundles that exist, their
/// live entries (`masked_by`'s deleted markers taken off the masked bundles).
#[must_use]
pub fn loaded_names(bundles: &[String], masked_by: &str, masked: &[String]) -> Vec<(String, HashMap<String, String>)> {
    let hide = if std::path::Path::new(masked_by).is_file() {
        super::index::masked_names(masked_by)
    } else {
        HashSet::new()
    };
    let mut out = Vec::new();
    for b in bundles {
        if !std::path::Path::new(b).is_file() {
            continue;
        }
        if let Ok(es) = super::index::live_entries(b, masked, &hide) {
            out.push((b.clone(), es.into_iter().map(|(n, _)| (n.to_lowercase(), n)).collect()));
        }
    }
    out
}

static GROUP_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    RegexBuilder::new(r"^levels/[^/]+/[^/]*(modelgroupentity|forestentity)[^/]*_havokasset\.dbx$")
        .case_insensitive(true)
        .build()
        .ok()
});

/// `model_group_check(level_dir, loaded, report)`: every member of the staged level's model groups against what
/// its group's physics expects - the member's physics as the install leaves it (the staged copy, else every copy the
/// loaded bundles have). True when there are groups and all fit; each problem reported.
///
/// # Errors
/// A file that cannot be read, a damaged partition.
pub fn model_group_check(
    level_dir: &str,
    loaded: &[(String, HashMap<String, String>)],
    report: &mut Vec<String>,
) -> Result<bool> {
    let mut staged: HashMap<String, String> = HashMap::new();
    for p in walk(level_dir) {
        staged.insert(rel_slash(&p, level_dir).to_lowercase(), p);
    }
    let mut groups: Vec<&String> = staged
        .keys()
        .filter(|k| GROUP_RE.as_ref().is_some_and(|r| r.is_match(k)))
        .collect();
    groups.sort_unstable();
    let mut problems: Vec<String> = Vec::new();
    let mut checked = 0;
    for g in &groups {
        let res = format!("{}.res", g.get(..g.len() - 4).unwrap_or_default());
        let forest = g.rsplit('/').next().unwrap_or_default().contains("forestentity");
        let got = (|| -> Result<std::result::Result<(Vec<(String, i64)>, Vec<String>), GroupError>> {
            let Some(rp) = staged.get(&res) else {
                return Ok(Err(GroupError("no physics resource".into())));
            };
            let x = to_xml(&read(staged.get(*g).map_or("", String::as_str))?)?;
            let (assets, enums) = match group_members(&x) {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            };
            let ids = match group_ids(&read(rp)?) {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            };
            Ok(Ok(group_expects(&assets, &enums, &ids)))
        })()?;
        let (want, own) = match got {
            Ok(v) => v,
            Err(GroupError(e)) => {
                problems.push(format!("{g}: {e}"));
                continue;
            }
        };
        problems.extend(own.into_iter().map(|p| format!("{g}: {p}")));
        let mut want = want;
        want.sort();
        for (member, n) in want {
            let low = format!("{member}.res");
            let mut copies: Vec<(String, Vec<u8>)> = Vec::new();
            if let Some(p) = staged.get(&low) {
                copies.push(("ours".into(), read(p)?));
            } else {
                for (b, names) in loaded {
                    if let Some(name) = names.get(&low) {
                        let place = crate::pytext::relpath(b, super::dirname(super::dirname(b)));
                        let mut d = bundle_read(b, std::slice::from_ref(name))?;
                        copies.push((place, d.remove(name).unwrap_or_default()));
                    }
                }
            }
            if copies.is_empty() {
                problems.push(format!(
                    "{g}: {member}: no physics resource in the level bundle or a bundle it loads"
                ));
                continue;
            }
            for (place, data) in copies {
                let p = match havok_parts(&data) {
                    Ok(have) => member_mismatch(g, forest, &member, n, have, &place),
                    Err(GroupError(e)) => Some(format!("{g}: {member} ({place}): {e}")),
                };
                problems.extend(p);
            }
            checked += 1;
        }
    }
    let state = if problems.is_empty() {
        "ok".to_owned()
    } else {
        format!("{} MISMATCH(ES)", problems.len())
    };
    report.push(format!(
        "model groups: {} groups, {checked} member objects checked against their group physics - {state}",
        groups.len()
    ));
    for p in problems.iter().take(30) {
        report.push(format!("  MISMATCH {p}"));
    }
    Ok(!groups.is_empty() && problems.is_empty())
}

/// `level_names(reached, ours, index)`'s result.
#[derive(Debug, Default)]
pub struct LevelNames {
    /// `VehicleEntityData` Names.
    pub vehicle: BTreeSet<String>,
    /// `SoldierWeaponData` Names of weapons someone holds.
    pub weapon: BTreeSet<String>,
    /// {Name: a partition naming it}.
    pub from: HashMap<String, String>,
    /// Weapons reached for a part only, held by nobody.
    pub unheld: BTreeSet<String>,
}

/// `level_names(reached, ours, index)`: the vehicle and weapon Names of the partitions reached (the reached
/// partitions in name order). A weapon counts only when some other reached partition names its partition's
/// `SoldierWeaponAsset`.
///
/// # Errors
/// A damaged partition.
pub fn level_names(reached: &[String], ours: &OMap<OurFile>, index: &StockIndex) -> Result<LevelNames> {
    let mut data_of: Vec<(String, Vec<u8>)> = Vec::new();
    let mut stock: OMap<Vec<(String, String)>> = OMap::new();
    let mut sorted: Vec<&String> = reached.iter().collect();
    sorted.sort_unstable();
    for low in sorted {
        let k = format!("{low}.dbx");
        if let Some((_, p)) = ours.get(&k) {
            if let Ok(d) = std::fs::read(p) {
                data_of.push((low.clone(), d));
            }
        } else if let Some((b, n, _)) = index.first(&k) {
            if !stock.contains(b) {
                stock.insert(b, Vec::new());
            }
            if let Some(v) = stock.get_mut(b) {
                v.push((low.clone(), n.to_owned()));
            }
        }
    }
    for (b, items) in stock.iter() {
        let names: Vec<String> = items.iter().map(|(_, n)| n.clone()).collect();
        let Ok(mut got) = bundle_read(b, &names) else { continue };
        for (low, n) in items {
            if let Some(d) = got.remove(n) {
                data_of.push((low.clone(), d));
            }
        }
    }
    let mut out = LevelNames::default();
    let mut assets: OMap<Vec<Vec<u8>>> = OMap::new();
    let name_re = Regex::new(r#"<field name="Name">([^<]*)</field>"#).map_err(|e| e.to_string())?;
    let swa = Regex::new(r#"<instance guid="([^"]+)" type="GameSharedResources\.SoldierWeaponAsset""#)
        .map_err(|e| e.to_string())?;
    for (low, data) in &data_of {
        if !(low.starts_with("objects/vehicles/") || low.starts_with("objects/weapons/")) {
            continue;
        }
        let has = |needle: &[u8]| data.windows(needle.len()).any(|w| w == needle);
        if !has(b"VehicleEntityData") && !has(b"SoldierWeaponData") {
            continue;
        }
        let x = to_xml(data)?;
        for (t, key) in [("VehicleEntityData", "vehicle"), ("SoldierWeaponData", "weapon")] {
            let body_re = Regex::new(&format!(r#"(?s)type="GameSharedResources\.{t}"[^>]*>(.*?)</instance>"#))
                .map_err(|e| e.to_string())?;
            for c in body_re.captures_iter(&x) {
                let body = c.get(1).map_or("", |m| m.as_str());
                let Some(nm) = name_re.captures(body).and_then(|c| c.get(1)).map(|m| m.as_str()) else {
                    continue;
                };
                if nm.is_empty() {
                    continue;
                }
                if key == "vehicle" {
                    out.vehicle.insert(nm.to_owned());
                } else {
                    out.weapon.insert(nm.to_owned());
                }
                out.from.entry(nm.to_owned()).or_insert_with(|| low.clone());
                if key == "weapon" {
                    for g in swa.captures_iter(&x).filter_map(|c| c.get(1)) {
                        let r = format!("{low}/{}", g.as_str().to_lowercase()).into_bytes();
                        if let Some(v) = assets.get_mut(nm) {
                            if !v.contains(&r) {
                                v.push(r);
                            }
                        } else {
                            assets.insert(nm, vec![r]);
                        }
                    }
                }
            }
        }
    }
    for (name, refs) in assets.iter() {
        let held = data_of.iter().any(|(low, data)| {
            let own = format!("{low}/").into_bytes();
            refs.iter()
                .filter(|r| !r.starts_with(&own))
                .any(|r| data.windows(r.len()).any(|w| w == r.as_slice()))
        });
        if !held {
            out.weapon.remove(name);
            out.unheld.insert(name.to_owned());
        }
    }
    Ok(out)
}

/// `icon_gate(names, movie, report, reclaimed, reclaim_re)`: true when every vehicle Name has a label in the HUD icon
/// movie's `WeaponIcon` timeline (character 483) and no Name the level uses is on a reclaimed frame.
///
/// # Errors
/// The movie cannot be read, or the pattern does not compile.
pub fn icon_gate(
    names: &LevelNames,
    movie: &str,
    reclaimed: &[String],
    reclaim_re: Option<&str>,
    report: &mut Vec<String>,
) -> Result<bool> {
    let mut m = bc2_menu::graft::load(movie).map_err(|e| format!("{movie}: {e}"))?;
    let o = m.char(483).map_err(|e| format!("{movie}: {e}"))?;
    let missing: Vec<&String> = names.vehicle.iter().filter(|n| m.label_frame(n, o).is_err()).collect();
    let re = reclaim_re
        .map(Regex::new)
        .transpose()
        .map_err(|e| format!("reclaim pattern: {e}"))?;
    let used: BTreeSet<&String> = names.vehicle.iter().chain(&names.weapon).collect();
    let clash: Vec<&String> = used
        .into_iter()
        .filter(|n| reclaimed.contains(n) || re.as_ref().is_some_and(|r| r.is_match(n)))
        .collect();
    let mut line = format!(
        "icons: {} vehicle Names, {} without a WeaponIcon label",
        names.vehicle.len(),
        missing.len()
    );
    if !missing.is_empty() {
        let v: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
        line.push_str(&format!(" ({})", v.join(", ")));
    }
    line.push_str(&format!("; {} Names on reclaimed frames", clash.len()));
    if !clash.is_empty() {
        let v: Vec<String> = clash
            .iter()
            .map(|n| format!("{n} in {}", names.from.get(*n).map_or("?", String::as_str)))
            .collect();
        line.push_str(&format!(" ({})", v.join(", ")));
    }
    if !names.unheld.is_empty() {
        let v: Vec<&str> = names.unheld.iter().map(String::as_str).collect();
        line.push_str(&format!(
            "; weapons reached for a part only, held by nobody: {}",
            v.join(", ")
        ));
    }
    report.push(line);
    Ok(missing.is_empty() && clash.is_empty())
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::assert_is_empty,
    reason = "test assertions"
)]
mod tests {
    use super::*;

    fn wrapper(ids: &[u32]) -> Vec<u8> {
        let n = ids.len() as u32;
        let mut b = Vec::new();
        b.extend(n.to_le_bytes());
        b.extend(n.to_le_bytes());
        for i in ids {
            b.extend(i.to_le_bytes());
        }
        b.extend(HAVOK_MAGIC);
        b
    }

    #[test]
    fn physics_wrappers() {
        assert_eq!(havok_parts(&wrapper(&[1, 1, 2])), Ok(3));
        assert_eq!(group_ids(&wrapper(&[7, 7, 9])), Ok(vec![7, 7, 9]));
        assert_eq!(havok_parts(b"short").unwrap_err().0, "too short for a physics wrapper");
        let mut bad = wrapper(&[1]);
        bad[4] = 2;
        assert_eq!(
            havok_parts(&bad).unwrap_err().0,
            "no physics wrapper (part counts 1 / 2, packfile at 12)"
        );
        assert_eq!(
            havok_parts(&[0, 0, 0, 0, 0, 0, 0, 0, 1]).unwrap_err().0,
            "no physics wrapper (part counts 0 / 0, packfile at -1)"
        );
    }

    #[test]
    fn group_expectations() {
        let assets = vec!["a".to_owned(), "b".to_owned(), "a".to_owned()];
        let (want, problems) = group_expects(&assets, &[1, 2, 3], &[1, 1, 2, 3, 3]);
        assert_eq!(want, [("a".to_owned(), 2), ("b".to_owned(), 1)]);
        assert!(problems.is_empty());
        let (_, problems) = group_expects(&assets, &[1, 2, 3], &[1, 2, 3, 3]);
        assert_eq!(problems, ["a: its instances have [1, 2] parts in the group physics"]);
        let (_, problems) = group_expects(&assets, &[1, 2, 3], &[2, 1, 3]);
        assert_eq!(
            problems[0],
            "the physics part list is not the 3 instances in order (3 runs)"
        );
        assert_eq!(member_mismatch("g", true, "m", 3, 13, "ours"), None);
        assert!(member_mismatch("g", false, "m", 3, 13, "ours").is_some());
    }

    #[test]
    fn group_member_arrays() {
        let x = r#"<array name="Assets">
<item ref="Objects/A/a_havok/0000" />
<item ref="Objects/B/b/1111" /></array><array name="Enums">4/ 5 /</array>"#;
        let (a, e) = group_members(x).unwrap();
        assert_eq!(a, ["objects/a/a_havok", "objects/b/b"]);
        assert_eq!(e, [4, 5]);
        assert_eq!(group_members("<x/>").unwrap_err().0, "no Assets / Enums arrays");
        assert!(group_members(r#"<array name="Assets"></array><array name="Enums">x</array>"#).is_err());
    }
}
