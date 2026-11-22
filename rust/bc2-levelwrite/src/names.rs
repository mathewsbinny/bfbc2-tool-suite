//! Every name of an exported level, edited and checked together (round EX1, `EDITOR_ARCHITECTURE.md` 3.12 "The
//! level's names").
//!
//! What the data says (OPFOR's game copy: 83 level descriptions, every level folder, `maps.ini`, OPFOR's server
//! browser `src\dll\server_browser.cpp`, the loader movie's `initLoadingScreen`):
//!
//! - **The level id** (`Levels/<ID>`) is what `maps.ini`'s `<Mode>=Levels/<ID>`, the description's `LevelName`, the
//!   level partition `Levels/<ID>.dbx`, `LevelData.Name` and the menus' `LevelListQuery` items (`<id>.dbx`,
//!   `<id>_description.dbx`) name. DICE's ids mix cases (`BC1_Oasis_CQ`); this export writes them upper case.
//! - **The folder** `Package\levels\<folder>\` is the id in lower case on every level of the game copy: the game finds
//!   a level's bundles there (`package.mft`'s `BundleSource game:levels/<folder>/level-00.fbrb`, `bundleManifest`'s
//!   `levels/<folder>/...` lines). It cannot differ from the id; the form shows it and an edit of it renames the id.
//! - **The on-screen name and description** are the localisation texts of the description's `Name` / `Description`
//!   keys, per language (`Dist\win32\overlay-00.fbrb`, eight languages on OPFOR's copy). DICE gives every mode of a
//!   map the map's keys (`MP_005GR` reads `MP_005_NAME`); an export's keys are `<ID>_NAME` / `<ID>_DESC`. The menus'
//!   map list, the loading screen's panel and the server browser's map column show the name; the loading screen
//!   finds its picture by the name key cut at its FIRST `_NAME` (`Awards/images/loadingScreens/<prefix>`).
//! - **The server-browser name** is `maps.ini`'s `name=` of the section the level's id names (OPFOR's browser looks
//!   the section up by the level's last path part, any case): the launcher shows it, and each browser row is
//!   `OPFOR - <name> (<mode>)`, cut at 159 characters.
//!
//! [`check`] gives each broken rule in plain words: a name that clashes with a stock level or another project's
//! installed export (or with a localisation text or loading picture the game already has), a character or length the
//! files cannot hold, an id holding `_NAME` (a black loading screen), a folder that is not the id. `mapexport check`
//! and the Export window run it; the stage check proves the names landed consistently (the start-up checksum crash of
//! a level registered with missing bundles, or a new name not registered, and the black loading screen of a
//! `_NAME` prefix mismatch).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use bc2_formats::fbrb;
use serde::{Deserialize, Serialize};

use crate::export::Game;

/// The shortest level id.
pub const MIN_ID: usize = 3;
/// The longest level id.
pub const MAX_ID: usize = 40;
/// The longest on-screen name.
pub const MAX_NAME: usize = 64;
/// The longest description.
pub const MAX_DESCRIPTION: usize = 400;
/// OPFOR's server browser row (`OPFOR - <name> (<mode>)`) is cut past this many characters.
pub const BROWSER_ROW: usize = 159;

/// A language's own texts (empty: the default's).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LangText {
    /// The on-screen name in this language.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The description in this language.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Every name of the new level (the Export window's Names, and the project's `Op::SetNames`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelNames {
    /// The in-game level id (`Levels/<level>`).
    pub level: String,
    /// The folder under `Package\levels\` (empty: the id in lower case, the only folder the game accepts).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    /// The launcher's and the server browser's name (`maps.ini`'s `name=`).
    pub browser: String,
    /// The level has its own on-screen name, description and picture (false: the base level's keys and picture).
    #[serde(default)]
    pub own: bool,
    /// The on-screen name, every language without its own.
    #[serde(default)]
    pub name: String,
    /// The description, every language without its own.
    #[serde(default)]
    pub description: String,
    /// Per language (`english`, `french` ...): its own texts.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub languages: BTreeMap<String, LangText>,
}

impl LevelNames {
    /// The id as the export writes it (trimmed, upper case).
    #[must_use]
    pub fn id(&self) -> String {
        self.level.trim().to_ascii_uppercase()
    }

    /// The folder the level is installed in: the id in lower case.
    #[must_use]
    pub fn folder(&self) -> String {
        self.id().to_ascii_lowercase()
    }

    /// The on-screen name and description in `lang` (its own, else the default; the name falls back on the browser
    /// name when none is given).
    #[must_use]
    pub fn texts(&self, lang: &str) -> (String, String) {
        let own = self.languages.get(lang);
        let name = own
            .map(|l| l.name.trim())
            .filter(|n| !n.is_empty())
            .or_else(|| Some(self.name.trim()).filter(|n| !n.is_empty()))
            .unwrap_or_else(|| self.browser.trim());
        let desc = own
            .map(|l| l.description.trim())
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| self.description.trim());
        (name.to_owned(), desc.to_owned())
    }

    /// The names in words (History).
    #[must_use]
    pub fn describe(&self) -> String {
        format!("Names: {} \"{}\"", self.id(), self.browser.trim())
    }
}

/// Which name a problem is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// The level id.
    Level,
    /// The folder.
    Folder,
    /// The server-browser name.
    Browser,
    /// The on-screen name (`None`: the default).
    Name(Option<String>),
    /// The description.
    Description(Option<String>),
}

/// A broken rule, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The rule it breaks (round V1: [`crate::rules`], `XVAL-NAME-*`).
    pub code: &'static str,
    /// The name it is about.
    pub field: Field,
    /// What is wrong and what to do.
    pub message: String,
}

/// The game copy's language tables and texts: what a name in each language can hold, and which keys exist.
#[derive(Debug, Clone, Default)]
pub struct Languages {
    /// Each language's histogram table (`language_<lang>_histogram.bin`).
    pub tables: BTreeMap<String, Vec<u16>>,
    /// The keys (hashes) every language file has, with the English text.
    pub keys: BTreeMap<u32, String>,
}

impl Languages {
    /// Reads `Dist\win32\overlay-00.fbrb`'s localisation (or nothing when it is missing).
    #[must_use]
    pub fn read(game: &Game) -> Self {
        Self::read_bundle(&game.bundle("game:overlay-00.fbrb"))
    }

    /// Reads a bundle's localisation (the overlay, or an install's backup of it).
    #[must_use]
    pub fn read_bundle(bundle: &Path) -> Self {
        let mut out = Self::default();
        let Ok(h) = fbrb::entries_path(bundle) else {
            return out;
        };
        let mut names = Vec::new();
        for e in &h.entries {
            let low = e.name.to_ascii_lowercase();
            if !e.is_deleted()
                && let Some(rest) = low.strip_prefix("localization/bin/language_")
                && (rest.ends_with("_histogram.bin") || rest == "english_normal.bin")
            {
                names.push(e.name.clone());
            }
        }
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let Ok(data) = fbrb::read_path(bundle, &refs) else {
            return out;
        };
        for (n, d) in names.iter().zip(&data) {
            let low = n.to_ascii_lowercase();
            if let Some(lang) = low
                .strip_prefix("localization/bin/language_")
                .and_then(|r| r.strip_suffix("_histogram.bin"))
            {
                if let Ok(t) = bc2_formats::loc::histogram(d) {
                    out.tables.insert(lang.to_owned(), t);
                }
            } else if let Ok(f) = bc2_formats::loc::parse(d) {
                for (i, e) in f.entries.iter().enumerate() {
                    let text = String::from_utf8_lossy(f.raw(d, i)).into_owned();
                    out.keys.insert(e.0, text);
                }
            }
        }
        out
    }

    /// The languages, in order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.tables.keys().cloned().collect()
    }

    /// The English text of a key the game has (`WIN32_<key>` first, as the game reads), if any.
    #[must_use]
    pub fn text_of(&self, key: &str) -> Option<&str> {
        let h = |k: &str| bc2_formats::loc::khash_str(k);
        self.keys
            .get(&h(&format!("WIN32_{key}")))
            .or_else(|| self.keys.get(&h(key)))
            .map(String::as_str)
    }

    /// A text in a language's encoding: `Err` names the first character its table has no code for.
    ///
    /// # Errors
    /// A character the language cannot store.
    pub fn encode(&self, lang: &str, text: &str) -> Result<Vec<u8>, char> {
        bc2_formats::loc::encode(text, self.tables.get(lang).map(Vec::as_slice))
    }
}

/// What the game copy already uses: levels, other projects' installed exports, the server browser's names, the
/// loading pictures.
#[derive(Debug, Clone, Default)]
pub struct Taken {
    /// Level ids (lower case) the game copy has, with why in words.
    pub levels: BTreeMap<String, String>,
    /// Installed exports (lower case): the project id that installed each.
    pub exports: BTreeMap<String, String>,
    /// The server browser's names (lower case): the `maps.ini` section that has each.
    pub browser: BTreeMap<String, String>,
    /// The loading pictures (`awards/images/loadingscreens/<name>.res`, lower case names).
    pub pictures: BTreeSet<String>,
}

fn folders(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir).map_or_else(
        |_| Vec::new(),
        |r| {
            r.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
                .collect()
        },
    )
}

/// The installed export's project id (`opfor-mapedit-installed.json`'s `project`, `<id> (<file>)`).
fn export_of(game: &Game, low: &str) -> Option<String> {
    let p = game.bundle(&format!("package:levels/{low}/opfor-mapedit-installed.json"));
    let text = std::fs::read_to_string(p).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let project = v.get("project")?.as_str()?;
    Some(project.split(" (").next().unwrap_or(project).to_owned())
}

/// `maps.ini` as sections: (section, `name=`, the levels its mode keys name).
#[must_use]
pub fn maps_ini(text: &str) -> Vec<(String, String, Vec<String>)> {
    let mut out: Vec<(String, String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let s = line.trim();
        if s.starts_with(';') || s.starts_with('#') || s.is_empty() {
            continue;
        }
        if let Some(sec) = s.strip_prefix('[').and_then(|r| r.split(']').next()) {
            out.push((sec.trim().to_owned(), String::new(), Vec::new()));
        } else if let (Some((k, v)), Some(last)) = (s.split_once('='), out.last_mut()) {
            if k.trim().eq_ignore_ascii_case("name") {
                last.1 = v.trim().to_owned();
            } else if let Some(lv) = v.trim().strip_prefix("Levels/") {
                last.2.push(lv.to_owned());
            }
        }
    }
    out
}

impl Taken {
    /// What the game copy at `game` uses.
    #[must_use]
    pub fn read(game: &Game) -> Self {
        let mut t = Self::default();
        for (top, what) in [
            (
                game.root.join("Package").join("levels"),
                "a level folder of game\\Package",
            ),
            (
                game.root.join("Dist").join("win32").join("levels"),
                "a level folder of game\\Dist",
            ),
        ] {
            for f in folders(&top) {
                if let Some(p) = export_of(game, &f) {
                    t.exports.insert(f, p);
                } else {
                    t.levels.entry(f).or_insert_with(|| what.to_owned());
                }
            }
        }
        for s in game.sources() {
            let low = s.to_ascii_lowercase();
            if let Some(lv) = low
                .split_once(':')
                .map_or(low.as_str(), |(_, r)| r)
                .strip_prefix("levels/")
                .and_then(|r| r.split('/').next())
                && !t.exports.contains_key(lv)
            {
                t.levels
                    .entry(lv.to_owned())
                    .or_insert_with(|| "registered in package.mft".to_owned());
            }
        }
        if let Ok(text) = std::fs::read_to_string(game.root.join("maps.ini")) {
            for (sec, name, levels) in maps_ini(&text) {
                if !name.is_empty() {
                    t.browser.entry(name.to_ascii_lowercase()).or_insert(sec.clone());
                }
                for lv in levels {
                    let low = lv.to_ascii_lowercase();
                    if !t.exports.contains_key(&low) {
                        t.levels.entry(low).or_insert_with(|| "named in maps.ini".to_owned());
                    }
                }
            }
        }
        if let Ok(live) = game.live(&game.ondemand_awards()) {
            #[allow(clippy::iter_over_hash_type, reason = "collected into a sorted set")]
            for k in live.keys() {
                if let Some(n) = k
                    .strip_prefix("awards/images/loadingscreens/")
                    .and_then(|r| r.strip_suffix(".res"))
                {
                    t.pictures.insert(n.to_owned());
                }
            }
        }
        t
    }
}

/// The first character of `t` outside printable ASCII.
fn not_ascii(t: &str) -> Option<char> {
    t.chars().find(|c| !(' '..='~').contains(c))
}

/// The language's name as the form shows it (`english` -> `English`).
#[must_use]
pub fn language_title(lang: &str) -> String {
    let mut c = lang.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// Checks the names together: each broken rule in words (empty: all is well). `project` is the project's id (its own
/// earlier install may be replaced); `mode` the mode's `maps.ini` key (`Conquest`); `taken` and `langs` what the game
/// copy has (without them only the rules of form are checked).
#[must_use]
#[allow(clippy::too_many_lines, reason = "the rules in order, each with its message")]
pub fn check(
    names: &LevelNames,
    project: Option<&str>,
    mode: &str,
    taken: Option<&Taken>,
    langs: Option<&Languages>,
) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut say = |code: &'static str, field: Field, message: String| out.push(Problem { code, field, message });
    let id = names.id();
    let low = id.to_ascii_lowercase();
    // the level id
    let mut id_ok = true;
    if id.is_empty() {
        say(
            "XVAL-NAME-1",
            Field::Level,
            "Give the level an id: the name the game loads it by (Levels/<id>), its folder and its files.".into(),
        );
        id_ok = false;
    } else if let Some(c) = id.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '_')) {
        say(
            "XVAL-NAME-1",
            Field::Level,
            format!(
                "The level id may hold only the letters A-Z, digits and _ (it is a file and folder name in the game's \
                 bundles and a localisation key): {c:?} is not one of them."
            ),
        );
        id_ok = false;
    } else if !(MIN_ID..=MAX_ID).contains(&id.len()) {
        say(
            "XVAL-NAME-1",
            Field::Level,
            format!(
                "The level id must be {MIN_ID} to {MAX_ID} characters long (it has {}).",
                id.len()
            ),
        );
        id_ok = false;
    } else if !id.starts_with(|c: char| c.is_ascii_alphabetic()) {
        say(
            "XVAL-NAME-1",
            Field::Level,
            "The level id must start with a letter, as every level of the game does.".into(),
        );
        id_ok = false;
    }
    if id_ok && let Some(at) = id.find("_NAME") {
        let prefix = id.get(..at).unwrap_or("").to_ascii_lowercase();
        say(
            "XVAL-NAME-2",
            Field::Level,
            format!(
                "The level id must not hold _NAME: the loading screen cuts the name key {id}_NAME at its first _NAME \
                 and would look for the picture {prefix:?} - a black loading screen. Choose an id without _NAME."
            ),
        );
        id_ok = false;
    }
    if id_ok && let Some(t) = taken {
        let ours = |p: &String| project.is_some_and(|me| me.eq_ignore_ascii_case(p));
        if let Some(why) = t.levels.get(&low) {
            say(
                "XVAL-NAME-3",
                Field::Level,
                format!(
                    "{id} is a level of the game copy ({why}): an export never replaces a level of the game. Choose \
                     another id."
                ),
            );
        } else if let Some(p) = t.exports.get(&low).filter(|p| !ours(p)) {
            say(
                "XVAL-NAME-3",
                Field::Level,
                format!(
                    "{id} is installed already, exported by another project ({p}). Choose another id, or uninstall \
                     that level first."
                ),
            );
        } else if names.own && !t.exports.contains_key(&low) && t.pictures.contains(&low) {
            say(
                "XVAL-NAME-3",
                Field::Level,
                format!(
                    "The game already has a loading picture called {low} (another map's): this level's own picture \
                     would replace it. Choose another id."
                ),
            );
        }
        if names.own
            && !t.exports.contains_key(&low)
            && let Some(l) = langs
        {
            let (nk, dk) = crate::export::loc_keys(&id);
            for k in [nk, dk] {
                if let Some(text) = l.text_of(&k) {
                    say(
                        "XVAL-NAME-3",
                        Field::Level,
                        format!(
                            "{k} is already a text of the game ({text:?}): a level with the id {id} would change it \
                             in every menu that shows it. Choose another id."
                        ),
                    );
                }
            }
        }
    }
    // the folder: the id in lower case
    let folder = names.folder.trim();
    if !folder.is_empty() && folder != low {
        say(
            "XVAL-NAME-4",
            Field::Folder,
            format!(
                "The folder must be the level id in lower case ({low}): the game finds a level's bundles in \
                 Package\\levels\\<id>\\, as it does for every one of its levels. Rename the id to rename the folder."
            ),
        );
    }
    // the server browser's (maps.ini) name
    let b = names.browser.as_str();
    if b.trim().is_empty() {
        say(
            "XVAL-NAME-5",
            Field::Browser,
            "Give the map a name for the launcher and the server browser (maps.ini).".into(),
        );
    } else {
        if let Some(c) = not_ascii(b) {
            say(
                "XVAL-NAME-5",
                Field::Browser,
                format!(
                    "The server-browser name may hold only plain ASCII (letters, digits, punctuation and spaces; \
                     maps.ini is plain text): {c:?} is not."
                ),
            );
        }
        if b.trim() != b {
            say(
                "XVAL-NAME-5",
                Field::Browser,
                "The server-browser name must not start or end with a space (maps.ini drops them).".into(),
            );
        }
        if b.contains('"') {
            say(
                "XVAL-NAME-5",
                Field::Browser,
                "The server-browser name must not hold \" (maps.ini reads a quoted value without its quotes).".into(),
            );
        }
        let row = format!("OPFOR - {b} ({mode})");
        if row.len() > BROWSER_ROW {
            say(
                "XVAL-NAME-5",
                Field::Browser,
                format!(
                    "The server-browser name is too long: its row \"OPFOR - <name> ({mode})\" would be cut at \
                     {BROWSER_ROW} characters (it has {}).",
                    row.len()
                ),
            );
        }
        if let Some(t) = taken
            && let Some(sec) = t.browser.get(&b.trim().to_ascii_lowercase())
            && !sec.eq_ignore_ascii_case(&id)
        {
            say(
                "XVAL-NAME-6",
                Field::Browser,
                format!(
                    "The server browser already lists a map called {:?} (maps.ini section [{sec}]): the rows would \
                     look the same. Choose another name.",
                    b.trim()
                ),
            );
        }
    }
    // the on-screen names, per language
    if names.own {
        let langs_known: Vec<String> = langs.map_or_default(Languages::names);
        if names.name.trim().is_empty() && names.browser.trim().is_empty() {
            say(
                "XVAL-NAME-7",
                Field::Name(None),
                "Give the map its on-screen name (the menus, the loading screen and the server browser's map \
                 column show it)."
                    .into(),
            );
        }
        for lang in names.languages.keys() {
            if !langs_known.is_empty() && !langs_known.contains(lang) {
                say(
                    "XVAL-NAME-7",
                    Field::Name(Some(lang.clone())),
                    format!(
                        "The game copy has no {} texts (it has {}).",
                        language_title(lang),
                        langs_known
                            .iter()
                            .map(|l| language_title(l))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
        }
        let mut checked: HashSet<(String, bool)> = HashSet::new();
        let all: Vec<String> = if langs_known.is_empty() {
            vec!["english".into()]
        } else {
            langs_known
        };
        for lang in &all {
            let (name, desc) = names.texts(lang);
            let own = names.languages.get(lang);
            let which = |set: bool| if set { Some(lang.clone()) } else { None };
            let name_own = own.is_some_and(|l| !l.name.trim().is_empty());
            let desc_own = own.is_some_and(|l| !l.description.trim().is_empty());
            for (text, field, max, what, own_text) in [
                (&name, Field::Name(which(name_own)), MAX_NAME, "name", name_own),
                (
                    &desc,
                    Field::Description(which(desc_own)),
                    MAX_DESCRIPTION,
                    "description",
                    desc_own,
                ),
            ] {
                let who = if own_text {
                    format!("The {} {what}", language_title(lang))
                } else {
                    format!("The {what}")
                };
                if text.chars().count() > max {
                    if checked.insert((format!("{who} long"), own_text)) {
                        say(
                            "XVAL-NAME-7",
                            field.clone(),
                            format!(
                                "{who} must be at most {max} characters (it has {}).",
                                text.chars().count()
                            ),
                        );
                    }
                    continue;
                }
                let enc = match langs {
                    Some(l) if !l.tables.is_empty() => l.encode(lang, text).err(),
                    _ => not_ascii(text),
                };
                if let Some(c) = enc {
                    say(
                        "XVAL-NAME-7",
                        field,
                        format!(
                            "{who} holds {c:?}, which the game's {} texts have no letter for (it would not show). \
                             Give {} its own text without it.",
                            language_title(lang),
                            language_title(lang)
                        ),
                    );
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn names(id: &str) -> LevelNames {
        LevelNames {
            level: id.into(),
            browser: "Operation Aurora (Conquest)".into(),
            own: true,
            name: "OPERATION AURORA".into(),
            description: "A beach and a jungle.".into(),
            ..LevelNames::default()
        }
    }

    fn taken() -> Taken {
        let mut t = Taken::default();
        t.levels
            .insert("mp_005".into(), "a level folder of game\\Package".into());
        t.exports.insert("mapedit_mine".into(), "P1".into());
        t.exports.insert("mapedit_theirs".into(), "P2".into());
        t.browser.insert("atacama desert".into(), "MP_005".into());
        t.pictures.insert("sp_05_a".into());
        t
    }

    fn langs() -> Languages {
        let mut l = Languages::default();
        let mut fr = vec![0u16; 256 + 128];
        fr[0x81] = 0xC9; // a one-byte É
        l.tables.insert("english".into(), vec![0u16; 256]);
        l.tables.insert("french".into(), fr);
        l.keys
            .insert(bc2_formats::loc::khash_str("SP_05_A_NAME"), "HEAVY METAL".into());
        l
    }

    fn fields(p: &[Problem]) -> Vec<Field> {
        p.iter().map(|x| x.field.clone()).collect()
    }

    #[test]
    fn good_names_pass() {
        let p = check(
            &names("MAPEDIT_AURORA"),
            Some("P1"),
            "Conquest",
            Some(&taken()),
            Some(&langs()),
        );
        assert!(p.is_empty(), "{p:?}");
        // its own earlier install may be replaced
        assert_eq!(
            check(&names("MAPEDIT_MINE"), Some("P1"), "Conquest", Some(&taken()), None),
            Vec::<Problem>::new()
        );
    }

    #[test]
    fn the_id_rules() {
        let c = |id: &str| check(&names(id), Some("P1"), "Conquest", Some(&taken()), Some(&langs()));
        assert_eq!(fields(&c("")), [Field::Level]);
        assert!(c("MAP-1").first().unwrap().message.contains("'-'"));
        assert!(c("AB").first().unwrap().message.contains("3 to 40"));
        assert!(c(&"A".repeat(41)).first().unwrap().message.contains("3 to 40"));
        assert!(c("1ABC").first().unwrap().message.contains("start with a letter"));
        // the black loading screen
        let black = c("MY_NAME_MAP");
        assert!(
            black.first().unwrap().message.contains("black loading screen"),
            "{black:?}"
        );
        // a stock level, another project's install, a stock picture and text
        assert!(
            c("mp_005")
                .first()
                .unwrap()
                .message
                .contains("a level of the game copy")
        );
        assert!(
            c("MAPEDIT_THEIRS")
                .first()
                .unwrap()
                .message
                .contains("another project (P2)")
        );
        let heavy = c("SP_05_A");
        assert!(
            heavy
                .iter()
                .any(|p| p.message.contains("loading picture called sp_05_a")),
            "{heavy:?}"
        );
        assert!(
            heavy
                .iter()
                .any(|p| p.message.contains("SP_05_A_NAME is already a text")),
            "{heavy:?}"
        );
    }

    #[test]
    fn the_folder_and_browser_rules() {
        let mut n = names("MAPEDIT_X");
        n.folder = "mapedit_y".into();
        assert_eq!(fields(&check(&n, None, "Conquest", None, None)), [Field::Folder]);
        n.folder = "mapedit_x".into();
        assert_eq!(check(&n, None, "Conquest", None, None), Vec::<Problem>::new());
        n.browser = " Atacama Desert".into();
        let p = check(&n, None, "Conquest", Some(&taken()), None);
        assert!(p.iter().any(|x| x.message.contains("space")), "{p:?}");
        assert!(p.iter().any(|x| x.message.contains("already lists a map")), "{p:?}");
        n.browser = "Caf\u{e9}".into();
        assert!(
            check(&n, None, "Conquest", None, None)[0]
                .message
                .contains("plain ASCII")
        );
        n.browser = "x".repeat(140);
        assert!(
            check(&n, None, "Squad Deathmatch", None, None)[0]
                .message
                .contains("cut at 159")
        );
        n.browser = "a \"quoted\" name".into();
        assert!(check(&n, None, "Conquest", None, None)[0].message.contains('"'));
    }

    #[test]
    fn names_per_language() {
        let l = langs();
        let mut n = names("MAPEDIT_X");
        n.languages.insert(
            "french".into(),
            LangText {
                name: "OP\u{c9}RATION AURORE".into(),
                description: String::new(),
            },
        );
        assert_eq!(check(&n, None, "Conquest", None, Some(&l)), Vec::<Problem>::new());
        assert_eq!(n.texts("french").0, "OP\u{c9}RATION AURORE");
        assert_eq!(n.texts("english").0, "OPERATION AURORA");
        // a letter the language's table lacks
        n.name = "OP\u{c9}RATION".into();
        let p = check(&n, None, "Conquest", None, Some(&l));
        assert_eq!(fields(&p), [Field::Name(None)], "{p:?}");
        assert!(p[0].message.contains("English"));
        n.name = "OPERATION".into();
        n.languages.insert("klingon".into(), LangText::default());
        assert!(
            check(&n, None, "Conquest", None, Some(&l))[0]
                .message
                .contains("no Klingon texts")
        );
        n.languages.clear();
        n.name = "x".repeat(65);
        assert!(
            check(&n, None, "Conquest", None, Some(&l))[0]
                .message
                .contains("at most 64")
        );
        // the base's names: no own texts to check
        n.own = false;
        assert_eq!(check(&n, None, "Conquest", None, Some(&l)), Vec::<Problem>::new());
    }

    #[test]
    fn maps_ini_sections() {
        let s = maps_ini(
            "; x\n[MP_005]\nname=Atacama Desert\nConquest=Levels/MP_005\nRush=Levels/MP_005GR\n\n[A]\nname = B \n",
        );
        assert_eq!(
            s[0],
            (
                "MP_005".into(),
                "Atacama Desert".into(),
                vec!["MP_005".into(), "MP_005GR".into()]
            )
        );
        assert_eq!(s[1].1, "B");
    }
}
