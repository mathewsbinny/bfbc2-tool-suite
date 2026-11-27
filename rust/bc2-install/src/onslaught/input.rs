//! Stage 9 of the console Onslaught port (round OR11): the port of Project OPFOR's `console_input.py`'s build and
//! `apply` up to the ledger - PC input for the seats of the console vehicles and weapons a port carries.
//!
//! Valparaiso crashed at load in the data loader's reference reader (0x4EB538, play test 2026-10-30): the console's
//! input data (`Objects/Soldiers/Common/DefaultSoldierInput`, `Objects/Vehicles/Common/Input/LandVehicleInput`: action
//! maps with console `Buttons` arrays) does not fit the PC types. PC seats leave `InputActionMaps` null and take their
//! input from `InputConceptDefinition` and `InputMapping`. So:
//!
//! - every seat whose `InputActionMaps` names the console's input takes the PC set of the same kind
//!   ([`CONSOLE_INPUT`]);
//! - the console vehicles' seats take the PC input of their role, seat by seat ([`role_inputs`]): their PC version's
//!   seat of the same `EntryOrderNumber` ([`COUNTERPARTS`]), or (the Mark V, no PC version) DICE's role rule; a seat
//!   without input is the AI's and stays as it is ([`seat_edits`]); the Mark V's seats are entered from 5 m, as the PC
//!   patrol boat's ([`ENTRY_RADIUS`]);
//! - the console input partitions leave the bundle unless something still names them.
//!
//! The work folder's files are the retired Python's byte for byte (rust/BENCHMARKS.md "OR11"); the ledger's parts stay
//! in OPFOR's `console_input.py`.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::LazyLock;

use bc2_pyrt::py::OMap;
use regex::Regex;

use super::files::{drop_xml, manifest_lines, under, write_manifest};
use super::port::{StageOut, read_text_utf8, walk_all, write_text_utf8};
use super::{OnsError, R, Source, dbx, fbrb, join, joins, pack, read_text, rel, rmtree, size, slash, stem4};

/// The PC soldier input (concepts, mapping).
pub const SOLDIER: (&str, &str) = (
    "input/bfbc2soldierinputconcepts/5780a065-4637-4fe3-9466-f8e589382cb9",
    "input/inputmappings/bfbc2soldierinputmapping/ea5c6c02-95a6-44d3-9c9c-3338661d2075",
);
/// The PC vehicle input (concepts, mapping).
pub const VEHICLE: (&str, &str) = (
    "input/bfbc2vehicleinputconcepts/9c6433f0-a3ac-4c41-b29c-90f890ae5e05",
    "input/inputmappings/bfbc2vehicleinputmapping/b37cb2d9-4357-48b6-a31f-9eeff349ea62",
);
/// The PC gunner's concepts (its mapping is read from the PC T-90's gunner seat).
pub const GUNNER_CONCEPTS: &str = "input/bfbc2vehicleinputconcepts/9c6433f0-a3ac-4c41-b29c-90f890ae5e05";
/// Console input partition -> the PC input of its kind.
pub const CONSOLE_INPUT: [(&str, (&str, &str)); 2] = [
    ("objects/soldiers/common/defaultsoldierinput", SOLDIER),
    ("objects/vehicles/common/input/landvehicleinput", VEHICLE),
];
/// Console vehicle -> its PC version (`None`: none; DICE's role rule).
pub const COUNTERPARTS: [(&str, Option<&str>); 3] = [
    (
        "objects/vehicles/land/t90/t90_ons",
        Some("objects/vehicles/land/t90/t90"),
    ),
    (
        "objects/weapons/stationary/ru_gmg_qlz87/ru_gmg_qlz87_window",
        Some("objects/weapons/stationary/ru_gmg_qlz87/ru_gmg_qlz87_window"),
    ),
    ("objects/vehicles/sea/markv/markv_ons", None),
];
/// The seat entry radius (metres) of a console vehicle with no PC version: the PC patrol boat's (the user's choice,
/// 2026-11-13).
pub const ENTRY_RADIUS: [(&str, &str); 1] = [("objects/vehicles/sea/markv/markv_ons", "5")];

fn re(p: &str) -> Regex {
    #[allow(
        clippy::expect_used,
        reason = "the patterns are constants, each compiled by the tests"
    )]
    Regex::new(p).expect("a constant pattern")
}

/// Python's `\s` in a str pattern.
const WS: &str = r"[\s\x1c-\x1f]";

/// `console_input.INST`.
pub static INST: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?s)(<instance guid="([^"]+)" type="([^"]+)"[^>]*>)(.*?)(</instance>)"#));
static FIELD: LazyLock<Regex> =
    LazyLock::new(|| re(r#"<field ref="([a-z0-9_/\-]+)/[0-9a-fA-F-]{36}" name="InputActionMaps" />"#));
static SEAT_FIELDS: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r#"{WS}*<field ref="[^"]*" name="(?:InputActionMaps|InputConceptDefinition|InputMapping)" />"#
    ))
});
static AT_IAM: LazyLock<Regex> = LazyLock::new(|| re(&format!(r#"{WS}*<field ref="[^"]*" name="InputActionMaps" />"#)));
static ORDER: LazyLock<Regex> = LazyLock::new(|| re(r#"name="EntryOrderNumber">(\d+)<"#));
static CONCEPT: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([^"]*)" name="InputConceptDefinition""#));
static MAPPING: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([^"]*)" name="InputMapping""#));
static COMPONENTS: LazyLock<Regex> = LazyLock::new(|| re(r#"(?s)<array name="Components">(.*?)</array>"#));
static ITEM: LazyLock<Regex> = LazyLock::new(|| re(r#"<item ref="([^"]+)""#));
static RADIUS: LazyLock<Regex> = LazyLock::new(|| re(r#"<field name="EntryRadius">([^<]*)</field>"#));
static REF: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([A-Za-z0-9_/\-]+)/[0-9A-Fa-f-]{36}""#));

/// A seat's input: (concept reference, mapping reference), `"null"` for a missing or null one.
pub type Input = (String, String);

fn input(a: &str, b: &str) -> Input {
    (a.to_owned(), b.to_owned())
}

/// `order(b)`: a seat's EntryOrderNumber.
fn order(b: &str) -> R<String> {
    ORDER
        .captures(b)
        .map(|c| c[1].to_owned())
        .ok_or_else(|| OnsError::Data("not found: EntryOrderNumber".into()))
}

/// `refs(b)`: a seat's (concept, mapping).
fn refs(b: &str) -> Input {
    (
        CONCEPT
            .captures(b)
            .map_or_else(|| "null".to_owned(), |c| c[1].to_owned()),
        MAPPING
            .captures(b)
            .map_or_else(|| "null".to_owned(), |c| c[1].to_owned()),
    )
}

/// `role_inputs(t, counterpart_xml)`: {seat GUID (lower case): (concept, mapping)} for a console vehicle's seats - its
/// PC counterpart's seat of the same EntryOrderNumber, or (no counterpart) DICE's role rule: the driver (order 0) the
/// vehicle set, a seat on a branch with weapons the gunner set (vehicle concepts, `gunner` mapping), any other the
/// soldier set. The seats in the partition's order.
///
/// # Errors
/// A seat without an EntryOrderNumber, or one the counterpart has no seat for.
pub fn role_inputs(t: &str, counterpart: Option<&str>, gunner: &str) -> R<Vec<(String, Input)>> {
    let mut insts: OMap<String, (String, String)> = OMap::default();
    for c in INST.captures_iter(t) {
        insts.insert(c[2].to_lowercase(), (c[3].to_owned(), c[4].to_owned()));
    }
    let seats: Vec<(String, String)> = insts
        .items
        .iter()
        .filter(|(_, (ty, _))| ty.ends_with("PlayerEntryComponentData"))
        .map(|(g, (_, b))| (g.clone(), b.clone()))
        .collect();
    let mut out = Vec::new();
    if let Some(cx) = counterpart {
        let mut pc: HashMap<String, Input> = HashMap::new();
        for c in INST.captures_iter(cx) {
            if c[3].ends_with("PlayerEntryComponentData") {
                pc.insert(order(&c[4])?, refs(&c[4]));
            }
        }
        for (g, b) in seats {
            let o = order(&b)?;
            let Some(v) = pc.get(&o) else {
                return Err(OnsError::Exit(format!(
                    "seat {g}: no PC counterpart seat of EntryOrderNumber {o}"
                )));
            };
            out.push((g, v.clone()));
        }
        return Ok(out);
    }
    let mut parent: HashMap<String, String> = HashMap::new();
    for (g, (_, b)) in &insts.items {
        for r in components(b) {
            parent.insert(r, g.clone());
        }
    }
    for (g, b) in seats {
        let v = if order(&b)? == "0" {
            input(VEHICLE.0, VEHICLE.1)
        } else if armed(&insts, &g, 0) || parent.get(&g).is_some_and(|p| armed(&insts, p, 0)) {
            input(GUNNER_CONCEPTS, gunner)
        } else {
            input(SOLDIER.0, SOLDIER.1)
        };
        out.push((g, v));
    }
    Ok(out)
}

/// The instances a `Components` array names (their GUIDs, lower case).
fn components(b: &str) -> Vec<String> {
    COMPONENTS.captures(b).map_or_else(Vec::new, |c| {
        ITEM.captures_iter(&c[1])
            .map(|i| i[1].rsplit('/').next().unwrap_or_default().to_lowercase())
            .collect()
    })
}

/// `armed(g, depth)`: the instance is a weapon component or has one below it (five levels at most).
fn armed(insts: &OMap<String, (String, String)>, g: &str, depth: u32) -> bool {
    let (ty, b) = insts
        .get(&g.to_owned())
        .map_or(("", ""), |(t, b)| (t.as_str(), b.as_str()));
    if ty.ends_with("WeaponComponentData") {
        return true;
    }
    depth < 5 && components(b).iter().any(|r| armed(insts, r, depth + 1))
}

/// The char index `k` of `a` as a byte index of `b` (Python's `b[:k]` with `k` counted in `a`).
fn byte_at(b: &str, k: usize) -> usize {
    b.char_indices().nth(k).map_or(b.len(), |(i, _)| i)
}

/// `seat_input(body, concept, mapping)`: a `PlayerEntryComponentData` body with `InputActionMaps` null and the given PC
/// input, where `InputActionMaps` was (else before `EntryOrderNumber`).
///
/// # Errors
/// A body with neither field.
pub fn seat_input(body: &str, concept: Option<&str>, mapping: Option<&str>) -> R<String> {
    let at = AT_IAM.find(body).map(|m| body[..m.start()].chars().count());
    let rest = SEAT_FIELDS.replace_all(body, "").into_owned();
    let pos = match at {
        Some(k) => byte_at(&rest, k),
        None => rest
            .find("<field name=\"EntryOrderNumber\">")
            .ok_or_else(|| OnsError::Data("ValueError: substring not found".into()))?,
    };
    let mut new = String::from("\n\t\t<field ref=\"null\" name=\"InputActionMaps\" />");
    if let Some(c) = concept {
        let _ = write!(new, "\n\t\t<field ref=\"{c}\" name=\"InputConceptDefinition\" />");
    }
    if let Some(m) = mapping {
        let _ = write!(new, "\n\t\t<field ref=\"{m}\" name=\"InputMapping\" />");
    }
    Ok(format!("{}{new}{}", &rest[..pos], &rest[pos..]))
}

/// `v.split("/")[-2] if "/" in v else v`: a reference's partition's last name.
fn short(v: &str) -> &str {
    if v.contains('/') {
        let parts: Vec<&str> = v.split('/').collect();
        parts.get(parts.len().saturating_sub(2)).copied().unwrap_or(v)
    } else {
        v
    }
}

/// `seat_edits(part, t, want, roles)`: a console vehicle partition's XML with its seats' input (`want`:
/// [`role_inputs`]) and entry radius ([`ENTRY_RADIUS`]) set; (the new XML, the number of edits). Each edit is
/// reported in `roles`. A seat without input (the AI's) is left as it is, its entry radius too.
///
/// # Errors
/// A player seat of a vehicle with a radius rule but no `EntryRadius` field; a seat without an EntryOrderNumber.
pub fn seat_edits(part: &str, t: &str, want: &[(String, Input)], roles: &mut Vec<String>) -> R<(String, usize)> {
    let want: HashMap<&str, &Input> = want.iter().map(|(g, v)| (g.as_str(), v)).collect();
    let radius = ENTRY_RADIUS.iter().find(|(p, _)| *p == part).map(|(_, r)| *r);
    let name = part.rsplit('/').next().unwrap_or(part);
    let mut out = String::new();
    let mut pos = 0;
    let mut n = 0;
    for c in INST.captures_iter(t) {
        let (Some(m4), Some(m2)) = (c.get(4), c.get(2)) else {
            continue;
        };
        let mut body = m4.as_str().to_owned();
        let g = m2.as_str().to_lowercase();
        if let Some(w) = want.get(g.as_str()) {
            let have = refs(&body);
            if have != input("null", "null") {
                if let Some(radius) = radius {
                    let Some(r) = RADIUS.captures(&body) else {
                        return Err(OnsError::Exit(format!("{part}: seat {g} has no EntryRadius field")));
                    };
                    let (whole, value) = (r[0].to_owned(), r[1].to_owned());
                    if value != radius {
                        body = body.replacen(&whole, &format!("<field name=\"EntryRadius\">{radius}</field>"), 1);
                        let g8: String = g.chars().take(8).collect();
                        roles.push(format!("{name} seat {g8}: entry radius {value} -> {radius}"));
                        n += 1;
                    }
                }
                if have != **w {
                    let opt = |v: &str| if v == "null" { None } else { Some(v.to_owned()) };
                    let (cc, mm) = (opt(&w.0), opt(&w.1));
                    body = seat_input(&body, cc.as_deref(), mm.as_deref())?;
                    let seat_no = order(m4.as_str())?;
                    roles.push(format!(
                        "{name} seat {seat_no}: {}/{} -> {}/{}",
                        short(&have.0),
                        short(&have.1),
                        short(&w.0),
                        short(&w.1)
                    ));
                    n += 1;
                }
            }
        }
        out.push_str(&t[pos..m4.start()]);
        out.push_str(&body);
        pos = m4.end();
    }
    out.push_str(&t[pos..]);
    Ok((out, n))
}

/// `counterpart_xml(part)`: a stock partition's XML from the game copy's index (the PC version of a console vehicle).
fn counterpart_xml(ix: &bc2_convert::meshview::Index, part: &str) -> R<String> {
    let data = ix.read(&format!("{part}.dbx")).ok_or_else(|| {
        OnsError::Exit(format!(
            "{part}: not in the game copy (the PC counterpart of a console vehicle)"
        ))
    })?;
    bc2_formats::dbx::to_xml(&data).map_err(|e| OnsError::Data(format!("{part}: {e}")))
}

/// The PC T-90 gunner seat's mapping (`GUNNER[1]`): seat 1's `InputMapping`, a gunner mapping.
///
/// # Errors
/// The T-90 is not in the game copy, or its seat 1 is not a gunner's.
pub fn gunner_mapping(t90: &str) -> R<String> {
    let mut gm = Vec::new();
    for c in INST.captures_iter(t90) {
        if c[3].ends_with("PlayerEntryComponentData") && c[4].contains("name=\"EntryOrderNumber\">1<") {
            let m = MAPPING
                .captures(&c[4])
                .ok_or_else(|| OnsError::Data("not found: InputMapping".into()))?;
            gm.push(m[1].to_owned());
        }
    }
    match gm.as_slice() {
        [one] if one.contains("gunnerinputmapping") => Ok(one.clone()),
        _ => {
            let list: Vec<String> = gm.iter().map(|s| bc2_pyrt::fmt::str_repr(s)).collect();
            Err(OnsError::Exit(format!(
                "the PC T-90 gunner seat: mapping [{}]",
                list.join(", ")
            )))
        }
    }
}

/// The stage's arguments.
#[derive(Debug, Clone)]
pub struct Args {
    /// Project OPFOR's folder (its game copy's index is `<root>\work\tools_out`'s).
    pub root: String,
    /// Its game copy.
    pub game: String,
    /// The level (lower case).
    pub lv: String,
    /// The bundle the stage builds from (the ledger's `source_bundle`).
    pub src_bundle: String,
    /// `--apply` (else `--plan`: built and reported, nothing packed or installed).
    pub write: bool,
}

/// The XML files under `src`, in `os.walk`'s order.
fn xml_files(src: &str) -> Vec<String> {
    let mut all = Vec::new();
    walk_all(src, &mut all);
    all.into_iter().filter(|f| f.ends_with(".xml")).collect()
}

/// DbxEdit's import of an edited partition's XML over the partition (`--like` it, the T-90's types).
fn import(x: &str, types: &str) -> R<String> {
    let d = format!("{}.dbx", stem4(x));
    let new = format!("{d}.new");
    dbx(&["import", x, &new, "--like", &d, "--types", types])?;
    std::fs::rename(&new, &d).map_err(|e| super::io_err(&d, &e))?;
    Ok(d)
}

/// `build(lv)` and `apply(lv, write)` up to the ledger.
///
/// # Errors
/// A tool's failure, a refusal in words, a file that does not read.
#[allow(clippy::too_many_lines, reason = "console_input.build, step for step")]
pub fn build(a: &Args) -> R<StageOut> {
    let lv = a.lv.as_str();
    let mut out = StageOut::default();
    let work = joins(&a.root, &["work", "console_input"]);
    let bundle = joins(&a.game, &["Package", "levels", lv, "level-00.fbrb"]);
    let src = joins(&work, &[lv, "pkg"]);
    let types = joins(
        &a.root,
        &[
            "work",
            "console_shaders",
            "shared",
            "mp_common_level",
            "Objects",
            "Vehicles",
            "Land",
            "T90",
            "T90.dbx",
        ],
    );
    rmtree(&src)?;
    fbrb(&["unpack", &a.src_bundle, &src])?;
    if !std::path::Path::new(&types).is_file() {
        return Err(OnsError::Exit(format!(
            "no {types} (console_shader_audit.py unpacks mp_common there)"
        )));
    }
    // its return code not checked, as the Python's
    let _ = bc2_tools::dbxedit::run(&["xml".to_owned(), src.clone()], None);
    let mut fixed: Vec<(String, usize)> = Vec::new();
    let mut seats = 0;
    for x in xml_files(&src) {
        let t = read_text_utf8(&x)?;
        if !FIELD
            .captures_iter(&t)
            .any(|m| CONSOLE_INPUT.iter().any(|(p, _)| *p == m[1].to_lowercase()))
        {
            continue;
        }
        let mut new = String::new();
        let mut pos = 0;
        let mut n = 0;
        for c in INST.captures_iter(&t) {
            let Some(m4) = c.get(4) else { continue };
            let mut body = m4.as_str().to_owned();
            if c[3].ends_with("PlayerEntryComponentData")
                && let Some(fm) = FIELD.captures(m4.as_str())
                && let Some((_, (concept, mapping))) = CONSOLE_INPUT.iter().find(|(p, _)| *p == fm[1].to_lowercase())
            {
                body = body.replacen(
                    &fm[0],
                    &format!(
                        "<field ref=\"null\" name=\"InputActionMaps\" />\n\t\t<field ref=\"{concept}\" \
                         name=\"InputConceptDefinition\" />\n\t\t<field ref=\"{mapping}\" name=\"InputMapping\" />"
                    ),
                    1,
                );
                n += 1;
            }
            new.push_str(&t[pos..m4.start()]);
            new.push_str(&body);
            pos = m4.end();
        }
        new.push_str(&t[pos..]);
        write_text_utf8(&x, &new)?;
        let d = import(&x, &types)?;
        fixed.push((slash(&rel(&d, &src)), n));
        seats += n;
    }
    // the console vehicles' seats: their PC counterpart's input seat by seat, or DICE's role rule (COUNTERPARTS)
    let ix = bc2_convert::meshview::Index::load(&a.root, &mut |s| eprintln!("{s}"))
        .map_err(|e| OnsError::Data(format!("the game copy's index: {e}")))?;
    let gunner = gunner_mapping(&counterpart_xml(&ix, "objects/vehicles/land/t90/t90")?)?;
    let mut roles: Vec<String> = Vec::new();
    for x in xml_files(&src) {
        let r = slash(&rel(&x, &src));
        let part = stem4(&r).to_lowercase();
        let Some((_, cp)) = COUNTERPARTS.iter().find(|(p, _)| *p == part) else {
            continue;
        };
        let t = read_text_utf8(&x)?;
        let cpx = match cp {
            Some(c) => Some(counterpart_xml(&ix, c)?),
            None => None,
        };
        let want = role_inputs(&t, cpx.as_deref(), &gunner)?;
        let (new_t, n) = seat_edits(&part, &t, &want, &mut roles)?;
        if n > 0 {
            write_text_utf8(&x, &new_t)?;
            let d = import(&x, &types)?;
            let rd = slash(&rel(&d, &src));
            if !fixed.iter().any(|(f, _)| *f == rd) {
                fixed.push((rd, n));
            }
            seats += n;
        }
    }
    for r in &roles {
        out.printed.push(format!("  seat input by role: {r}"));
    }
    // the console input partitions, unless something still names them
    let mut refs_all: HashSet<String> = HashSet::new();
    for x in xml_files(&src) {
        let t = read_text(&x)?;
        refs_all.extend(REF.captures_iter(&t).map(|c| c[1].to_lowercase()));
    }
    let man = join(&src, "fbrb.manifest");
    let mut lines = manifest_lines(&man)?;
    let mut removed: Vec<String> = Vec::new();
    for (part, _) in CONSOLE_INPUT {
        if refs_all.contains(part) {
            continue;
        }
        let mut keep = Vec::new();
        for l in lines {
            let name = l.split('|').next().unwrap_or_default().to_owned();
            let low = name.to_lowercase();
            let stem = low.rsplit_once('.').map_or(low.as_str(), |(s, _)| s);
            if stem == part {
                let p = under(&src, &name);
                if std::path::Path::new(&p).is_file() {
                    std::fs::remove_file(&p).map_err(|e| super::io_err(&p, &e))?;
                }
                removed.push(name);
            } else {
                keep.push(l);
            }
        }
        lines = keep;
    }
    for l in &mut lines {
        let cols: Vec<&str> = l.split('|').collect();
        if cols.len() >= 4 && fixed.iter().any(|(r, _)| r.to_lowercase() == cols[0].to_lowercase()) {
            let sz = size(&under(&src, cols[0]))?.to_string();
            let mut c: Vec<&str> = cols[..3].to_vec();
            c.extend(std::iter::repeat_n(sz.as_str(), cols.len() - 3));
            *l = c.join("|");
        }
    }
    write_manifest(&man, &lines)?;
    drop_xml(&src)?;
    let detail = if fixed.is_empty() {
        String::new()
    } else {
        let each: Vec<String> = fixed
            .iter()
            .map(|(r, n)| format!("{} {n}", stem4(&bc2_pyrt::py::basename(r))))
            .collect();
        format!(" ({})", each.join(", "))
    };
    out.printed.push(format!(
        "{lv}: {seats} seats given PC input in {} partitions{detail}; console input partitions removed: {}",
        fixed.len(),
        if removed.is_empty() {
            "none".to_owned()
        } else {
            removed.join(", ")
        }
    ));
    let mut still: Vec<&str> = CONSOLE_INPUT
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| refs_all.contains(*p))
        .collect();
    still.sort_unstable();
    if !still.is_empty() {
        out.printed
            .push(format!("  still referenced (kept): {}", still.join(", ")));
    }
    if !a.write {
        return Ok(out);
    }
    if fixed.is_empty() && removed.is_empty() {
        out.printed.push(format!("{lv}: nothing to change"));
        return Ok(out);
    }
    let built = joins(&work, &[lv, "level-00.fbrb"]);
    pack(&src, &built)?;
    out.writes.push((bundle, Source::File(built)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(g: &str, concept: Option<&str>, mapping: Option<&str>, order: u32) -> String {
        let mut s = format!(
            "<instance guid=\"{g}\" type=\"Entity.PlayerEntryComponentData\">\n\t\t<field ref=\"null\" \
             name=\"InputActionMaps\" />\n"
        );
        if let Some(c) = concept {
            let _ = writeln!(s, "\t\t<field ref=\"{c}\" name=\"InputConceptDefinition\" />");
        }
        if let Some(m) = mapping {
            let _ = writeln!(s, "\t\t<field ref=\"{m}\" name=\"InputMapping\" />");
        }
        let _ = writeln!(
            s,
            "\t\t<field name=\"EntryOrderNumber\">{order}</field>\n\t\t<field name=\"EntryRadius\">0</field>\n</instance>"
        );
        s
    }

    /// A seat without input (the AI's) keeps its entry radius: the edit was reported ('0 -> 5') and counted, then
    /// dropped with the rest of the seat (QA 2026-11-14). A player seat gets the radius.
    #[test]
    fn an_ai_seat_and_its_radius_stay() {
        let part = "objects/vehicles/sea/markv/markv_ons";
        let t = seat("aaaaaaaa-0", Some(SOLDIER.0), Some(SOLDIER.1), 1) + &seat("bbbbbbbb-0", None, None, 0);
        let mut roles = Vec::new();
        let want = vec![
            ("aaaaaaaa-0".to_owned(), input(SOLDIER.0, SOLDIER.1)),
            ("bbbbbbbb-0".to_owned(), input(VEHICLE.0, VEHICLE.1)),
        ];
        let (new, n) = seat_edits(part, &t, &want, &mut roles).unwrap_or_default();
        let pieces: Vec<&str> = new.split("</instance>").collect();
        assert!(pieces[0].contains("<field name=\"EntryRadius\">5</field>"));
        assert!(pieces[1].contains("<field name=\"EntryRadius\">0</field>") && !pieces[1].contains("InputMapping"));
        assert_eq!(n, 1);
        assert_eq!(roles, vec!["markv_ons seat aaaaaaaa: entry radius 0 -> 5".to_owned()]);
        // a player seat whose input differs: both edits made and reported
        let mut roles = Vec::new();
        let t = seat("cccccccc-0", Some(VEHICLE.0), Some(VEHICLE.1), 2);
        let want = vec![("cccccccc-0".to_owned(), input(SOLDIER.0, SOLDIER.1))];
        let (new, n) = seat_edits(part, &t, &want, &mut roles).unwrap_or_default();
        assert_eq!(n, 2);
        assert_eq!(roles.len(), 2);
        assert!(new.contains(SOLDIER.1) && new.contains("<field name=\"EntryRadius\">5</field>"));
        assert_eq!(
            roles[1],
            "markv_ons seat 2: bfbc2vehicleinputconcepts/bfbc2vehicleinputmapping -> \
             bfbc2soldierinputconcepts/bfbc2soldierinputmapping"
        );
    }

    #[test]
    fn the_role_rule_without_a_counterpart() {
        let weapon = "<instance guid=\"W\" type=\"Entity.WeaponComponentData\">\n</instance>\n";
        let gun_seat = seat("G", None, None, 1).replace(
            "</instance>",
            "\t\t<array name=\"Components\">\n\t\t\t<item ref=\"x/W\" />\n\t\t</array>\n</instance>",
        );
        let t = seat("D", None, None, 0) + &gun_seat + &seat("P", None, None, 2) + weapon;
        let got = role_inputs(&t, None, "gm").unwrap_or_default();
        assert_eq!(
            got,
            vec![
                ("d".to_owned(), input(VEHICLE.0, VEHICLE.1)),
                ("g".to_owned(), input(GUNNER_CONCEPTS, "gm")),
                ("p".to_owned(), input(SOLDIER.0, SOLDIER.1)),
            ]
        );
        // a counterpart: its seat of the same order; a seat it lacks refused
        let cp = seat("X", Some("c0"), Some("m0"), 0);
        assert!(matches!(role_inputs(&t, Some(&cp), "gm"), Err(OnsError::Exit(_))));
        let got = role_inputs(&seat("D", None, None, 0), Some(&cp), "gm").unwrap_or_default();
        assert_eq!(got, vec![("d".to_owned(), input("c0", "m0"))]);
    }

    #[test]
    fn seat_input_takes_the_action_maps_place() {
        let body =
            "\n\t\t<field ref=\"x/y\" name=\"InputActionMaps\" />\n\t\t<field name=\"EntryOrderNumber\">1</field>";
        let got = seat_input(body, Some("c"), None).unwrap_or_default();
        assert_eq!(
            got,
            "\n\t\t<field ref=\"null\" name=\"InputActionMaps\" />\n\t\t<field ref=\"c\" \
             name=\"InputConceptDefinition\" />\n\t\t<field name=\"EntryOrderNumber\">1</field>"
        );
        assert!(seat_input("nothing", None, None).is_err());
    }

    #[test]
    fn the_t90_gunner_mapping() {
        let t = seat("A", Some("c"), Some("input/inputmappings/bfbc2gunnerinputmapping/g"), 1);
        assert_eq!(
            gunner_mapping(&t).ok().as_deref(),
            Some("input/inputmappings/bfbc2gunnerinputmapping/g")
        );
        let bad = seat("A", Some("c"), Some("m"), 1);
        assert!(matches!(gunner_mapping(&bad), Err(OnsError::Exit(m)) if m.ends_with("mapping ['m']")));
    }

    #[test]
    fn patterns_compile() {
        let _ = (&*FIELD, &*SEAT_FIELDS, &*AT_IAM, &*COMPONENTS, &*RADIUS, &*REF, &*ITEM);
    }
}
