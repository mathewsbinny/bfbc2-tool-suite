//! Stage 3 of the console Onslaught port (round OR9): the port of Project OPFOR's `console_mode.py`'s `plan` and
//! `apply` up to the ledger - the console Onslaught game set-up back on the level stage 2 built:
//!
//! - `LevelData.GameMode` pointed at `gamemodes/onslaught` (the console level's reference), and
//!   `GameModes/Onslaught.dbx` written into the level's bundle from the PC install's own `GameModes/Conquest`
//!   (mp_common) with the console mode's GUIDs, its `ModeID` / matchmaking id / description key ([`MODE_FIELDS`]);
//! - the console's `SpawnPointManager` and its `SpawnPointManagerData`;
//! - Nelson Bay's aerial heightmap values set back to the PC map's ([`PC_VALUES`]);
//! - every Onslaught flag `OnlyTakeableByTeam 0` (PC reads the field so that neither 1 nor 2 lets the attackers take a
//!   flag; play test 2026-10-29);
//! - the console's doorway pre-destructions left out (spheres reaching a placed door within 1.5 m: see-through walls
//!   and doors that flickered on PC);
//! - the console's `ExplosiveObjects` left out (Nelson Bay crashed at load with them; `console_mode.EXPLOSIVES` was
//!   off, its branch never ran and is not ported).
//!
//! The work folder's files are the Python's byte for byte (rust/BENCHMARKS.md "OR9"); the ledger's parts (already
//! applied, the check before the build, the install) stay in OPFOR's `console_mode.py`.

use std::sync::LazyLock;

use regex::{NoExpand, Regex};

use super::port::{StageOut, find, read_text_utf8, walk_all, write_text_utf8};
use super::{OnsError, R, Source, dbx, fbrb, join, joins, pack, read, rmtree, size, stem4, write};

/// `LevelData` values set back to the PC map's, per level.
pub const PC_VALUES: [(&str, &[(&str, &str)]); 1] = [(
    "ons_mp_008",
    &[("AerialHeightmapCellSize", "4"), ("AerialHeightmapQuality", "1")],
)];
/// The capture point values every Onslaught flag gets.
pub const CAPTURE_VALUES: [(&str, &str); 1] = [("OnlyTakeableByTeam", "0")];
/// Metres beyond a pre-destruction's radius that reach a door.
pub const DOOR_REACH: f64 = 1.5;
/// The Onslaught game mode's fields over PC Conquest's (`MatchmakingQueryID` stays CONQUEST: the PC stats code knows no
/// ONSLAUGHT category).
pub const MODE_FIELDS: [(&str, &str); 4] = [
    ("Name", "GameModes/Onslaught"),
    ("ModeID", "onslaught"),
    ("MatchmakingQueryID", "CONQUEST"),
    ("Description", "MP_GAMEMODE_ONSLAUGHT_NAME"),
];
/// The console levels this stage knows (`console_common.LEVELS`).
pub const LEVELS: [&str; 4] = ["ons_mp_002", "ons_mp_004", "ons_mp_005", "ons_mp_008"];

fn re(p: &str) -> Regex {
    #[allow(
        clippy::expect_used,
        reason = "the patterns are constants, each compiled by the tests"
    )]
    Regex::new(p).expect("a constant pattern")
}

/// Python's `\s` in a str pattern (`str.isspace`: Unicode white space and the separators U+001C - U+001F).
const WS: &str = r"[\s\x1c-\x1f]";

/// `console_common.INST`.
pub(crate) static INST: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?s)(<instance guid="([^"]+)" type="([^"]+)"[^>]*>)(.*?)(</instance>)"#));
static OFFSET: LazyLock<Regex> = LazyLock::new(|| re(r#"(<field name="EnumOffset">)(\d+)(</field>)"#));
static HEAD: LazyLock<Regex> = LazyLock::new(|| re(r#"<partition guid="([^"]+)" primaryInstance="([^"]+)""#));
static GAMEMODE: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?i)ref="(gamemodes/onslaught/([0-9a-f-]{36}))" name="GameMode""#));
static SPM: LazyLock<Regex> = LazyLock::new(|| re(r#"(?i)ref="([0-9a-f-]{36})" name="SpawnPointManager""#));
static EXPL: LazyLock<Regex> = LazyLock::new(|| re(r#"(?s)<array name="ExplosiveObjects">(.*?)</array>"#));
static GUID_REF: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([0-9a-f-]{36})""#));
static PC_GAMEMODE: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="gamemodes/[^"]*" name="GameMode""#));
static TRANSFORM: LazyLock<Regex> = LazyLock::new(|| re(r#"<complex name="Transform">([^<]*)</complex>"#));
static REFOBJ: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([^"]+)" name="ReferencedObject""#));
static RADIUS: LazyLock<Regex> = LazyLock::new(|| re(r#"name="Radius">([^<]*)<"#));
static NAME: LazyLock<Regex> = LazyLock::new(|| re(r#"<field name="Name">([^<]*)</field>"#));

/// `field(body, name, value)`: the first `<field name="<name>">` set to `value`.
fn field(body: &str, name: &str, value: &str) -> String {
    let r = re(&format!(r#"(<field name="{}">)[^<]*(</field>)"#, regex::escape(name)));
    r.replacen(body, 1, |c: &regex::Captures<'_>| format!("{}{value}{}", &c[1], &c[2]))
        .into_owned()
}

/// `xml_of(dbx)`: DbxEdit's XML of a .dbx (written beside it), read as text.
fn xml_of(path: &str) -> R<String> {
    dbx(&["xml", path])?;
    read_text_utf8(&format!("{}.xml", stem4(path)))
}

/// `tree_info(text)[0]`: a cull tree's `EnumOffset`.
fn enum_offset(text: &str) -> R<i64> {
    let c = OFFSET
        .captures(text)
        .ok_or_else(|| OnsError::Data("not found: EnumOffset".into()))?;
    c[2].parse::<i64>()
        .map_err(|e| OnsError::Data(format!("EnumOffset: {e}")))
}

/// `level_data(text)`: the first instance whose type ends `LevelData` (its whole text).
fn level_data(text: &str) -> R<String> {
    INST.captures_iter(text)
        .find(|c| c[3].ends_with("LevelData"))
        .map(|c| c[0].to_owned())
        .ok_or_else(|| OnsError::Data("StopIteration: no LevelData".into()))
}

/// Python's `float(text)`.
fn py_float(t: &str) -> R<f64> {
    bc2_pyrt::py::strip(t)
        .parse::<f64>()
        .map_err(|_| OnsError::Data(format!("could not convert string to float: {t:?}")))
}

/// `position(body)`: the transform's last row (the coordinates at 12 - 14; fewer when the text is shorter).
fn position(body: &str) -> R<Option<Vec<f64>>> {
    let Some(c) = TRANSFORM.captures(body) else {
        return Ok(None);
    };
    let v: Vec<&str> = c[1].split('/').collect();
    let mut out = Vec::new();
    for s in v.iter().skip(12).take(3) {
        out.push(py_float(s)?);
    }
    Ok(Some(out))
}

/// `drop_door_predestruction(level, console)`: the level without the console's pre-destructions whose sphere reaches
/// a placed door, with their `Objects` items; the GUIDs left out.
fn drop_door_predestruction(level: &str, console: &str) -> R<(String, Vec<String>)> {
    let con_guids: std::collections::HashSet<String> = INST
        .captures_iter(console)
        .filter(|c| c[3].ends_with("PredestructionEntityData"))
        .map(|c| c[2].to_uppercase())
        .collect();
    let mut doors: Vec<Vec<f64>> = Vec::new();
    for c in INST.captures_iter(level) {
        if c[3].ends_with("ReferenceObjectData") {
            let r = REFOBJ.captures(&c[4]).map(|x| x[1].to_owned());
            let pos = position(&c[4])?;
            if let (Some(r), Some(pos)) = (r, pos)
                && !pos.is_empty()
                && r.to_lowercase().contains("/door")
            {
                doors.push(pos);
            }
        }
    }
    let mut dropped = Vec::new();
    for c in INST.captures_iter(level) {
        if !c[3].ends_with("PredestructionEntityData") || !con_guids.contains(&c[2].to_uppercase()) {
            continue;
        }
        let pos = position(&c[4])?;
        let radius = RADIUS
            .captures(&c[4])
            .ok_or_else(|| OnsError::Data("not found: Radius".into()))
            .and_then(|x| py_float(&x[1]))?;
        let Some(pos) = pos.filter(|p| !p.is_empty()) else {
            continue;
        };
        let reach = doors.iter().any(|d| {
            // sum((a - b) ** 2 for a, b in zip(pos, d)) ** 0.5: zip stops at the shorter
            let s = pos
                .iter()
                .zip(d)
                .fold(0.0f64, |s, (a, b)| s + bc2_pyrt::math::sq(a - b));
            bc2_pyrt::math::pow(s, 0.5) <= radius + DOOR_REACH
        });
        if reach {
            dropped.push(c[2].to_owned());
        }
    }
    let mut out = level.to_owned();
    for g in &dropped {
        let r = re(&format!(
            r#"(?si){WS}*<instance guid="{g}" type="[^"]+"[^>]*>.*?</instance>"#
        ));
        out = r.replacen(&out, 1, NoExpand("")).into_owned();
        let r = re(&format!(r#"{WS}*<item ref="{}" />"#, g.to_lowercase()));
        out = r.replace_all(&out, NoExpand("")).into_owned();
    }
    Ok((out, dropped))
}

/// `mode_partition_guid(console, instance_guid)`: the console mode partition's GUID when the console data has
/// `GameModes/Onslaught` (an Update package carries it in mp2_common), else one derived from the instance GUID.
fn mode_partition_guid(console: &str, instance_guid: &str) -> R<String> {
    for d in [
        join(console, "GameModes"),
        joins(console, &["..", "con_mp2", "GameModes"]),
    ] {
        let f = join(&d, "Onslaught.dbx");
        if std::path::Path::new(&f).is_file() {
            let x = xml_of(&f)?;
            if let Some(m) = HEAD.captures(&x)
                && m[2].to_uppercase() == instance_guid.to_uppercase()
            {
                return Ok(m[1].to_owned());
            }
        }
    }
    Ok(bc2_formats::guid::new_guid(
        "opfor:gamemodes/onslaught",
        &[instance_guid],
    ))
}

/// What stage 3 is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder (`console_common.OPFOR`): its `work\console_mode\` is written.
    pub root: String,
    /// The game copy (`console_common.GAME`): read only.
    pub game: String,
    /// The Onslaught level (lower case).
    pub lv: String,
    /// The console level's unpacked folder (`--console`, default stage 1's `con_level`).
    pub console: String,
    /// `--apply` (the level and the mode partition written, the bundle packed) rather than `--plan`.
    pub write: bool,
}

/// `console_mode.plan(lv, console)` and, with `write`, `apply`'s build after the ledger's checks.
#[expect(clippy::too_many_lines, reason = "console_mode.plan and apply, step by step")]
pub fn build(a: &Args) -> R<StageOut> {
    let lv = a.lv.as_str();
    if !LEVELS.contains(&lv) {
        return Err(OnsError::Data(format!("KeyError: '{lv}'")));
    }
    let lvu = lv.to_uppercase();
    let work = joins(&a.root, &["work", "console_mode"]);
    let bundle = joins(&a.game, &["Package", "levels", lv, "level-00.fbrb"]);
    let src = joins(&work, &[lv, "pkg"]);
    rmtree(&src)?;
    fbrb(&["unpack", &bundle, &src])?;
    let level_dbx = find(&src, &format!("Levels/{lvu}.dbx"))?;
    let level = xml_of(&level_dbx)?;
    let con_level_dbx = find(&a.console, &format!("levels/{lvu}.dbx"))?;
    let con = xml_of(&con_level_dbx)?;
    // the cull trees' offsets: the explosive objects' slot shift, read (and checked) though the objects stay out
    enum_offset(&xml_of(&find(
        &src,
        &format!("Levels/{lvu}/staticculltree_exclusive.dbx"),
    )?)?)?;
    enum_offset(&xml_of(&find(
        &a.console,
        &format!("levels/{lvu}/staticculltree_exclusive.dbx"),
    )?)?)?;
    let con_ld = level_data(&con)?;
    let con_inst: std::collections::HashMap<String, String> = INST
        .captures_iter(&con)
        .map(|c| (c[2].to_uppercase(), c[0].to_owned()))
        .collect();
    let port_has: std::collections::HashSet<String> = INST.captures_iter(&level).map(|c| c[2].to_uppercase()).collect();
    let gm = GAMEMODE
        .captures(&con_ld)
        .ok_or_else(|| OnsError::Exit(format!("{lv}: the console level has no onslaught game mode")))?;
    let (gm_ref, gm_guid) = (gm[1].to_owned(), gm[2].to_owned());
    let spm = SPM.captures(&con_ld).map(|c| c[1].to_owned());
    let expl_refs = EXPL
        .captures(&con_ld)
        .map_or(0, |c| GUID_REF.captures_iter(&c[1]).count());

    let body = level_data(&level)?;
    let mut changes: Vec<String> = Vec::new();
    let n = PC_GAMEMODE.find_iter(&body).count();
    let mut new_body = PC_GAMEMODE
        .replace_all(&body, NoExpand(&format!(r#"ref="{gm_ref}" name="GameMode""#)))
        .into_owned();
    changes.push(if n > 0 {
        format!("GameMode -> {gm_ref}")
    } else {
        "GameMode field not found".into()
    });
    for (name, value) in PC_VALUES.iter().filter(|(l, _)| *l == lv).flat_map(|(_, v)| v.iter()) {
        let r = re(&format!(r#"(<field name="{}">)[^<]*(</field>)"#, regex::escape(name)));
        if r.is_match(&new_body) {
            new_body = field(&new_body, name, value);
            changes.push(format!("{name} -> {value} (PC)"));
        }
    }
    let mut added: Vec<String> = Vec::new();
    if let Some(s) = spm.as_deref().filter(|s| !port_has.contains(&s.to_uppercase())) {
        new_body = new_body.replace(
            r#"<field ref="null" name="SpawnPointManager" />"#,
            &format!(r#"<field ref="{}" name="SpawnPointManager" />"#, s.to_lowercase()),
        );
        added.push(
            con_inst
                .get(&s.to_uppercase())
                .cloned()
                .ok_or_else(|| OnsError::Data(format!("KeyError: '{}'", s.to_uppercase())))?,
        );
        changes.push(format!(
            "SpawnPointManager -> {} (SpawnPointManagerData added)",
            s.to_lowercase()
        ));
    }
    if expl_refs > 0 {
        changes.push(format!("ExplosiveObjects: {expl_refs} left out (EXPLOSIVES off)"));
    }
    let new_level = level.replace(&body, &new_body);
    let (mut new_level, dropped) = drop_door_predestruction(&new_level, &con)?;
    if !dropped.is_empty() {
        let short: Vec<String> = dropped.iter().map(|g| g.chars().take(8).collect()).collect();
        changes.push(format!(
            "{} door pre-destructions left out ({})",
            dropped.len(),
            short.join(", ")
        ));
    }
    let mut flags = 0usize;
    let mut set_values: Vec<String> = Vec::new();
    new_level = INST
        .replace_all(&new_level, |c: &regex::Captures<'_>| {
            let mut whole = c[0].to_owned();
            if !c[3].ends_with("CapturePointEntityData") || !whole.contains(r#"<field name="HoistFlag">true</field>"#) {
                return whole;
            }
            flags += 1;
            let name = NAME.captures(&whole).map(|m| m[1].to_owned());
            for (k, v) in CAPTURE_VALUES {
                whole = field(&whole, k, v);
                set_values.push(format!("{} {k} {v}", name.as_deref().unwrap_or("?")));
            }
            whole
        })
        .into_owned();
    changes.push(format!("{flags} flags: {}", set_values.join(", ")));
    if !added.is_empty() {
        let ins: Vec<String> = added.iter().map(|x| format!("\t{x}")).collect();
        new_level = new_level.replace("</partition>", &format!("{}\n</partition>", ins.join("\n")));
    }

    // GameModes/Onslaught.dbx from the PC install's Conquest (read from the bundle in memory)
    let conquest_bundle = joins(&a.game, &["Dist", "win32", "levels", "mp_common", "level-00.fbrb"]);
    let data = bc2_gamefs::bundles::read_path(&conquest_bundle, &["gamemodes/conquest.dbx"])
        .map_err(|e| OnsError::Data(format!("{conquest_bundle}: {e}")))?
        .into_iter()
        .next()
        .ok_or_else(|| OnsError::Data("KeyError: 'gamemodes/conquest.dbx'".into()))?;
    let conquest_dbx = joins(&work, &[lv, "Conquest.dbx"]);
    write(&conquest_dbx, &data)?;
    let q = bc2_formats::dbx::to_xml(&data).map_err(|e| OnsError::Data(format!("{conquest_dbx}: {e}")))?;
    let head = HEAD
        .captures(&q)
        .ok_or_else(|| OnsError::Data("not found: partition head".into()))?;
    let (qg, qi) = (head[1].to_owned(), head[2].to_owned());
    let con_mode_part = mode_partition_guid(&a.console, &gm_guid)?;
    let mut mode = q
        .replace(&qg, &con_mode_part)
        .replace(&qi, &gm_guid.to_uppercase())
        .replace(r#"id="Conquest""#, r#"id="Onslaught""#);
    for (k, v) in MODE_FIELDS {
        mode = field(&mode, k, v);
    }
    let have_mode = std::path::Path::new(&joins(&src, &["GameModes", "Onslaught.dbx"])).is_file();
    let mut out = StageOut::default();
    out.printed.push(format!(
        "{lv}: {}{}",
        changes.join("; "),
        if have_mode {
            ""
        } else {
            "; GameModes/Onslaught.dbx added (from PC Conquest)"
        }
    ));
    if !a.write {
        return Ok(out);
    }

    // apply: the level and the mode partition imported, the bundle packed
    let tmp = joins(&work, &[lv, &format!("tmp{}", std::process::id())]);
    rmtree(&tmp)?;
    std::fs::create_dir_all(&tmp).map_err(|e| super::io_err(&tmp, &e))?;
    let x = join(&tmp, "level.xml");
    write_text_utf8(&x, &new_level)?;
    let lout = join(&tmp, "level.dbx");
    let said = dbx(&["import", &x, &lout, "--like", &level_dbx, "--types", &con_level_dbx])?;
    let last = bc2_pyrt::py::splitlines(bc2_pyrt::py::strip(&said))
        .last()
        .copied()
        .ok_or_else(|| OnsError::Data("IndexError: list index out of range".into()))?
        .to_owned();
    out.printed.push(format!("  {last}"));
    write(&level_dbx, &read(&lout)?)?;
    let mx = join(&tmp, "Onslaught.xml");
    write_text_utf8(&mx, &mode)?;
    let mdbx = joins(&src, &["GameModes", "Onslaught.dbx"]);
    if let Some(d) = std::path::Path::new(&mdbx).parent() {
        std::fs::create_dir_all(d).map_err(|e| super::io_err(&d.to_string_lossy(), &e))?;
    }
    dbx(&["import", &mx, &mdbx, "--like", &conquest_dbx])?;
    let sz = size(&mdbx)?;
    let man = join(&src, "fbrb.manifest");
    let mut lines: Vec<String> = bc2_pyrt::py::splitlines(&read_text_utf8(&man)?)
        .into_iter()
        .filter(|l| !l.starts_with("GameModes/Onslaught.dbx|"))
        .map(str::to_owned)
        .collect();
    // after the bundle's Conquest mode where it has one (Nelson Bay), else at the end (Valparaiso has none)
    let i = lines
        .iter()
        .position(|l| l.starts_with("GameModes/Conquest.dbx|"))
        .unwrap_or_else(|| lines.len().wrapping_sub(1));
    let at = i.wrapping_add(1).min(lines.len());
    lines.insert(at, format!("GameModes/Onslaught.dbx|<non-resource>|00010000|{sz}|{sz}"));
    write(&man, (lines.join("\n") + "\n").as_bytes())?;
    let mut files = Vec::new();
    walk_all(&src, &mut files);
    for f in files.iter().filter(|f| f.ends_with(".xml")) {
        std::fs::remove_file(f).map_err(|e| super::io_err(f, &e))?;
    }
    rmtree(&tmp)?;
    let built = joins(&work, &[lv, "level-00.fbrb"]);
    pack(&src, &built)?;
    out.writes.push((bundle, Source::File(built)));
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        let _ = (
            &*INST,
            &*OFFSET,
            &*HEAD,
            &*GAMEMODE,
            &*SPM,
            &*EXPL,
            &*GUID_REF,
            &*PC_GAMEMODE,
        );
        let _ = (&*TRANSFORM, &*REFOBJ, &*RADIUS, &*NAME);
        assert_eq!(
            field(r#"<field name="A">1</field><field name="A">2</field>"#, "A", "x"),
            r#"<field name="A">x</field><field name="A">2</field>"#
        );
        assert_eq!(enum_offset(r#"<field name="EnumOffset">17</field>"#).unwrap(), 17);
        assert!(enum_offset("").is_err());
    }

    #[test]
    fn doors() {
        let t = |g: &str, ty: &str, body: &str| format!("<instance guid=\"{g}\" type=\"{ty}\">{body}</instance>\n");
        let tr = |x: f64| format!("<complex name=\"Transform\">1/0/0/0/0/1/0/0/0/0/1/0/{x}/0/0/1</complex>");
        let level = t(
            "D",
            "Entity.ReferenceObjectData",
            &format!(
                "<field ref=\"objects/door/x/1\" name=\"ReferencedObject\" />{}",
                tr(0.0)
            ),
        ) + &t(
            "P1",
            "Entity.PredestructionEntityData",
            &format!("{}<field name=\"Radius\">3</field>", tr(4.0)),
        ) + &t(
            "P2",
            "Entity.PredestructionEntityData",
            &format!("{}<field name=\"Radius\">3</field>", tr(5.0)),
        ) + "\t\t\t<item ref=\"p1\" />\n";
        let con = t("p1", "Entity.PredestructionEntityData", "") + &t("p2", "Entity.PredestructionEntityData", "");
        let (out, d) = drop_door_predestruction(&level, &con).unwrap();
        assert_eq!(d, ["P1"]);
        assert!(!out.contains("\"P1\"") && out.contains("\"P2\""));
    }
}
