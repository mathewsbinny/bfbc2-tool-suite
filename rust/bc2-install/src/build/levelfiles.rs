//! The level's own partitions and the copies of ours the install changes (`wake_install.edit_level`, `team_refs`,
//! `drop_instances`, `test_drop`, `test_spawn`, `vo_from`, `remap_targets`, `patch_refs`, `persistence_ids`,
//! `weapon_persistence`, `patch_ours`, `kit_voices`, `sky_patch`, `root_files`, `mesh_flags`, `ondemand_waves`,
//! `emitter_atlas`, `undergrowth_files`).
//!
//! The partitions are edited as element trees (`bc2_formats::dbx`, the model `bfbc2core.dbx` reads into) and
//! written with a fresh string table, as `dbx.write` writes them.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use bc2_formats::dbx::{self, Body, CONTAINER, DbxFile, Element, TEXT};
use regex::Regex;

use super::closure::{Src, Take};
use super::index::{OurFile, StockIndex, primary_type};
use super::{OMap, Result, bundle_one, bundle_read, pjoin, read, to_xml, write};

// ------------------------------------------------------------------------------------------------ the tree

fn parse(data: &[u8]) -> Result<DbxFile> {
    dbx::parse(data).map_err(|e| format!("ValueError: {e}"))
}

fn dbx_write(f: &DbxFile) -> Result<Vec<u8>> {
    dbx::write(f).map_err(|e| format!("ValueError: {e}"))
}

fn attr<'a>(f: &'a DbxFile, e: &Element, name: &str) -> Option<&'a str> {
    f.attr(e, name)
}

/// Every attribute `name` of `e` set to `value` (`e.attributes = [(k, value if k == name else v) ...]`).
fn set_attr(strings: &mut DbxFile, e: &mut Element, name: &str, value: &str) {
    let v = strings.intern(value);
    for (k, val) in &mut e.attributes {
        if strings.str(*k) == name {
            *val = v;
        }
    }
}

fn kids(e: &Element) -> &[Element] {
    e.children()
}

fn kids_mut(e: &mut Element) -> Option<&mut Vec<Element>> {
    match &mut e.body {
        Body::Container(c) => Some(c),
        _ => None,
    }
}

fn child_at(f: &DbxFile, e: &Element, name: &str) -> Option<usize> {
    kids(e).iter().position(|c| attr(f, c, "name") == Some(name))
}

/// The partition's instance positions among its children.
fn instance_at(f: &DbxFile) -> Vec<usize> {
    f.roots
        .first()
        .map(|p| {
            kids(p)
                .iter()
                .enumerate()
                .filter(|(_, c)| f.tag(c) == "instance")
                .map(|(i, _)| i)
                .collect()
        })
        .unwrap_or_default()
}

/// Runs `edit` on the file's roots taken out of it (so the file's string table can grow while they change).
fn with_roots<T>(f: &mut DbxFile, edit: impl FnOnce(&mut DbxFile, &mut Vec<Element>) -> T) -> T {
    let mut roots = std::mem::take(&mut f.roots);
    let out = edit(f, &mut roots);
    f.roots = roots;
    out
}

/// `copy.deepcopy` of an element of `src` into `dst`'s table.
fn copy_el(dst: &mut DbxFile, src: &DbxFile, e: &Element) -> Element {
    let tag = dst.intern(src.str(e.tag));
    let attributes = e
        .attributes
        .iter()
        .map(|(k, v)| (dst.intern(src.str(*k)), dst.intern(src.str(*v))))
        .collect();
    let body = match &e.body {
        Body::Container(c) => Body::Container(c.iter().map(|x| copy_el(dst, src, x)).collect()),
        Body::Text(t) => Body::Text(dst.intern(src.str(*t))),
        Body::Numbers(r) => Body::Numbers(r.clone()),
        Body::Scalar(r) => Body::Scalar(*r),
    };
    Element {
        tag,
        kind: e.kind,
        attributes,
        body,
    }
}

/// `dbx.value(c) == 'true'`.
fn is_true(f: &DbxFile, e: &Element) -> bool {
    dbx::value(f, e).is_ok_and(|v| v == "true")
}

/// A bool's value byte set to false (`c.raw = c.raw[:1] + b'\x00'`).
fn set_false(e: &mut Element) {
    if let Body::Scalar(r) = &mut e.body {
        r[1] = 0;
    }
}

// ------------------------------------------------------------------------------------------------ remaps

/// `drop_instances(f, guids)`: the instances left out of the partition, array items naming them removed, fields
/// naming them null.
pub fn drop_instances(f: &mut DbxFile, guids: &[String]) {
    let gl: HashSet<String> = guids.iter().map(|g| g.to_lowercase()).collect();
    with_roots(f, |f, roots| {
        let Some(part) = roots.first_mut() else { return };
        if let Some(c) = kids_mut(part) {
            c.retain(|x| !gl.contains(&attr(f, x, "guid").unwrap_or_default().to_lowercase()));
        }
        drop_walk(f, part, &gl);
    });
}

fn drop_walk(f: &mut DbxFile, e: &mut Element, gl: &HashSet<String>) {
    let Some(c) = kids_mut(e) else { return };
    if c.is_empty() {
        return;
    }
    c.retain(|x| !(f.tag(x) == "item" && gl.contains(&attr(f, x, "ref").unwrap_or_default().to_lowercase())));
    for x in c.iter_mut() {
        if gl.contains(&attr(f, x, "ref").unwrap_or_default().to_lowercase()) {
            set_attr(f, x, "ref", "null");
        }
        drop_walk(f, x, gl);
    }
}

/// What `remap_targets` maps (OPFOR's `REMAP`, in its order) and the ported muzzle effects that keep their own.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct RemapSpec {
    /// (old partition, new partition or `None`: the reference nulled).
    pub remap: Vec<(String, Option<String>)>,
    /// `PORTED_EFFECT_FALLBACKS`: an old partition our builds have is ours (checked: an effect root).
    pub fallbacks: Vec<String>,
    /// `REMAP_REFS`: single references (partition/guid, lower case) to set (`None`: null).
    pub refs: Vec<(String, Option<String>)>,
}

/// `remap_targets(index, ours)`: {old partition: `new/guid` (the target's primary instance) or `None`}, in the
/// remap's order.
///
/// # Errors
/// A target that cannot be read, or a ported muzzle effect without an effect root (`ValueError`).
pub fn remap_targets(
    spec: &RemapSpec,
    index: &StockIndex,
    ours: &OMap<OurFile>,
) -> Result<Vec<(String, Option<String>)>> {
    let mut out = Vec::new();
    for (old, new) in &spec.remap {
        if spec.fallbacks.contains(old)
            && let Some((_, p)) = ours.get(&format!("{old}.dbx"))
        {
            let (kind, guid) = primary_type(&read(p)?)?;
            let ok = matches!(kind.as_deref(), Some("Entity.EffectAsset" | "Entity.EffectEntityData"));
            if !ok || guid.as_deref().is_none_or(str::is_empty) {
                return Err(format!(
                    "ValueError: Ported muzzle effect lacks a valid effect root: {old}"
                ));
            }
            continue;
        }
        let Some(new) = new else {
            out.push((old.clone(), None));
            continue;
        };
        let k = format!("{new}.dbx");
        let data = if let Some((_, p)) = ours.get(&k) {
            read(p)?
        } else {
            let (b, n, _) = index.first(&k).ok_or_else(|| format!("KeyError: '{k}'"))?;
            bundle_one(b, n)?
        };
        let (_, g) = primary_type(&data)?;
        let g = g.ok_or_else(|| format!("AttributeError: {k} has no primary instance"))?;
        out.push((old.clone(), Some(format!("{new}/{}", g.to_lowercase()))));
    }
    Ok(out)
}

/// `patch_refs(data, to)`: the partition with its references remapped (`to`: {partition: `name/guid` or `None`};
/// `refs`: single references) and how many - the bytes as they were when none was.
///
/// # Errors
/// A damaged partition.
pub fn patch_refs(
    data: &[u8],
    to: &HashMap<String, Option<String>>,
    refs: &HashMap<String, Option<String>>,
) -> Result<(Vec<u8>, usize)> {
    let mut f = parse(data)?;
    let at = instance_at(&f);
    let n = with_roots(&mut f, |f, roots| {
        let mut n = 0;
        if let Some(Body::Container(c)) = roots.first_mut().map(|p| &mut p.body) {
            for i in at {
                if let Some(inst) = c.get_mut(i) {
                    patch_walk(f, inst, to, refs, &mut n);
                }
            }
        }
        n
    });
    if n == 0 {
        return Ok((data.to_vec(), 0));
    }
    Ok((dbx_write(&f)?, n))
}

fn patch_walk(
    f: &mut DbxFile,
    e: &mut Element,
    to: &HashMap<String, Option<String>>,
    refs: &HashMap<String, Option<String>>,
    n: &mut usize,
) {
    if let Some(r) = attr(f, e, "ref").filter(|r| !r.is_empty()).map(str::to_owned) {
        let low = r.to_lowercase();
        if let Some(new) = refs.get(&low) {
            let v = new.clone().unwrap_or_else(|| "null".into());
            set_attr(f, e, "ref", &v);
            *n += 1;
        } else if let Some((head, _)) = r.rsplit_once('/')
            && let Some(new) = to.get(&head.to_lowercase())
        {
            let v = new.clone().unwrap_or_else(|| "null".into());
            set_attr(f, e, "ref", &v);
            *n += 1;
        }
    }
    if let Some(c) = kids_mut(e) {
        for x in c.iter_mut() {
            patch_walk(f, x, to, refs, n);
        }
    }
}

static PERSISTENT: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#"(?s)<instance guid="([^"]+)" type="GameSharedResources.PersistentWeapon"[^>]*>(.*?)</instance>"#).ok()
});
static ID_FIELD: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"name="Id">([^<]*)<"#).ok());

/// `persistence_ids(index)`: {`PersistentWeapon` Id: `persistence/weapons/<guid>`} of the game's
/// `Persistence/Weapons` (the first stock bundle holding it).
///
/// # Errors
/// In no stock bundle, or an instance without its Id.
pub fn persistence_ids(index: &StockIndex) -> Result<HashMap<String, String>> {
    for b in &index.bundles {
        if let Some((n, _)) = b.get("persistence/weapons.dbx") {
            let x = to_xml(&bundle_one(&b.path, n)?)?;
            let mut out = HashMap::new();
            if let Some(r) = PERSISTENT.as_ref() {
                for c in r.captures_iter(&x) {
                    let g = c.get(1).map_or("", |m| m.as_str());
                    let body = c.get(2).map_or("", |m| m.as_str());
                    let id = ID_FIELD
                        .as_ref()
                        .and_then(|r| r.captures(body))
                        .and_then(|c| c.get(1))
                        .ok_or_else(|| format!("AssertionError: no 'name=\"Id\">([^<]*)<' in the text ({g})"))?;
                    out.insert(
                        id.as_str().to_owned(),
                        format!("persistence/weapons/{}", g.to_lowercase()),
                    );
                }
            }
            return Ok(out);
        }
    }
    Err("Persistence/Weapons: in no stock bundle".into())
}

/// `weapon_persistence(data, ref)`: the partition's `SoldierWeaponData` with `Persistence` = ref, and how many changed.
///
/// # Errors
/// A damaged partition.
pub fn weapon_persistence(data: &[u8], r: &str) -> Result<(Vec<u8>, usize)> {
    let mut f = parse(data)?;
    let at = instance_at(&f);
    let n = with_roots(&mut f, |f, roots| {
        let mut n = 0;
        if let Some(Body::Container(c)) = roots.first_mut().map(|p| &mut p.body) {
            for i in at {
                let Some(inst) = c.get_mut(i) else { continue };
                if attr(f, inst, "type") != Some("GameSharedResources.SoldierWeaponData") {
                    continue;
                }
                let Some(k) = child_at(f, inst, "Persistence") else {
                    continue;
                };
                if let Some(ch) = kids_mut(inst).and_then(|c| c.get_mut(k))
                    && attr(f, ch, "ref") != Some(r)
                {
                    set_attr(f, ch, "ref", r);
                    n += 1;
                }
            }
        }
        n
    });
    if n == 0 {
        return Ok((data.to_vec(), 0));
    }
    Ok((dbx_write(&f)?, n))
}

fn put_patched(top: &str, rel: &str, data: &[u8]) -> Result<String> {
    let p = pjoin(top, rel);
    write(&p, data)?;
    Ok(p)
}

/// `patch_ours(ours, index, report)`: our partitions with the remaps applied and the WW2 weapons given their
/// unlock entries (`weapons`: {BFBC2 PersistentWeapon Id: our weapons' partition stems}, in order), as copies in
/// `patched` (ours then names them).
///
/// # Errors
/// A partition that cannot be read or written, or a weapon not among ours.
pub fn patch_ours(
    spec: &RemapSpec,
    weapons: &[(String, Vec<String>)],
    patched: &str,
    ours: &mut OMap<OurFile>,
    index: &StockIndex,
    report: &mut Vec<String>,
) -> Result<()> {
    let to: Vec<(String, Option<String>)> = remap_targets(spec, index, ours)?;
    let mut keys: Vec<Vec<u8>> = to.iter().map(|(k, _)| k.as_bytes().to_vec()).collect();
    keys.extend(
        spec.refs
            .iter()
            .map(|(k, _)| k.rsplit_once('/').map_or(k.as_str(), |(h, _)| h).as_bytes().to_vec()),
    );
    let to: HashMap<String, Option<String>> = to.into_iter().collect();
    let refs: HashMap<String, Option<String>> = spec.refs.iter().cloned().collect();
    super::stage::rmtree(patched)?;
    let (mut files, mut total) = (0, 0);
    let items: Vec<(String, OurFile)> = ours.iter().map(|(k, v)| (k.to_owned(), v.clone())).collect();
    for (low, (rel, path)) in &items {
        if !low.ends_with(".dbx") {
            continue;
        }
        let data = read(path)?;
        let lower = data.to_ascii_lowercase();
        if !keys.iter().any(|k| k.is_empty() || contains(&lower, k)) {
            continue;
        }
        let (new, n) = patch_refs(&data, &to, &refs)?;
        if n > 0 {
            let p = put_patched(patched, rel, &new)?;
            ours.insert(low, (rel.clone(), p));
            files += 1;
            total += n;
        }
    }
    report.push(format!(
        "remapped: {total} references in {files} of our partitions (REMAP: BFBC2 equivalents, or null)"
    ));
    let ids = persistence_ids(index)?;
    let mut by_part: HashMap<String, String> = HashMap::new();
    for (id, parts) in weapons {
        let r = ids.get(id).ok_or_else(|| format!("KeyError: '{id}'"))?;
        for p in parts {
            by_part.insert(p.clone(), r.clone());
        }
    }
    let mut missing: std::collections::BTreeSet<String> = by_part.keys().cloned().collect();
    let mut done = 0;
    let items: Vec<(String, OurFile)> = ours.iter().map(|(k, v)| (k.to_owned(), v.clone())).collect();
    for (low, (rel, path)) in &items {
        let stem = low
            .get(..low.len().saturating_sub(4))
            .unwrap_or_default()
            .rsplit('/')
            .next()
            .unwrap_or_default();
        if !low.ends_with(".dbx") || !low.starts_with("objects/weapons/") {
            continue;
        }
        let Some(r) = by_part.get(stem) else { continue };
        missing.remove(stem);
        let (new, n) = weapon_persistence(&read(path)?, r)?;
        if n > 0 {
            let p = put_patched(patched, rel, &new)?;
            ours.insert(low, (rel.clone(), p));
            done += 1;
        }
    }
    if !missing.is_empty() {
        let v: Vec<String> = missing.iter().map(|m| bc2_pyrt::fmt::str_repr(m)).collect();
        return Err(format!(
            "WEAPON_PERSISTENCE: weapons not among our partitions: [{}]",
            v.join(", ")
        ));
    }
    report.push(format!(
        "weapon unlock entries: {done} WW2 weapons given BFBC2 entries (WEAPON_PERSISTENCE)"
    ));
    Ok(())
}

// ------------------------------------------------------------------------------------------------ the level

/// What `edit_level` changes (the level's bindings and OPFOR's decisions).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct LevelSpec {
    /// `LEVEL` (`levels/wake_island_s`).
    pub level: String,
    /// `NAME`.
    pub name: String,
    /// `LV`.
    pub lv: String,
    /// `work\ww2` (the WW2 teams are in `kits`).
    pub w: String,
    /// `TEAMS`: (old team partition, the WW2 team's partition), in order.
    pub teams: Vec<(String, String)>,
    /// `SKY_DOME`: BF1943's sky dome object.
    pub sky_dome: String,
    /// The sky's Option B (`wake_sky.enabled`): the dome kept (exactly one placement).
    #[serde(default)]
    pub keep_dome: bool,
    /// `--vietnam-vo`: Vietnam's voice-over sets from `vo_level`.
    #[serde(default)]
    pub vietnam_vo: bool,
    /// `VO_LEVEL`.
    #[serde(default)]
    pub vo_level: String,
    /// `WAKE_TEST_DROP` (a testing aid): parts of blueprint references whose placements are left out.
    #[serde(default)]
    pub test_drop: Vec<String>,
    /// `WAKE_TEST_SPAWN` (a testing aid): where the US base's spawn moves.
    #[serde(default)]
    pub test_spawn: Option<[f64; 3]>,
}

fn team_refs(spec: &LevelSpec) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for (old, new) in &spec.teams {
        let p = pjoin(&spec.w, &format!("kits/{new}.dbx"));
        let (_, g) = primary_type(&read(&p)?)?;
        let g = g.ok_or_else(|| format!("AttributeError: {p} has no primary instance"))?;
        out.push((old.clone(), format!("{}/{}", new.to_lowercase(), g.to_lowercase())));
    }
    Ok(out)
}

fn ours_guid(ours: &OMap<OurFile>, p: &str) -> Result<String> {
    let (_, path) = ours
        .get(&format!("{p}.dbx"))
        .ok_or_else(|| format!("KeyError: '{p}.dbx'"))?;
    let (_, g) = primary_type(&read(path)?)?;
    Ok(g.unwrap_or_default().to_lowercase())
}

fn ref_walk(f: &mut DbxFile, e: &mut Element, to: &HashMap<String, String>, n: &mut usize) {
    if let Some(r) = attr(f, e, "ref").filter(|r| !r.is_empty()).map(str::to_owned) {
        let head = r.rsplit_once('/').map_or(r.as_str(), |(h, _)| h).to_lowercase();
        if let Some(v) = to.get(&head) {
            set_attr(f, e, "ref", v);
            *n += 1;
        }
    }
    if let Some(c) = kids_mut(e) {
        for x in c.iter_mut() {
            ref_walk(f, x, to, n);
        }
    }
}

/// The partition's `LevelData` instance's position among the partition's children.
fn level_data_at(f: &DbxFile) -> Result<usize> {
    instance_at(f)
        .into_iter()
        .find(|&i| {
            f.roots
                .first()
                .and_then(|p| kids(p).get(i))
                .is_some_and(|x| attr(f, x, "type") == Some("GameSharedResources.LevelData"))
        })
        .ok_or_else(|| "StopIteration: no GameSharedResources.LevelData instance".to_owned())
}

fn instance_mut(roots: &mut [Element], i: usize) -> Option<&mut Element> {
    roots.first_mut().and_then(kids_mut).and_then(|c| c.get_mut(i))
}

fn instance_ref(f: &DbxFile, i: usize) -> Option<&Element> {
    f.roots.first().and_then(|p| kids(p).get(i))
}

/// `test_drop`: the placements whose blueprint reference holds one of the parts left out (and `groups:<kind>`: the
/// model group entities of that kind).
fn test_drop(f: &mut DbxFile, parts: &[String], report: &mut Vec<String>) {
    let parts: Vec<String> = parts
        .iter()
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return;
    }
    let mut gone = Vec::new();
    for i in instance_at(f) {
        let Some(x) = instance_ref(f, i) else { continue };
        if attr(f, x, "type") == Some("Entity.ReferenceObjectData") {
            let r = f
                .child(x, "ReferencedObject")
                .and_then(|c| attr(f, c, "ref"))
                .unwrap_or_default()
                .to_lowercase();
            if parts.iter().any(|p| r.contains(p.as_str())) {
                gone.push(attr(f, x, "guid").unwrap_or_default().to_owned());
            }
        }
    }
    let kinds: Vec<&str> = parts.iter().filter_map(|p| p.strip_prefix("groups:")).collect();
    for i in instance_at(f) {
        let Some(x) = instance_ref(f, i) else { continue };
        let t = attr(f, x, "type").unwrap_or_default().to_lowercase();
        if kinds.iter().any(|k| {
            t.ends_with(&format!(".{k}modelgroupentitydata")) || (t.ends_with(".forestentitydata") && *k == "forest")
        }) {
            gone.push(attr(f, x, "guid").unwrap_or_default().to_owned());
        }
    }
    drop_instances(f, &gone);
    let shown: Vec<String> = parts.iter().map(|p| bc2_pyrt::fmt::str_repr(p)).collect();
    report.push(format!(
        "level: TEST - {} placements left out (WAKE_TEST_DROP [{}])",
        gone.len(),
        shown.join(", ")
    ));
}

/// `test_spawn`: the US base's spawn (the carrier's `SoldierSpawn` and its alternate points) moved to `to`.
fn test_spawn(f: &mut DbxFile, to: [f64; 3], report: &mut Vec<String>) -> Result<()> {
    let at = instance_at(f);
    let mut guid_at: HashMap<String, usize> = HashMap::new();
    for &i in &at {
        if let Some(x) = instance_ref(f, i) {
            guid_at.insert(attr(f, x, "guid").unwrap_or_default().to_lowercase(), i);
        }
    }
    let sp = at
        .iter()
        .copied()
        .find(|&i| {
            instance_ref(f, i).is_some_and(|x| {
                attr(f, x, "type") == Some("GameSharedResources.SoldierSpawnEntityData")
                    && f.child(x, "LocationNameSid")
                        .is_some_and(|c| f.text(c) == "CQ_MP_BASE_ALLIED_CARRIER")
            })
        })
        .ok_or("StopIteration: no CQ_MP_BASE_ALLIED_CARRIER spawn")?;
    let spe = instance_ref(f, sp).ok_or("no spawn")?;
    let m0 = f
        .child(spe, "Transform")
        .and_then(dbx::floats)
        .ok_or("ValueError: not 4-byte numbers")?;
    let d: Vec<f64> = (0..3)
        .map(|k| to.get(k).copied().unwrap_or(0.0) - f64::from(m0.get(12 + k).copied().unwrap_or(0.0)))
        .collect();
    let mut moved_ids = vec![sp];
    if let Some(alt) = f.child(spe, "AlternativeSpawnPoints") {
        for c in kids(alt) {
            if let Some(r) = attr(f, c, "ref")
                && let Some(&i) = guid_at.get(&r.to_lowercase())
            {
                moved_ids.push(i);
            }
        }
    }
    let n = moved_ids.len();
    with_roots(f, |f, roots| {
        for i in moved_ids {
            let Some(inst) = instance_mut(roots, i) else { continue };
            let Some(k) = child_at(f, inst, "Transform") else {
                continue;
            };
            let Some(tr) = kids_mut(inst).and_then(|c| c.get_mut(k)) else {
                continue;
            };
            let Some(mut m) = dbx::floats(tr) else { continue };
            for (j, dk) in d.iter().enumerate() {
                if let Some(v) = m.get_mut(12 + j) {
                    // Python adds in double precision and packs the result as float32
                    *v = (f64::from(*v) + dk) as f32;
                }
            }
            let mut raw = Vec::new();
            dbx::put_varint(&mut raw, m.len() as u64);
            dbx::put_varint(&mut raw, 4);
            for v in m {
                raw.extend(v.to_be_bytes());
            }
            tr.body = Body::Numbers(raw);
        }
    });
    let shown: Vec<String> = to.iter().map(|v| bc2_pyrt::fmt::float_repr(*v)).collect();
    report.push(format!(
        "level: TEST - the US base spawn moved to [{}] ({n} points; WAKE_TEST_SPAWN)",
        shown.join(", ")
    ));
    Ok(())
}

/// `vo_from(level_bundle)`: a stock level's `LevelData` (the file and the instance's position).
fn vo_from(bundle: &str) -> Result<(DbxFile, usize)> {
    let h = bc2_gamefs::bundles::entries_path(bundle).map_err(|e| format!("{bundle}: {e}"))?;
    let re = Regex::new(r"^levels/[^/]+\.dbx$").map_err(|e| e.to_string())?;
    let n = h
        .entries
        .iter()
        .map(|e| e.name.clone())
        .find(|n| re.is_match(&n.to_lowercase()))
        .ok_or("StopIteration: no level partition")?;
    let f = parse(&bundle_one(bundle, &n)?)?;
    let at = level_data_at(&f)?;
    Ok((f, at))
}

/// `edit_level(data, ours, report)`: the level partition with its teams pointed at the WW2 teams, its material
/// grid at the grid's primary instance, its `LevelData` named `Levels/<NAME>`, BF1943's sky dome left out (or kept:
/// the sky's Option B), and its voice-over: BF1943's with DICE's Japanese chatter and the level's game-info lines
/// added (or Vietnam's, `--vietnam-vo`).
///
/// # Errors
/// A partition that cannot be read, or a level without its fields.
#[allow(clippy::too_many_lines, reason = "edit_level, step for step")]
pub fn edit_level(data: &[u8], ours: &OMap<OurFile>, spec: &LevelSpec, report: &mut Vec<String>) -> Result<Vec<u8>> {
    let mut f = parse(data)?;
    let mut to: HashMap<String, String> = team_refs(spec)?.into_iter().collect();
    let grid = format!("{}/materials_xenon", spec.level);
    let g = ours_guid(ours, &grid)?;
    to.insert(grid.clone(), format!("{grid}/{g}"));
    let n = with_roots(&mut f, |f, roots| {
        let mut n = 0;
        for r in roots.iter_mut() {
            ref_walk(f, r, &to, &mut n);
        }
        n
    });
    let ld = level_data_at(&f)?;
    let new_name = format!("Levels/{}", spec.name);
    with_roots(&mut f, |f, roots| -> Result<()> {
        let inst = instance_mut(roots, ld).ok_or("no LevelData")?;
        let k = child_at(f, inst, "Name").ok_or("AssertionError: LevelData: no field Name")?;
        let nm = kids_mut(inst).and_then(|c| c.get_mut(k)).ok_or("no Name")?;
        if nm.kind != TEXT {
            return Err("LevelData.Name: not a text field".into());
        }
        nm.body = Body::Text(f.intern(&new_name));
        Ok(())
    })?;
    let dome_pre = format!("{}/", spec.sky_dome);
    let mut dome = Vec::new();
    for i in instance_at(&f) {
        let Some(x) = instance_ref(&f, i) else { continue };
        if attr(&f, x, "type") != Some("Entity.ReferenceObjectData") {
            continue;
        }
        let ro = f.child(x, "ReferencedObject").ok_or_else(|| {
            format!(
                "AssertionError: {}: no field ReferencedObject",
                attr(&f, x, "guid").unwrap_or_default()
            )
        })?;
        if attr(&f, ro, "ref")
            .unwrap_or_default()
            .to_lowercase()
            .starts_with(&dome_pre)
        {
            dome.push(attr(&f, x, "guid").unwrap_or_default().to_owned());
        }
    }
    if spec.keep_dome {
        if dome.len() != 1 {
            return Err("ValueError: Option B requires exactly one source sky dome placement".into());
        }
        report.push("level: source BF1943 sky dome retained (Option B prototype)".into());
    } else {
        drop_instances(&mut f, &dome);
        report.push(format!("level: BF1943's sky dome left out ({} placement)", dome.len()));
    }
    test_drop(&mut f, &spec.test_drop, report);
    if let Some(at) = spec.test_spawn {
        test_spawn(&mut f, at, report)?;
    }
    let ld = level_data_at(&f)?;
    if spec.vietnam_vo {
        let (src, sld) = vo_from(&spec.vo_level)?;
        let src_inst = instance_ref(&src, sld).ok_or("no LevelData")?;
        let mut counts = Vec::new();
        let mut copies = Vec::new();
        for field in ["VoiceOvers", "VoiceOverLines"] {
            let c = src.child(src_inst, field).map(kids).unwrap_or_default();
            counts.push(c.len());
            copies.push(c.to_vec());
        }
        with_roots(&mut f, |f, roots| -> Result<()> {
            let inst = instance_mut(roots, ld).ok_or("no LevelData")?;
            for (field, items) in ["VoiceOvers", "VoiceOverLines"].into_iter().zip(copies) {
                let k =
                    child_at(f, inst, field).ok_or_else(|| format!("AssertionError: LevelData: no field {field}"))?;
                let new: Vec<Element> = items.iter().map(|x| copy_el(f, &src, x)).collect();
                if let Some(ch) = kids_mut(inst).and_then(|c| c.get_mut(k)) {
                    ch.body = Body::Container(new);
                }
            }
            Ok(())
        })?;
        report.push(format!(
            "level: {n} references re-pointed; voice-over: nam_mp_002cq's (--vietnam-vo: {} trigger sets, {} lines); \
             LevelData named Levels/{}",
            counts.first().copied().unwrap_or(0),
            counts.get(1).copied().unwrap_or(0),
            spec.name
        ));
        return dbx_write(&f);
    }
    for (stem, line) in [
        (
            "sound/vo_triggers/sp/japanese/vo_sp_jp_generic".to_owned(),
            "level: voice-over: DICE's Japanese battle chatter (VO_SP_JP_Generic) added",
        ),
        (
            format!("sound/vo_triggers/ww2/{}_gameinfo", spec.lv),
            "level: voice-over: BF1943's round won / lost and multikill lines (wake_vo.gameinfo)",
        ),
    ] {
        let vo_len = instance_ref(&f, ld)
            .and_then(|x| f.child(x, "VoiceOvers"))
            .ok_or("AssertionError: LevelData: no field VoiceOvers")?
            .children()
            .len();
        if vo_len == 0 || !ours.contains(&format!("{stem}.dbx")) {
            continue;
        }
        let g = ours_guid(ours, &stem)?;
        let new_ref = format!("{stem}/{g}");
        let pre = format!("{stem}/");
        let added = with_roots(&mut f, |f, roots| {
            let Some(inst) = instance_mut(roots, ld) else {
                return false;
            };
            let Some(k) = child_at(f, inst, "VoiceOvers") else {
                return false;
            };
            let Some(vo) = kids_mut(inst).and_then(|c| c.get_mut(k)).and_then(kids_mut) else {
                return false;
            };
            let Some(first) = vo.first() else { return false };
            let mut item = first.clone();
            set_attr(f, &mut item, "ref", &new_ref);
            if vo
                .iter()
                .any(|c| attr(f, c, "ref").unwrap_or_default().to_lowercase().starts_with(&pre))
            {
                return false;
            }
            vo.push(item);
            true
        });
        if added {
            report.push(line.to_owned());
        }
    }
    let inst = instance_ref(&f, ld).ok_or("no LevelData")?;
    let vo = f.child(inst, "VoiceOvers").map_or(0, |c| c.children().len());
    let lines = f
        .child(inst, "VoiceOverLines")
        .ok_or("AssertionError: LevelData: no field VoiceOverLines")?
        .children()
        .len();
    report.push(format!(
        "level: {n} references re-pointed (teams -> WW2_US / WW2_JP, the material grid); voice-over: BF1943's ({vo} \
         trigger asset(s), {lines} lines); LevelData named Levels/{}",
        spec.name
    ));
    dbx_write(&f)
}

// ------------------------------------------------------------------------------------------------ our copies

/// `kit_voices(ours, report)`: the WW2 kits' `VOTypes` set to BF1943's person of their side (`voices`: (partition
/// prefix, the person's reference), in order; `persons`: the persons a kit's `VOTypes` may hold), as copies in
/// `patched`.
///
/// # Errors
/// A partition that cannot be read or written.
pub fn kit_voices(
    voices: &[(String, String)],
    persons: &HashSet<String>,
    patched: &str,
    ours: &mut OMap<OurFile>,
    report: &mut Vec<String>,
) -> Result<()> {
    let mut items: Vec<(String, OurFile)> = ours.iter().map(|(k, v)| (k.to_owned(), v.clone())).collect();
    items.sort();
    let mut done = 0;
    for (low, (rel, path)) in items {
        let Some((_, voice)) = voices.iter().find(|(s, _)| low.starts_with(s.as_str())) else {
            continue;
        };
        if !low.ends_with(".dbx") {
            continue;
        }
        let mut f = parse(&read(&path)?)?;
        let at = instance_at(&f);
        let hit = with_roots(&mut f, |f, roots| {
            let mut hit = 0;
            for i in at {
                if let Some(inst) = instance_mut(roots, i) {
                    voice_walk(f, inst, voice, persons, &mut hit);
                }
            }
            hit
        });
        if hit > 0 {
            let p = put_patched(patched, &rel, &dbx_write(&f)?)?;
            ours.insert(&low, (rel, p));
            done += 1;
        }
    }
    report.push(format!(
        "kits: {done} WW2 kits speak with BF1943's person of their side (VOTypes Beach_US / Beach_JP)"
    ));
    Ok(())
}

fn voice_walk(f: &mut DbxFile, e: &mut Element, voice: &str, persons: &HashSet<String>, hit: &mut usize) {
    let Some(c) = kids_mut(e) else { return };
    for x in c.iter_mut() {
        let is_votypes = attr(f, x, "name") == Some("VOTypes");
        let has_kids = !x.children().is_empty();
        let names_person = x.children().iter().any(|i| {
            let r = attr(f, i, "ref").unwrap_or_default();
            persons.contains(r.rsplit('/').next().unwrap_or_default())
        });
        if is_votypes && has_kids && names_person {
            let Some(mut item) = x.children().first().cloned() else {
                continue;
            };
            set_attr(f, &mut item, "ref", voice);
            x.body = Body::Container(vec![item]);
            *hit += 1;
        } else if has_kids {
            voice_walk(f, x, voice, persons, hit);
        }
    }
}

/// What `sky_patch` copies (OPFOR's `SKY_FROM`, `SKY_KINDS`, `SKY_KEEP_BF`).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct SkySpec {
    /// The donor bundle (a path).
    pub bundle: String,
    /// The donor state (`weather/r_mp03/states/state1/state1`).
    pub state: String,
    /// The kinds of state partitions.
    pub kinds: Vec<String>,
    /// The kinds kept as the level's own.
    pub keep: Vec<String>,
    /// The level's weather states' folder (`weather/wake_island/states`).
    pub states: String,
}

/// `sky_patch(ours, report)` without the sky's Option B: the level's weather states with the donor state's cloud
/// layer panorama mapping (the kinds not kept), as copies in `patched`.
///
/// # Errors
/// A partition that cannot be read or written, or a donor kind missing.
pub fn sky_patch(spec: &SkySpec, patched: &str, ours: &mut OMap<OurFile>, report: &mut Vec<String>) -> Result<()> {
    let h = bc2_gamefs::bundles::entries_path(&spec.bundle).map_err(|e| format!("{}: {e}", spec.bundle))?;
    let names: HashMap<String, String> = h
        .entries
        .iter()
        .filter(|e| !e.is_deleted())
        .map(|e| (e.name.to_lowercase(), e.name.clone()))
        .collect();
    let mut donors: HashMap<String, DbxFile> = HashMap::new();
    for k in &spec.kinds {
        let key = format!("{}_{k}.dbx", spec.state);
        let n = names.get(&key).ok_or_else(|| format!("KeyError: '{key}'"))?;
        donors.insert(k.clone(), parse(&bundle_one(&spec.bundle, n)?)?);
    }
    let pat = Regex::new(&format!(
        r"^{}/([^/]+)/([^/]+)_({})\.dbx$",
        regex::escape(&spec.states),
        spec.kinds.join("|")
    ))
    .map_err(|e| e.to_string())?;
    let mut items: Vec<(String, OurFile)> = ours.iter().map(|(k, v)| (k.to_owned(), v.clone())).collect();
    items.sort();
    let mut done = Vec::new();
    for (low, (rel, path)) in items {
        let Some(c) = pat.captures(&low) else { continue };
        let (state, again, kind) = (
            c.get(1).map_or("", |m| m.as_str()),
            c.get(2).map_or("", |m| m.as_str()),
            c.get(3).map_or("", |m| m.as_str()),
        );
        if state != again || spec.keep.iter().any(|k| k == kind) {
            continue;
        }
        let Some(donor) = donors.get(kind) else { continue };
        let mut f = parse(&read(&path)?)?;
        let at = *instance_at(&f).first().ok_or("IndexError: no instance")?;
        let dat = *instance_at(donor).first().ok_or("IndexError: no instance")?;
        let dinst = instance_ref(donor, dat).ok_or("no donor instance")?;
        let n = with_roots(&mut f, |f, roots| -> Result<usize> {
            let inst = instance_mut(roots, at).ok_or("no instance")?;
            if inst.kind != CONTAINER || dinst.kind != CONTAINER {
                return Err(format!("AssertionError: {low}: a sky state instance is no container"));
            }
            let mut have: HashMap<String, usize> = HashMap::new();
            for (i, c) in kids(inst).iter().enumerate() {
                have.insert(attr(f, c, "name").unwrap_or_default().to_owned(), i);
            }
            let mut n = 0;
            for c in kids(dinst) {
                let name = attr(donor, c, "name").unwrap_or_default();
                if name == "Name" || name == "MemoryReportGroup" {
                    continue;
                }
                let Some(&i) = have.get(name) else { continue };
                if kind == "clouds" && name != "PanoramicCloudLayer" {
                    continue;
                }
                let copy = copy_el(f, donor, c);
                if let Some(slot) = kids_mut(inst).and_then(|v| v.get_mut(i)) {
                    *slot = copy;
                }
                n += 1;
            }
            Ok(n)
        })?;
        let p = put_patched(patched, &rel, &dbx_write(&f)?)?;
        ours.insert(&low, (rel, p));
        done.push(format!("{state} {kind} ({n})"));
    }
    report.push(format!(
        "sky: mp_003's state1 panorama mapping in {} Wake state partitions ({}); the static cloud layer's panorama: \
         MP03_Sky_01 / _m (REMAP)",
        done.len(),
        done.join(", ")
    ));
    Ok(())
}

/// What `root_files` takes (OPFOR's `ROOT_FILES`, `ROOT_DONOR`, `ROOT_FILE_DONORS`).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct RootSpec {
    /// The level's staging folder (`WORK`; the dbx copies before the rename are in `root`).
    pub work: String,
    /// `NAME`.
    pub name: String,
    /// The files, in order.
    pub files: Vec<String>,
    /// `ROOT_DONOR`: (level, bundle).
    pub donor: (String, String),
    /// `ROOT_FILE_DONORS`: {lower file: (level, bundle)}.
    pub donors: HashMap<String, (String, String)>,
}

/// `root_files(stage, report)`: the donor's level-root partitions and atlases under the level's name, in the level
/// folder: {lower name: (name, type, path)}.
///
/// # Errors
/// A donor without the file, DbxEdit's rename failed or left the old prefix.
pub fn root_files(spec: &RootSpec, stage: &str, report: &mut Vec<String>) -> Result<OMap<Take>> {
    let mut out = OMap::new();
    let tmp = pjoin(&spec.work, "root");
    std::fs::create_dir_all(&tmp).map_err(|e| format!("{tmp}: {e}"))?;
    for f in &spec.files {
        let (lv, bundle) = spec.donors.get(&f.to_lowercase()).unwrap_or(&spec.donor);
        let h = bc2_gamefs::bundles::entries_path(bundle).map_err(|e| format!("{bundle}: {e}"))?;
        let key = format!("levels/{lv}/{f}").to_lowercase();
        let e = h
            .entries
            .iter()
            .filter(|e| !e.is_deleted())
            .rfind(|e| e.name.to_lowercase() == key)
            .ok_or_else(|| format!("KeyError: '{key}'"))?;
        let data = bundle_one(bundle, &e.name)?;
        let tail = e.name.splitn(3, '/').nth(2).unwrap_or_default();
        let new = format!("Levels/{}/{tail}", spec.name);
        let dst = pjoin(stage, &new);
        std::fs::create_dir_all(super::dirname(&dst)).map_err(|e| format!("{dst}: {e}"))?;
        if e.name.to_lowercase().ends_with(".dbx") {
            let src = pjoin(&tmp, crate::pytext::basename(&e.name));
            std::fs::write(&src, &data).map_err(|e| format!("{src}: {e}"))?;
            let guid = bc2_formats::guid::new_guid("opfor", &[&new.to_lowercase()]);
            let a1 = format!("replaceprefix=levels/{lv}/|Levels/{}/", spec.name);
            let a2 = format!("partguid={guid}");
            let o = super::describe::dbxedit(&["edit", &src, &dst, &a1, &a2]);
            if o.code != 0 {
                return Err(format!(
                    "failed: DbxEdit edit {src} {dst} {a1} {a2}\n{}\n{}",
                    o.stdout, o.stderr
                ));
            }
            if to_xml(&read(&dst)?)?.to_lowercase().contains(&format!("levels/{lv}/")) {
                return Err(format!("{new}: still names levels/{lv}/ after the rename"));
            }
        } else {
            std::fs::write(&dst, &data).map_err(|e| format!("{dst}: {e}"))?;
        }
        out.insert(
            &new.to_lowercase(),
            Take {
                name: new,
                kind: Some(e.type_name.clone()),
                src: Src::Path(dst),
            },
        );
    }
    let asm = spec.donors.get("animationsoundmap.dbx").map_or("", |(l, _)| l.as_str());
    report.push(format!(
        "level root: {} files under Levels/{}/ (shader collection, emitter system, atlases: from {}; animation sound \
         map: from {asm})",
        out.len(),
        spec.name,
        spec.donor.0
    ));
    Ok(out)
}

fn src_bytes(t: &Take) -> Result<Option<Vec<u8>>> {
    Ok(match &t.src {
        Src::Bytes(b) => Some(b.clone()),
        Src::Path(p) => Some(read(p)?),
        Src::Stock(..) => None,
    })
}

/// Python's `repr` of bytes (`b'...'`).
fn bytes_repr(b: &[u8]) -> String {
    let quote = if b.contains(&b'\'') && !b.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut s = format!("b{quote}");
    for &c in b {
        match c {
            b'\\' => s.push_str("\\\\"),
            b'\t' => s.push_str("\\t"),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            _ if c == quote as u8 => {
                s.push('\\');
                s.push(quote);
            }
            0x20..=0x7e => s.push(char::from(c)),
            _ => s.push_str(&format!("\\x{c:02x}")),
        }
    }
    s.push(quote);
    s
}

fn contains(data: &[u8], needle: &[u8]) -> bool {
    data.windows(needle.len()).any(|w| w == needle)
}

/// `MESH_FLAGS`: a mesh asset's optional resources, loaded by its name plus a suffix when the flag is set.
pub const MESH_FLAGS: [(&str, &str); 2] = [
    ("OccluderMeshEnable", "_occludermesh"),
    ("IrradianceVolumeEnable", "_irradiance"),
];

/// `mesh_flags(files, have, report)`: the flags of [`MESH_FLAGS`] switched off in the mesh assets whose resource
/// no bundle the level loads has (`have`: every lower name the level has).
///
/// # Errors
/// A partition that cannot be read.
pub fn mesh_flags(files: &mut OMap<Take>, have: &HashSet<String>, report: &mut Vec<String>) -> Result<()> {
    let mut off = Vec::new();
    let keys: Vec<String> = files.keys().map(str::to_owned).collect();
    for low in keys {
        if !low.ends_with(".dbx") {
            continue;
        }
        let Some(t) = files.get(&low) else { continue };
        let Some(data) = src_bytes(t)? else { continue };
        if !MESH_FLAGS.iter().any(|(f, _)| contains(&data, f.as_bytes())) {
            continue;
        }
        let mut f = parse(&data)?;
        let at = instance_at(&f);
        let changed = with_roots(&mut f, |f, roots| {
            let mut changed = false;
            for i in at {
                let Some(inst) = instance_mut(roots, i) else { continue };
                if !attr(f, inst, "type").unwrap_or_default().ends_with("MeshAsset") {
                    continue;
                }
                let nm = f.child(inst, "Name").map(|c| f.text(c).to_owned());
                let mesh = nm.clone().unwrap_or_default().to_lowercase();
                for (field, suffix) in MESH_FLAGS {
                    let Some(k) = child_at(f, inst, field) else { continue };
                    let Some(c) = kids_mut(inst).and_then(|v| v.get_mut(k)) else {
                        continue;
                    };
                    if is_true(f, c) && !have.contains(&format!("{mesh}{suffix}.res")) {
                        set_false(c);
                        changed = true;
                        off.push(format!("{}.{field}", nm.clone().unwrap_or_default()));
                    }
                }
            }
            changed
        });
        if changed && let Some(t) = files.get_mut(&low) {
            t.src = Src::Bytes(dbx_write(&f)?);
        }
    }
    off.sort();
    let shown: Vec<String> = off.iter().take(12).map(|s| bc2_pyrt::fmt::str_repr(s)).collect();
    report.push(format!(
        "mesh flags: {} switched off (their resource is in no bundle): [{}]",
        off.len(),
        shown.join(", ")
    ));
    Ok(())
}

/// `ondemand_waves(files, have, report)`: the on-demand waves the level copies made ordinary level waves -
/// `IsOnDemand` false, the audio unwrapped from the on-demand form into the level bundle (`bundles`: the on-demand
/// sound bundles, in order).
///
/// # Errors
/// A partition or bundle that cannot be read, or an audio entry not in the on-demand form.
pub fn ondemand_waves(
    files: &mut OMap<Take>,
    have: &HashSet<String>,
    bundles: &[String],
    report: &mut Vec<String>,
) -> Result<()> {
    let mut od: HashMap<String, (String, String)> = HashMap::new();
    for b in bundles {
        if std::path::Path::new(b).exists() {
            let h = bc2_gamefs::bundles::entries_path(b).map_err(|e| format!("{b}: {e}"))?;
            for e in h.entries.iter().filter(|e| !e.is_deleted()) {
                od.entry(e.name.to_lowercase())
                    .or_insert_with(|| (b.clone(), e.name.clone()));
            }
        }
    }
    let mut want: OMap<(String, String)> = OMap::new();
    let mut changed = 0;
    let keys: Vec<String> = files.keys().map(str::to_owned).collect();
    for low in keys {
        if !low.ends_with("_wave.dbx") {
            continue;
        }
        let Some(t) = files.get(&low) else { continue };
        let Some(data) = src_bytes(t)? else { continue };
        if !contains(&data, b"IsOnDemand") {
            continue;
        }
        let mut f = parse(&data)?;
        let at = instance_at(&f);
        let res = format!("{}.res", low.get(..low.len() - 4).unwrap_or_default());
        let hit = with_roots(&mut f, |f, roots| {
            let mut hit = false;
            for i in at {
                let Some(inst) = instance_mut(roots, i) else { continue };
                let is_wave = attr(f, inst, "type") == Some("Audio.WaveAsset");
                let Some(k) = child_at(f, inst, "IsOnDemand") else {
                    continue;
                };
                let Some(c) = kids_mut(inst).and_then(|v| v.get_mut(k)) else {
                    continue;
                };
                if is_wave && is_true(f, c) {
                    let Some(src) = od.get(&res) else { continue };
                    set_false(c);
                    want.insert(&res, src.clone());
                    hit = true;
                }
            }
            hit
        });
        if hit {
            if let Some(t) = files.get_mut(&low) {
                t.src = Src::Bytes(dbx_write(&f)?);
            }
            changed += 1;
        }
    }
    let _ = have;
    let mut by: OMap<Vec<(String, String)>> = OMap::new();
    for (res, (b, n)) in want.iter() {
        if !by.contains(b) {
            by.insert(b, Vec::new());
        }
        if let Some(v) = by.get_mut(b) {
            v.push((res.to_owned(), n.clone()));
        }
    }
    for (b, items) in by.iter() {
        let names: Vec<String> = items.iter().map(|(_, n)| n.clone()).collect();
        let got = bundle_read(b, &names)?;
        for (res, n) in items {
            let d = got.get(n).map_or(&[][..], Vec::as_slice);
            if d.get(..4) != Some(b"RES\x02") || d.get(4..8) != Some(b"Wave") {
                let head = d.get(..8.min(d.len())).unwrap_or_default();
                return Err(format!("{n}: not an on-demand wave ({})", bytes_repr(head)));
            }
            let size = d
                .get(56..60)
                .and_then(|s| <[u8; 4]>::try_from(s).ok())
                .map_or(0, u32::from_le_bytes) as usize;
            let body = d.get(64..(64 + size).min(d.len())).unwrap_or_default().to_vec();
            files.insert(
                res,
                Take {
                    name: n.clone(),
                    kind: Some("Wave".into()),
                    src: Src::Bytes(body),
                },
            );
        }
    }
    report.push(format!(
        "on-demand waves: {changed} wave partitions made ordinary level waves, {} audio resources unwrapped into the \
         level bundle",
        want.len()
    ));
    Ok(())
}

/// The emitter atlas build's manifest, as OPFOR reads it (`emitter_atlas.py`'s `manifest.json`).
#[derive(Debug, Clone, Default)]
pub struct AtlasManifest {
    /// {lower emitter partition: (sha1 of the partition it was built from, its re-pointed copy's file)}.
    pub emitters: HashMap<String, (String, String)>,
    /// (lower atlas name, its name, its file, its size) in the manifest's order.
    pub atlases: Vec<(String, String, String, (i64, i64))>,
}

/// `emitter_atlas(take, report)`: the merged emitter atlases in place of the donor's, and every emitter the level
/// takes swapped for its re-pointed copy (`folder`: the build; `manifest`: its manifest, `None` when not built).
///
/// # Errors
/// An emitter changed since the build or not in it (the build is to be run again), or an atlas not taken.
pub fn emitter_atlas(
    take: &mut OMap<Take>,
    folder: &str,
    manifest: Option<&AtlasManifest>,
    report: &mut Vec<String>,
) -> Result<()> {
    let Some(m) = manifest else {
        report.push(
            "emitter atlas: not built (emitter_atlas.py) - mp_005's atlases shipped; emitters authored against other \
             atlases draw the wrong images"
                .into(),
        );
        return Ok(());
    };
    let (mut swapped, mut stale) = (0, Vec::new());
    let keys: Vec<String> = take.keys().map(str::to_owned).collect();
    for low in keys {
        if !low.ends_with(".dbx") {
            continue;
        }
        let Some(t) = take.get(&low) else { continue };
        let name = t.name.clone();
        let Some(data) = src_bytes(t)? else { continue };
        let Some((sha_in, file)) = m.emitters.get(&low) else {
            if contains(&data, b"TextureOriginU") {
                let f = parse(&data)?;
                let infos = bc2_convert::emitteratlas::infos(&f).map_err(|e| format!("EmitterAtlasError: {e:?}"))?;
                if infos
                    .iter()
                    .any(|i| (0..2).any(|ch| bc2_convert::emitteratlas::drawn(i, ch)))
                {
                    stale.push(format!("{name} (not in the build)"));
                }
            }
            continue;
        };
        let mut h = sha1_smol::Sha1::new();
        h.update(&data);
        if h.digest().to_string() != *sha_in {
            stale.push(format!("{name} (changed since the build)"));
            continue;
        }
        let bytes = read(&pjoin(folder, file))?;
        if let Some(t) = take.get_mut(&low) {
            t.src = Src::Bytes(bytes);
        }
        swapped += 1;
    }
    if !stale.is_empty() {
        let first: Vec<&str> = stale.iter().take(10).map(String::as_str).collect();
        return Err(format!(
            "emitter atlas: {} emitters differ from emitter_atlas.py's build - run it again: {}",
            stale.len(),
            first.join("; ")
        ));
    }
    for (low, name, file, _) in &m.atlases {
        if !take.contains(low) {
            return Err(format!(
                "emitter atlas: {name} is not a level-root file the install takes"
            ));
        }
        let bytes = read(&pjoin(folder, file))?;
        if let Some(t) = take.get_mut(low) {
            t.src = Src::Bytes(bytes);
        }
    }
    let sizes: Vec<String> = m.atlases.iter().map(|(_, _, _, (w, h))| format!("{w} x {h}")).collect();
    report.push(format!(
        "emitter atlas: emitter_atlas.py's atlases ({}) in place of mp_005's; {swapped} emitters re-pointed into them",
        sizes.join(", ")
    ));
    Ok(())
}

/// `undergrowth_files(index, report)`: the stand-in undergrowth meshes (`standins`: (new mesh, DICE's donor), in
/// order) - the donor's mesh set with its set names renamed and its LOD data as DICE's: {lower name: (name, type,
/// bytes)}.
///
/// # Errors
/// A donor not in the stock bundles, or a mesh set that cannot be read.
pub fn undergrowth_files(
    standins: &[(String, String)],
    index: &StockIndex,
    report: &mut Vec<String>,
) -> Result<OMap<Take>> {
    let mut out = OMap::new();
    let mut lows: Vec<&str> = Vec::new();
    for b in &index.bundles {
        for (low, _, _) in &b.names {
            lows.push(low.as_str());
        }
    }
    lows.sort_unstable();
    lows.dedup();
    for (new, donor) in standins {
        let dl = donor.to_lowercase();
        let key = format!("{dl}.res");
        let (b, n, t) = index.first(&key).ok_or_else(|| format!("KeyError: '{key}'"))?;
        let kind = bc2_formats::meshdata::Kind::of(t).ok_or_else(|| format!("{n}: not a mesh set ({t})"))?;
        let mut s = bc2_formats::meshdata::parse_set(&bundle_one(b, n)?, kind, false, None)
            .map_err(|e| format!("{n}: {e:?}"))?;
        let re = regex::bytes::RegexBuilder::new(&regex::escape(donor))
            .case_insensitive(true)
            .unicode(false)
            .build()
            .map_err(|e| e.to_string())?;
        for l in &mut s.lods {
            for (nm, _) in &mut l.sets {
                *nm = re.replace_all(nm, regex::bytes::NoExpand(new.as_bytes())).into_owned();
            }
        }
        let data = bc2_formats::meshdata::write_set(&s, kind).map_err(|e| format!("{n}: {e:?}"))?;
        out.insert(
            &format!("{}.res", new.to_lowercase()),
            Take {
                name: format!("{new}.res"),
                kind: Some(t.to_owned()),
                src: Src::Bytes(data),
            },
        );
        let pre = format!("{dl}_lod");
        for low in &lows {
            if low.starts_with(&pre) && low.ends_with(".res") {
                let (b2, n2, t2) = index.first(low).ok_or("no entry")?;
                let rel = format!("{new}{}", n2.get(donor.len()..).unwrap_or_default());
                out.insert(
                    &rel.to_lowercase(),
                    Take {
                        name: rel.clone(),
                        kind: Some(t2.to_owned()),
                        src: Src::Bytes(bundle_one(b2, n2)?),
                    },
                );
            }
        }
    }
    let shown: Vec<String> = standins
        .iter()
        .map(|(k, v)| format!("{} <- {v}", k.rsplit('/').next().unwrap_or_default()))
        .collect();
    report.push(format!(
        "undergrowth: {} terrain meshes as DICE's ({})",
        standins.len(),
        shown.join(", ")
    ));
    Ok(out)
}
