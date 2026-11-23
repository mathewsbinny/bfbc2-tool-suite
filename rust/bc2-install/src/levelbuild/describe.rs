//! The startup bundle's partitions the install writes (`wake_install.description`, `air_superiority_mode`,
//! `level_list_items` / `level_list_xml` / `level_list_query`): a level's description (a Vietnam description with
//! new guids and the level's names), OPFOR's Air Superiority game mode, and the menus' level catalog naming every
//! installed WW2 level. Each is DbxEdit's XML of the donor edited as text and imported `--like` the donor, in the
//! folders the Python used (the install oracle hashes them).

use std::fs;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::index::primary_guid_of;
use super::{Result, bundle_one, pjoin, to_xml};

/// `cc.run(cc.DBX, ...)` / `cc.dbx_run(...)`: DbxEdit's command line in process.
#[must_use]
pub fn dbxedit(args: &[&str]) -> bc2_tools::dbxedit::Outcome {
    let a: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    bc2_tools::dbxedit::run(&a, None)
}

fn dbx_xml(like: &str) -> Result<()> {
    let o = dbxedit(&["xml", like]);
    if o.code != 0 {
        return Err(format!("failed: DbxEdit xml {like}\n{}\n{}", o.stdout, o.stderr));
    }
    Ok(())
}

fn tail_chars(s: &str, n: usize) -> String {
    let c = s.chars().count();
    s.chars().skip(c.saturating_sub(n)).collect()
}

/// `DbxEdit import <xml> <out> --like <like>`, refused (with the last 2,000 characters of its output) when it fails
/// or guessed a type.
fn import(what: &str, xp: &str, out: &str, like: &str) -> Result<()> {
    let o = dbxedit(&["import", xp, out, "--like", like]);
    let text = format!("{}{}", o.stdout, o.stderr);
    if o.code != 0 || text.contains("guess") {
        return Err(format!("{what} import: {}", tail_chars(&text, 2000)));
    }
    Ok(())
}

/// A text file read as Python's text mode reads it (UTF-8; `\r\n` and `\r` as `\n`).
fn read_universal(p: &str) -> Result<String> {
    let b = fs::read(p).map_err(|e| format!("{p}: {e}"))?;
    let s = String::from_utf8(b).map_err(|e| format!("UnicodeDecodeError: {p}: {e}"))?;
    Ok(s.replace("\r\n", "\n").replace('\r', "\n"))
}

fn read_raw(p: &str) -> Result<String> {
    let b = fs::read(p).map_err(|e| format!("{p}: {e}"))?;
    String::from_utf8(b).map_err(|e| format!("UnicodeDecodeError: {p}: {e}"))
}

static GUID: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"guid="([0-9A-F-]{36})""#).ok());
static PRIMARY: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"primaryInstance="([0-9A-F-]{36})""#).ok());

/// Every upper-case guid of the XML replaced by `new(guid)` (upper and lower case spellings), in sorted order.
fn new_guids(x: &str, new: impl Fn(&str) -> String) -> String {
    let mut gs: Vec<String> = GUID.as_ref().map_or_else(Vec::new, |r| {
        r.captures_iter(x)
            .filter_map(|c| c.get(1))
            .map(|m| m.as_str().to_owned())
            .collect()
    });
    gs.sort();
    gs.dedup();
    let mut x = x.to_owned();
    for g in gs {
        let n = new(&g);
        x = x.replace(&g, &n).replace(&g.to_lowercase(), &n.to_lowercase());
    }
    x
}

/// `re.subn(pattern, f, x)`: every match replaced by `f(captures)`, and how many.
fn subn(pattern: &str, x: &str, f: impl Fn(&Captures<'_>) -> String) -> Result<(String, usize)> {
    let r = Regex::new(pattern).map_err(|e| format!("{pattern}: {e}"))?;
    let n = r.find_iter(x).count();
    Ok((r.replace_all(x, |c: &Captures<'_>| f(c)).into_owned(), n))
}

fn g<'a>(c: &'a Captures<'_>, i: usize) -> &'a str {
    c.get(i).map_or("", |m| m.as_str())
}

/// `opfor_guid('ww2', *parts)`: `uuid5(NAMESPACE_URL, 'opfor-ww2:<parts joined by :>')`, upper case.
#[must_use]
pub fn ww2_guid(parts: &[&str]) -> String {
    bc2_formats::guid::new_guid("opfor-ww2", parts)
}

/// What the Air Superiority mode is made of (OPFOR's `AS_MODE`, `AS_DONOR`, `AS_FIELDS`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModeSpec {
    /// `work\ww2` (the mode is built in `install_mode`).
    pub w: String,
    /// The startup bundle the donor is read from.
    pub startup: String,
    /// The donor partition (`GameModes/NamConquest.dbx`).
    pub donor: String,
    /// The mode's partition (`GameModes/AirSuperiority`).
    pub mode: String,
    /// The fields set, in order: (field, value).
    pub fields: Vec<(String, String)>,
}

/// `air_superiority_mode(report)`: `GameModes/AirSuperiority.dbx` (new guids from the donor's): its path and primary
/// instance guid.
///
/// # Errors
/// The donor or DbxEdit failed, or not every edit matched.
pub fn air_superiority_mode(spec: &ModeSpec, report: &mut Vec<String>) -> Result<(String, String)> {
    let tmp = pjoin(&spec.w, "install_mode");
    fs::create_dir_all(&tmp).map_err(|e| format!("{tmp}: {e}"))?;
    let like = pjoin(&tmp, "donor.dbx");
    fs::write(&like, bundle_one(&spec.startup, &spec.donor)?).map_err(|e| format!("{like}: {e}"))?;
    dbx_xml(&like)?;
    let x = read_universal(&format!("{}.xml", like.get(..like.len() - 4).unwrap_or_default()))?;
    let x = new_guids(&x, |g| ww2_guid(&["airsuperiority", g]));
    let (mut x, mut n) = subn(r#"id="NamConquest""#, &x, |_| r#"id="AirSuperiority""#.to_owned())?;
    for (k, v) in &spec.fields {
        let (y, m) = subn(
            &format!(r#"(<field name="{}">)[^<]*(</field>)"#, regex::escape(k)),
            &x,
            |c| format!("{}{v}{}", g(c, 1), g(c, 2)),
        )?;
        x = y;
        n += m;
    }
    if n != 1 + spec.fields.len() {
        return Err(format!(
            "air superiority mode: {n} of {} edits matched in {}",
            1 + spec.fields.len(),
            spec.donor
        ));
    }
    let xp = pjoin(&tmp, "AirSuperiority.xml");
    fs::write(&xp, &x).map_err(|e| format!("{xp}: {e}"))?;
    let out = pjoin(&tmp, "AirSuperiority.dbx");
    import("air superiority mode", &xp, &out, &like)?;
    let guid = PRIMARY
        .as_ref()
        .and_then(|r| r.captures(&x))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
        .ok_or("no primaryInstance in the text")?;
    report.push(format!(
        "game mode: {} (NamConquest's, ModeID airsuperiority, matchmaking CONQUEST)",
        spec.mode
    ));
    Ok((out, guid))
}

/// What a level's description is made of (the level's bindings).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DescSpec {
    /// The level's staging folder (`WORK`; the description is built in `desc`).
    pub work: String,
    /// The startup bundle the donor is read from.
    pub startup: String,
    /// The donor (`Levels/NAM_MP_002CQ_description.dbx`).
    pub donor: String,
    /// `NAME` (`WAKE_ISLAND_S`).
    pub name: String,
    /// `DESC_SEED`: the new guids' seed.
    pub seed: String,
    /// `DESC_TEXT`: the description's loc key.
    pub text: String,
    /// `MINIMAP` and the path of its partition.
    pub minimap: (String, String),
    /// `MUSIC` and the path of its partition.
    pub music: (String, String),
    /// The Air Superiority mode when the level runs it (`MODE == 'air_superiority'`).
    pub mode: Option<ModeSpec>,
}

/// `description(ours, report)`: `Levels/<NAME>_description` - the donor's with new guids, the level's names, minimap
/// and music, no intro voice-over, `PACIFIC` matchmaking (and the Air Superiority mode for a level that runs it).
///
/// # Errors
/// The donor or DbxEdit failed, or an edit matched nothing.
pub fn description(spec: &DescSpec, report: &mut Vec<String>) -> Result<String> {
    let tmp = pjoin(&spec.work, "desc");
    fs::create_dir_all(&tmp).map_err(|e| format!("{tmp}: {e}"))?;
    let like = pjoin(&tmp, "donor.dbx");
    fs::write(&like, bundle_one(&spec.startup, &spec.donor)?).map_err(|e| format!("{like}: {e}"))?;
    dbx_xml(&like)?;
    let x = read_universal(&format!("{}.xml", like.get(..like.len() - 4).unwrap_or_default()))?;
    let mut x = new_guids(&x, |gd| ww2_guid(&[&spec.seed, gd]));
    let mg = primary_guid_of(&spec.minimap.1)?.unwrap_or_default().to_lowercase();
    let mu = primary_guid_of(&spec.music.1)?.unwrap_or_default().to_lowercase();
    let name = &spec.name;
    let mut subs: Vec<(String, Box<dyn Fn(&Captures<'_>) -> String>)> = vec![
        (
            r#"(<field name="Name">)Levels/NAM_MP_002CQ_description(</field>)"#.into(),
            Box::new(move |c| format!("{}Levels/{name}_description{}", g(c, 1), g(c, 2))),
        ),
        (
            r#"id="NAM_MP_002CQ_description""#.into(),
            Box::new(move |_| format!(r#"id="{name}_description""#)),
        ),
        (
            r#"(<field name="LevelName">)[^<]*(</field>)"#.into(),
            Box::new(move |c| format!("{}Levels/{name}{}", g(c, 1), g(c, 2))),
        ),
        (
            r#"(<field name="Name">)NAM_MP_002_NAME(</field>)"#.into(),
            Box::new(move |c| format!("{}{name}_NAME{}", g(c, 1), g(c, 2))),
        ),
        (
            r#"(<field name="Description">)NAM_MP_002_DESC(</field>)"#.into(),
            Box::new(|c| format!("{}{}{}", g(c, 1), spec.text, g(c, 2))),
        ),
        (
            r#"ref="[^"]*" (name="Music")"#.into(),
            Box::new(move |c| format!(r#"ref="{}/{mu}" {}"#, spec.music.0, g(c, 1))),
        ),
        (
            r#"ref="[^"]*" (name="MinimapTexture")"#.into(),
            Box::new(move |c| format!(r#"ref="{}/{mg}" {}"#, spec.minimap.0, g(c, 1))),
        ),
        (r"\n\t*<item>Sound/VO/[^<]*</item>".into(), Box::new(|_| String::new())),
        (
            r#"(<field name="MatchmakingModID">)VIETNAM(</field>)"#.into(),
            Box::new(|c| format!("{}PACIFIC{}", g(c, 1), g(c, 2))),
        ),
    ];
    if let Some(mode) = &spec.mode {
        let (_, mode_guid) = air_superiority_mode(mode, report)?;
        let mode_ref = format!("{}/{}", mode.mode.to_lowercase(), mode_guid.to_lowercase());
        subs.push((
            r#"ref="gamemodes/namconquest/[0-9a-f-]{36}" (name="GameMode")"#.into(),
            Box::new(move |c| format!(r#"ref="{mode_ref}" {}"#, g(c, 1))),
        ));
    }
    for (pat, rep) in &subs {
        let (y, n) = subn(pat, &x, |c| rep(c))?;
        if n == 0 {
            return Err(format!("description: nothing matched {pat}"));
        }
        x = y;
    }
    let xp = pjoin(&tmp, &format!("{name}_description.xml"));
    fs::write(&xp, &x).map_err(|e| format!("{xp}: {e}"))?;
    let out = pjoin(&tmp, &format!("{name}_description.dbx"));
    import("description", &xp, &out, &like)?;
    report.push(format!(
        "description: Levels/{name}_description (NAM_MP_002CQ's: namconquest, Vietnam kit pictures; Wake's minimap \
         and loading music; no intro voice-over)"
    ));
    Ok(out)
}

/// `level_list_items(items, levels)`: the list's items with every WW2 level's names out (`ww2_item`: the pattern
/// naming them), then each level's `<lv>.dbx` and `<lv>_description.dbx` in; sorted, as DICE's list is.
#[must_use]
pub fn level_list_items(items: &[String], levels: &[String], ww2_item: &Regex) -> Vec<String> {
    let mut keep: std::collections::BTreeSet<String> =
        items.iter().filter(|i| !ww2_item.is_match(i)).cloned().collect();
    for lv in levels {
        keep.insert(format!("{lv}.dbx"));
        keep.insert(format!("{lv}_description.dbx"));
    }
    keep.into_iter().collect()
}

/// The pattern of a WW2 level's list items (`WW2_LIST_ITEM`).
///
/// # Errors
/// A level name the pattern cannot hold.
pub fn ww2_list_item(ww2_levels: &[String]) -> Result<Regex> {
    Regex::new(&format!(r"(?i)^({})(_description)?\.dbx$", ww2_levels.join("|"))).map_err(|e| e.to_string())
}

static RESULTS: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"(?s)(<array name="Results">)(.*?)(</array>)"#).ok());
static LEAD: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(\r*\n)(\t*)<item>").ok());
static ITEM: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"<item>([^<]*)</item>").ok());

fn items_of(x: &str) -> Vec<String> {
    ITEM.as_ref().map_or_else(Vec::new, |r| {
        r.captures_iter(x)
            .filter_map(|c| c.get(1))
            .map(|m| m.as_str().to_owned())
            .collect()
    })
}

/// `level_list_xml(x, levels)`: the `LevelListQuery` XML with its `Results` set by [`level_list_items`], the items'
/// line break and indentation kept.
///
/// # Errors
/// The XML has no `Results` array.
pub fn level_list_xml(x: &str, levels: &[String], ww2_item: &Regex, list: &str) -> Result<String> {
    let m = RESULTS
        .as_ref()
        .and_then(|r| r.captures(x))
        .and_then(|c| c.get(2))
        .ok_or_else(|| format!("{list}: no Results array"))?;
    let body = m.as_str();
    let (nl, ind) = LEAD
        .as_ref()
        .and_then(|r| r.captures(body))
        .map_or(("\n".to_owned(), "\t\t\t".to_owned()), |c| {
            (g(&c, 1).to_owned(), g(&c, 2).to_owned())
        });
    let tail = match body.rfind("</item>") {
        Some(i) => body.get(i + "</item>".len()..).unwrap_or_default().to_owned(),
        None => format!("{nl}{}", ind.get(..ind.len().saturating_sub(1)).unwrap_or_default()),
    };
    let items = level_list_items(&items_of(body), levels, ww2_item);
    let mut new = String::new();
    for i in &items {
        new.push_str(&nl);
        new.push_str(&ind);
        new.push_str("<item>");
        new.push_str(i);
        new.push_str("</item>");
    }
    new.push_str(&tail);
    Ok(format!(
        "{}{new}{}",
        x.get(..m.start()).unwrap_or_default(),
        x.get(m.end()..).unwrap_or_default()
    ))
}

/// What the level list is made of.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ListSpec {
    /// Wake's staging folder (the list is built in `levellist`).
    pub work: String,
    /// The startup bundle as it was before the WW2 install (its ledger copy) or as it is.
    pub source: String,
    /// `LevelListQuery.dbx`.
    pub entry: String,
    /// Every WW2 level (what the list takes out first).
    pub ww2_levels: Vec<String>,
    /// The levels installed: (lv, NAME).
    pub levels: Vec<(String, String)>,
}

/// `level_list_query(report, levels)`: `LevelListQuery.dbx` naming the installed WW2 levels, built in
/// `<work>\levellist` and read back.
///
/// # Errors
/// The donor or DbxEdit failed, or the built list is not the wanted one.
pub fn level_list_query(spec: &ListSpec, report: &mut Vec<String>) -> Result<String> {
    let tmp = pjoin(&spec.work, "levellist");
    fs::create_dir_all(&tmp).map_err(|e| format!("{tmp}: {e}"))?;
    let like = pjoin(&tmp, "donor.dbx");
    fs::write(&like, bundle_one(&spec.source, &spec.entry)?).map_err(|e| format!("{like}: {e}"))?;
    dbx_xml(&like)?;
    let x = read_raw(&format!("{}.xml", like.get(..like.len() - 4).unwrap_or_default()))?;
    let item = ww2_list_item(&spec.ww2_levels)?;
    let lvs: Vec<String> = spec.levels.iter().map(|(lv, _)| lv.clone()).collect();
    let xp = pjoin(&tmp, "LevelListQuery.xml");
    fs::write(&xp, level_list_xml(&x, &lvs, &item, &spec.entry)?).map_err(|e| format!("{xp}: {e}"))?;
    let out = pjoin(&tmp, &spec.entry);
    import("LevelListQuery", &xp, &out, &like)?;
    let got = items_of(&to_xml(&super::read(&out)?)?);
    let want = level_list_items(&items_of(&x), &lvs, &item);
    if got != want {
        return Err(format!(
            "LevelListQuery: the built list differs from the wanted one ({} / {} items)",
            got.len(),
            want.len()
        ));
    }
    let names: Vec<&str> = spec.levels.iter().map(|(_, n)| n.as_str()).collect();
    report.push(format!(
        "level list: LevelListQuery.dbx names {} ({} items) - the catalog the menus list levels from",
        names.join(", "),
        got.len()
    ));
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn level_list_as_the_python() {
        let lv = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<String>>();
        let item = ww2_list_item(&lv(&["wake_island_s", "guadal_canal", "iwo_jima_s", "coral_sea"])).unwrap();
        let stock = lv(&["MP_005.dbx", "mp_005_description.dbx"]);
        let mut with = stock.clone();
        with.extend(lv(&["coral_sea.dbx", "Wake_Island_S.dbx"]));
        let got = level_list_items(&with, &lv(&["wake_island_s"]), &item);
        assert_eq!(
            got,
            lv(&[
                "MP_005.dbx",
                "mp_005_description.dbx",
                "wake_island_s.dbx",
                "wake_island_s_description.dbx"
            ])
        );
        assert_eq!(level_list_items(&got, &[], &item), stock);
        let x = "<a>\r\n\t\t<array name=\"Results\">\r\n\t\t\t<item>MP_005.dbx</item>\r\n\t\t</array>\r\n</a>";
        let y = level_list_xml(x, &lv(&["wake_island_s"]), &item, "L").unwrap();
        assert!(y.contains("\r\n\t\t\t<item>wake_island_s.dbx</item>\r\n\t\t\t<item>wake_island_s_description.dbx</item>\r\n\t\t</array>"));
        assert_eq!(level_list_xml(&y, &[], &item, "L").unwrap(), x);
        assert!(level_list_xml("<instance/>", &[], &item, "L").is_err());
        let empty = "<array name=\"Results\"></array>";
        assert_eq!(
            level_list_xml(empty, &lv(&["x"]), &item, "L").unwrap(),
            "<array name=\"Results\">\n\t\t\t<item>x.dbx</item>\n\t\t\t<item>x_description.dbx</item>\n\t\t</array>"
        );
    }

    #[test]
    fn guids_of_python() {
        // opfor_guid('ww2', 'wake-description', g) is uuid5 of 'opfor-ww2:wake-description:<g>'
        assert_eq!(
            ww2_guid(&["a", "b"]),
            bc2_formats::guid::new_guid("opfor-ww2:a", &["b"])
        );
    }
}
