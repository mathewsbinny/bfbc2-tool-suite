//! A Napisal-level fix (round OR13): the port of Project OPFOR's `apply_sector_offsets.py`'s offsets, assets and
//! bundle edit - Napisal's Onslaught levels given the `Levels/<LEVEL>/Precomputed/<Kind>/SectorOffsets` assets their
//! path data needs, so the runtime numbers its sectors as the file does (without them it uses 0,0 and every route
//! request from some sectors fails).
//!
//! The values are read from the level's installed path data (per sector, `floor(sector centre / 100)` less the
//! sector's own grid id; the most common value over all sectors), which gives all 27 of DICE's shipped PC assets
//! (infantry and vehicle). Each asset is a `GameSharedResources.PathdataSectorOffsets` partition OPFOR writes: its
//! field types from the user's own `sp_004_B` asset (DbxEdit import `--like`, then OPFOR's GUIDs by `replace` /
//! `partguid`). OPFOR's script keeps the command line, the install ledger's calls and the scratch clean-up; the work
//! folder's files are the retired Python's byte for byte (rust/BENCHMARKS.md "OR13").

use super::napisal::{find_bytes, pct, rel};
use super::port::{StageOut, read_text_utf8};
use super::{OnsError, R, copy, dbx, fbrb, join, joins, pack, read, rmtree, size, write};

/// The levels the fix knows (`LEVELS`): lower case -> the level partition's name.
pub const LEVELS: [(&str, &str); 3] = [
    ("ons_mp_002", "ONS_MP_002"),
    ("ons_mp_004", "ONS_MP_004"),
    ("ons_mp_005gr", "ONS_MP_005GR"),
];
/// (the asset's kind, the path data's folder).
pub const KINDS: [(&str, &str); 2] = [("Infantry", "infantry"), ("Vehicle", "vehicle")];
/// The share of sectors that must give the same offset (ONS_MP_002's infantry X is 82 %: sector bounds include
/// their overlap, so sectors at the edge read one off).
pub const MIN_AGREEMENT: f64 = 0.75;

fn level_name(lv: &str) -> R<&'static str> {
    LEVELS
        .iter()
        .find(|(l, _)| *l == lv)
        .map(|(_, n)| *n)
        .ok_or_else(|| OnsError::Data(format!("KeyError: '{lv}'")))
}

/// `bundles(lv)`: the level's Dist and Package `level-00.fbrb` (both, present or not).
#[must_use]
pub fn bundles(game: &str, lv: &str) -> Vec<String> {
    vec![
        joins(game, &["Dist", "win32", "levels", lv, "level-00.fbrb"]),
        joins(game, &["Package", "levels", lv, "level-00.fbrb"]),
    ]
}

/// One kind's offsets: ((x, z), (the share of sectors agreeing on x, on z)).
pub type Offsets = ((i64, i64), (f64, f64));

/// `Counter.most_common(1)[0]` of values in the order first met: the largest count, the first met among equals.
fn most_common(c: &[(i64, usize)]) -> (i64, usize) {
    let mut best = (0, 0);
    for &(v, n) in c {
        if n > best.1 {
            best = (v, n);
        }
    }
    best
}

fn count(c: &mut Vec<(i64, usize)>, v: i64) {
    match c.iter_mut().find(|(k, _)| *k == v) {
        Some((_, n)) => *n += 1,
        None => c.push((v, 1)),
    }
}

/// `dict(counter)`'s repr.
fn counter_repr(c: &[(i64, usize)]) -> String {
    let items: Vec<String> = c.iter().map(|(k, v)| format!("{k}: {v}")).collect();
    format!("{{{}}}", items.join(", "))
}

/// `offsets(lv, kind)`: (x, z) from the installed path data's own sector ids, and the share of sectors that agree.
pub fn offsets(game: &str, lv: &str, kind: &str) -> R<Offsets> {
    let p = joins(game, &["Output", "win32", "ai", "levels", lv, kind, "pathdata.hpd"]);
    let data = read(&p)?;
    let f = bc2_formats::hpd::parse(&data).map_err(|e| OnsError::Data(format!("{p}: {e}")))?;
    let (mut cx, mut cz) = (Vec::new(), Vec::new());
    let mut n = 0usize;
    for s in &f.sectors {
        let b = s.bounds.map(f64::from);
        let id = i64::from(s.id);
        #[allow(clippy::cast_possible_truncation, reason = "math.floor of a sector coordinate / 200")]
        {
            count(&mut cx, ((b[0] + b[1]) / 200.0).floor() as i64 - (id >> 8));
            count(&mut cz, ((b[4] + b[5]) / 200.0).floor() as i64 - (id & 0xff));
        }
        n += 1;
    }
    if n == 0 {
        // Counter().most_common(1)[0] of an empty file
        return Err(OnsError::Data("IndexError: list index out of range".into()));
    }
    let ((x, nx), (z, nz)) = (most_common(&cx), most_common(&cz));
    #[allow(clippy::cast_precision_loss, reason = "sector counts")]
    let (ax, az) = (nx as f64 / n as f64, nz as f64 / n as f64);
    if ax.min(az) < MIN_AGREEMENT {
        return Err(OnsError::Exit(format!(
            "{lv} {kind}: sector ids disagree ({} / {})",
            counter_repr(&cx),
            counter_repr(&cz)
        )));
    }
    Ok(((x, z), (ax, az)))
}

/// `guids(lv, kind)`: OPFOR's partition and instance GUIDs of the asset (`opfor_guid('sector-offsets', lv, kind,
/// part)`).
#[must_use]
pub fn guids(lv: &str, kind: &str) -> (String, String) {
    let k = kind.to_lowercase();
    let g = |part: &str| bc2_formats::guid::new_guid("opfor:sectoroffsets", &[lv, &k, part]);
    (g("partition"), g("instance"))
}

/// `entry(lv, kind)`: the asset's name.
pub fn entry(lv: &str, kind: &str) -> R<String> {
    Ok(format!("Levels/{}/Precomputed/{kind}/SectorOffsets", level_name(lv)?))
}

/// One planned asset: (kind, (x, z), agreement, name, partition, instance).
pub type Row = (String, (i64, i64), (f64, f64), String, String, String);

/// `plan(lv)`.
pub fn plan(game: &str, lv: &str) -> R<Vec<Row>> {
    let mut rows = Vec::new();
    for (kind, folder) in KINDS {
        let (xz, agree) = offsets(game, lv, folder)?;
        let (part, inst) = guids(lv, kind);
        rows.push((kind.to_owned(), xz, agree, entry(lv, kind)?, part, inst));
    }
    Ok(rows)
}

/// `--plan`'s lines of a level.
pub fn plan_lines(root: &str, game: &str, lv: &str) -> R<Vec<String>> {
    let shown: Vec<String> = bundles(game, lv).iter().map(|b| rel(b, root)).collect();
    Ok(plan(game, lv)?
        .into_iter()
        .map(|(kind, xz, agree, name, part, inst)| {
            format!(
                "{lv} {kind}: XOffset {}, ZOffset {} (sector ids agree {} / {}); Name {name}; partition {part}, \
                 instance {inst}; entry {name}.dbx in {}",
                xz.0,
                xz.1,
                pct(agree.0),
                pct(agree.1),
                shown.join(", ")
            )
        })
        .collect())
}

/// `reference()`: the user's own `sp_004_B` infantry asset (types only), copied to `work\sectoroff\reference.dbx`,
/// and its two GUIDs. The `sp_004_B` unpack is `apply_ai_template`'s cache (`work\combat_a\sp_004_b_src`), made when
/// it has no manifest.
pub fn reference(root: &str, game: &str) -> R<(String, String, String)> {
    let cache = joins(root, &["work", "combat_a", "sp_004_b_src"]);
    if !std::path::Path::new(&join(&cache, "fbrb.manifest")).is_file() {
        rmtree(&cache)?;
        let bundle = joins(game, &["Dist", "win32", "levels", "sp_004_B", "level-00.fbrb"]);
        fbrb(&["unpack", &bundle, &cache])?;
    }
    let src = joins(
        &cache,
        &["Levels", "SP_004_B", "Precomputed", "Infantry", "SectorOffsets.dbx"],
    );
    let work = joins(root, &["work", "sectoroff"]);
    let refp = join(&work, "reference.dbx");
    copy(&src, &refp)?;
    dbx(&["xml", &refp])?;
    let t = read_text_utf8(&join(&work, "reference.xml"))?;
    let head = regex::Regex::new(r#"<partition guid="([^"]+)" primaryInstance="([^"]+)""#)
        .map_err(|e| OnsError::Data(e.to_string()))?;
    let m = head.captures(&t);
    match m {
        Some(c) if t.contains("GameSharedResources.PathdataSectorOffsets") => {
            Ok((refp, c[1].to_owned(), c[2].to_owned()))
        }
        _ => Err(OnsError::Exit("reference is not a PathdataSectorOffsets asset".into())),
    }
}

/// One built asset: (the .dbx built, its path in the bundle).
pub type Asset = (String, String);

/// `build(lv, kind, xz, ref, ref_part, ref_inst)`: the asset written and checked (name, values as big-endian 4-byte
/// integers, GUIDs, DbxEdit check).
#[allow(clippy::too_many_arguments, reason = "the Python's arguments")]
fn build(root: &str, lv: &str, kind: &str, xz: (i64, i64), refp: &str, ref_part: &str, ref_inst: &str) -> R<Asset> {
    let (part, inst) = guids(lv, kind);
    let name = entry(lv, kind)?;
    let d = joins(root, &["work", "sectoroff", lv]);
    let stem = join(&d, kind);
    let src = format!(
        "<?xml version=\"1.0\"?>\n<partition guid=\"{ref_part}\" primaryInstance=\"{ref_inst}\" exportMode=\"All\">\n\
         \t<instance guid=\"{ref_inst}\" type=\"GameSharedResources.PathdataSectorOffsets\" id=\"SectorOffsets\">\n\
         \t\t<field name=\"Name\">{name}</field>\n\
         \t\t<field name=\"MemoryReportGroup\">MemoryReportGroup_default</field>\n\
         \t\t<field name=\"XOffset\">{}</field>\n\
         \t\t<field name=\"ZOffset\">{}</field>\n\
         \t</instance>\n</partition>\n",
        xz.0, xz.1
    );
    write(&format!("{stem}_src.xml"), src.as_bytes())?;
    dbx(&[
        "import",
        &format!("{stem}_src.xml"),
        &format!("{stem}_typed.dbx"),
        "--like",
        refp,
    ])?;
    let out = format!("{stem}.dbx");
    dbx(&[
        "edit",
        &format!("{stem}_typed.dbx"),
        &out,
        &format!("replace={ref_inst}|{inst}"),
        &format!("partguid={part}"),
    ])?;
    dbx(&["xml", &out])?;
    let t = read_text_utf8(&format!("{stem}.xml"))?;
    let want = [
        format!("<partition guid=\"{part}\" primaryInstance=\"{inst}\""),
        format!("<instance guid=\"{inst}\""),
        format!("<field name=\"Name\">{name}</field>"),
        format!("<field name=\"XOffset\">{}</field>", xz.0),
        format!("<field name=\"ZOffset\">{}</field>", xz.1),
    ];
    if want.iter().any(|w| !t.contains(w.as_str())) || t.matches("<field ").count() != 4 {
        return Err(OnsError::Exit(format!(
            "{out}: built asset does not read back as intended:\n{t}"
        )));
    }
    let raw = read(&out)?;
    let be = |v: i64| i32::try_from(v).map(i32::to_be_bytes).ok();
    let stored = |v: i64| be(v).is_some_and(|b| find_bytes(&raw, &b).is_some());
    if !stored(xz.0) || !stored(xz.1) {
        return Err(OnsError::Exit(format!(
            "{out}: offsets are not stored as 4-byte integers"
        )));
    }
    let c = dbx(&["check", &out])?;
    if !c.contains(" 0 error(s)") {
        return Err(OnsError::Exit(format!("DbxEdit check: {c}")));
    }
    Ok((out, format!("{name}.dbx")))
}

/// `apply`'s first half for a level: the reference, then each kind's asset built; the lines it printed.
pub fn assets(root: &str, game: &str, lv: &str, out: &mut StageOut) -> R<Vec<Asset>> {
    let (refp, ref_part, ref_inst) = reference(root, game)?;
    let mut made = Vec::new();
    for (kind, xz, agree, _name, part, inst) in plan(game, lv)? {
        let (dbxf, path) = build(root, lv, &kind, xz, &refp, &ref_part, &ref_inst)?;
        out.printed.push(format!(
            "{lv} {kind}: XOffset {}, ZOffset {} (sector ids agree {} / {}); {path}; partition {part}, instance \
             {inst}; {} bytes",
            xz.0,
            xz.1,
            pct(agree.0),
            pct(agree.1),
            size(&dbxf)?
        ));
        made.push((dbxf, path));
    }
    Ok(made)
}

/// `present(listing, assets)`: which of the entries the bundle holds - `all`, `none`, `partial`, or `foreign` (an
/// asset of another name).
#[must_use]
pub fn present(listing: &str, assets: &[Asset]) -> &'static str {
    let names: Vec<String> = bc2_pyrt::py::splitlines(listing)
        .iter()
        .map(|l| l.split('|').next().unwrap_or("").to_lowercase())
        .collect();
    let ours: Vec<String> = assets.iter().map(|(_, p)| p.to_lowercase()).collect();
    let other = names
        .iter()
        .any(|n| n.contains("/precomputed/") && n.ends_with("/sectoroffsets.dbx") && !ours.contains(n));
    if other {
        return "foreign";
    }
    let have: Vec<bool> = ours.iter().map(|p| names.contains(p)).collect();
    if have.iter().all(|h| *h) {
        "all"
    } else if !have.iter().any(|h| *h) {
        "none"
    } else {
        "partial"
    }
}

/// What a bundle's edit came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edited {
    /// The bundle holds the assets as built already.
    Already,
    /// The bundle built with them (the file to install).
    Packed(String),
}

/// `apply`'s bundle step after the ledger's check: the bundle unpacked into `work\sectoroff\<lv>\b<k>`, the assets
/// added at the manifest's tail, packed, the tail checked.
pub fn edit_bundle(root: &str, lv: &str, k: usize, bundle: &str, assets: &[Asset]) -> R<Edited> {
    let d = joins(root, &["work", "sectoroff", lv, &format!("b{k}")]);
    rmtree(&d)?;
    fbrb(&["unpack", bundle, &d])?;
    let man = join(&d, "fbrb.manifest");
    let mut text = read_text_utf8(&man)?;
    let state = present(&text, assets);
    if state == "all" {
        let mut same = true;
        for (f, p) in assets {
            if read(&super::port::under(&d, p))? != read(f)? {
                same = false;
                break;
            }
        }
        if same {
            return Ok(Edited::Already);
        }
    }
    if state != "none" {
        return Err(OnsError::Exit(format!(
            "{}: SectorOffsets entries {state} - inconsistent state, stopping",
            rel(bundle, root)
        )));
    }
    for (f, p) in assets {
        copy(f, &super::port::under(&d, p))?;
        text = format!(
            "{}\n{p}|<non-resource>|00010000|{}\n",
            text.trim_end_matches(['\r', '\n']),
            size(f)?
        );
    }
    write(&man, text.as_bytes())?;
    let packed = joins(root, &["work", "sectoroff", lv, &format!("level-00_b{k}.fbrb")]);
    pack(&d, &packed)?;
    let listing = fbrb(&["list", &packed])?;
    let names: Vec<&str> = bc2_pyrt::py::splitlines(&listing)
        .into_iter()
        .filter(|x| !bc2_pyrt::py::strip(x).is_empty())
        .filter_map(|x| bc2_pyrt::py::split(x).last().copied())
        .collect();
    let tail = &names[names.len().saturating_sub(assets.len() + 1)..];
    if !assets
        .iter()
        .all(|(_, p)| tail.iter().any(|t| t.to_lowercase() == p.to_lowercase()))
    {
        let shown: Vec<String> = tail.iter().map(|t| bc2_pyrt::fmt::str_repr(t)).collect();
        return Err(OnsError::Exit(format!(
            "packed bundle: entries not at the tail ([{}])",
            shown.join(", ")
        )));
    }
    Ok(Edited::Packed(packed))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn guids_entries_and_agreement() {
        // opfor_guid('sector-offsets', 'ons_mp_002', 'infantry', 'partition'): uuid5 of
        // 'opfor:sectoroffsets:ons_mp_002:infantry:partition' (Python's uuid module)
        let (p, i) = guids("ons_mp_002", "Infantry");
        assert_eq!(p, "35BF602B-9459-5776-92DB-06DEE4395FE3");
        assert_ne!(p, i);
        assert_eq!(
            entry("ons_mp_005gr", "Vehicle").unwrap(),
            "Levels/ONS_MP_005GR/Precomputed/Vehicle/SectorOffsets"
        );
        assert!(entry("ons_mp_008", "Vehicle").is_err());
        assert_eq!(most_common(&[(3, 2), (-1, 5), (4, 5)]), (-1, 5));
        assert_eq!(counter_repr(&[(-7, 3), (2, 1)]), "{-7: 3, 2: 1}");
    }

    #[test]
    fn present_states() {
        let a = vec![
            (
                "x".to_owned(),
                "Levels/L/Precomputed/Infantry/SectorOffsets.dbx".to_owned(),
            ),
            (
                "y".to_owned(),
                "Levels/L/Precomputed/Vehicle/SectorOffsets.dbx".to_owned(),
            ),
        ];
        assert_eq!(present("a.dbx|x\nb|y\n", &a), "none");
        assert_eq!(
            present("levels/l/precomputed/infantry/sectoroffsets.dbx|q\n", &a),
            "partial"
        );
        assert_eq!(
            present(
                "Levels/L/Precomputed/Infantry/SectorOffsets.dbx|q\nLevels/L/Precomputed/Vehicle/SectorOffsets.dbx|q",
                &a
            ),
            "all"
        );
        assert_eq!(
            present("Levels/M/Precomputed/Infantry/SectorOffsets.dbx|q\n", &a),
            "foreign"
        );
    }
}
