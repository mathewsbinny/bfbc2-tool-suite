//! Battlefield 1943's effects in BFBC2 (round OR8, "OPFOR in Rust": `bfbc2-bf1943-import/effectport.py`, its
//! Python retired): an effect, the emitters it plays, the textures they draw.
//!
//! What an effect is (both games): an `EffectAsset` partition holding an `EffectEntityData` whose components are
//! `EmitterEntityData` (each naming an emitter document) and `SoundEntityData` (a sound). BF1943 keeps the emitter
//! documents per level (`levels/<level>/emitters/...`: the partition inside keeps the global name `emitters/...`),
//! each made of `Emitter.*` data instances and naming its textures (`textures/effects/...`).
//!
//! How it is ported:
//! - every partition the effect reaches is walked ([`walk`]): emitter documents are taken from the level's copy
//!   ([`level_copy`]), textures converted ([`crate::texture::convert`]), sounds and anything else BFBC2 has under the
//!   same name kept as references;
//! - each partition goes through [`crate::dbxport::port`] on BFBC2 templates ([`templates`]: [`TEMPLATES`] and the
//!   first BFBC2 emitter documents in name order that hold a type still needed, found through [`EmitterIndex`]);
//!   [`DEFAULTS`] checks the templates give DICE's usual values for the fields BFBC2 added;
//! - a BF1943 sound BFBC2 lacks is not ported here: [`port`] reports it (`sound` records, [`sounds_of`]) for the
//!   sound porter ([`crate::soundport`]);
//! - a reference that will resolve to nothing is null and reported (dbxport: no template value stands in for it).
//!
//! The game data comes through two traits: [`Source`] (BF1943's resources) and [`Stock`] (the game copy's index).

#![allow(
    clippy::implicit_hasher,
    clippy::too_many_lines,
    reason = "the module's sets are std's; the port's steps kept in one function, in the reference's order"
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use bc2_formats::dbx;
use rayon::prelude::*;
use regex::Regex;

use crate::dbxport::py::lower;
use crate::dbxport::rules::Rules;
use crate::dbxport::{self, Arena, CatView, Fail, Options, Ours, SiKey, SiVal, Templates};

/// BFBC2 partitions whose instances serve as templates first: DICE's small generic detonation.
pub const TEMPLATES: &[&str] = &["fx/explostions/detonations/generic/fx_detonation_s"];
/// The DICE-only fields whose values vary over DICE's own effects: `(type, field, the value the templates must give)`.
pub const DEFAULTS: &[(&str, &str, &str)] = &[
    ("EmitterEntityData", "SpawnProbability", "1"),
    ("EffectEntityData", "TriggerPostEffect", "false"),
    ("EffectEntityData", "PostEffectContrastOffset", "0"),
    ("EffectEntityData", "PostEffectFadeInTime", "0"),
    ("EffectEntityData", "PostEffectFadeOutTime", "0"),
    ("EffectEntityData", "PostEffectSizeScale", "1"),
];
/// The dbxport records whose lines the effect report shows (all of them are in the records).
pub const SHOWN: &[&str] = &[
    "substituted",
    "nulled",
    "relocated",
    "pruned",
    "guarded",
    "value",
    "UNMAPPED",
    "left out instance",
    "type",
    "untemplated",
    "nested",
    "renamed",
    "enum",
    "schema",
];
/// Emitter documents read and indexed at a time.
pub const EMITTER_BATCH: usize = 2000;
/// The emitter index file's version (2: this module's JSON; 1 was the Python's pickle).
pub const INDEX_VERSION: u32 = 2;
/// The texture every effect may name that the resolver keeps (DICE's default normal map).
const DEFAULT_NORMAL: &str = "textures/effects/emittersystem/defaultnormalmap";

static REF: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"ref="([^"]+)/[0-9a-fA-F-]{36}""#).ok());

// ------------------------------------------------------------------------------------------------ errors

/// Why an effect (or a sound) did not port.
#[derive(Debug)]
pub enum Error {
    /// A callback of the caller's raised (the binding holds the exception).
    Callback,
    /// A partition or file that does not read (`ValueError`).
    Value(String),
    /// A bundle of the game copy that does not read (the bundle, the reader's error).
    Bundle(String, bc2_formats::fbrb::FbrbError),
    /// A template partition the game copy lacks (`dbxport.MissingTemplate`).
    MissingTemplate(String),
    /// A texture that does not convert (`texture.ConvertError`).
    Texture(String),
    /// The partition port's own failure (`PortError`, `SchemaError`, a case it does not take).
    Port(Fail),
    /// A file that cannot be written.
    Io(String),
    /// The sound porter's stop (`SystemExit`): instances of a type no donor partition has.
    Exit(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Callback => f.write_str("a callback raised"),
            Self::Value(m) | Self::MissingTemplate(m) | Self::Texture(m) | Self::Io(m) | Self::Exit(m) => {
                f.write_str(m)
            }
            Self::Bundle(b, e) => write!(f, "{b}: {e}"),
            Self::Port(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<Fail> for Error {
    fn from(e: Fail) -> Self {
        match e {
            Fail::Callback => Self::Callback,
            Fail::Parse(m) => Self::Value(m),
            e => Self::Port(e),
        }
    }
}

/// `Result` with [`Error`].
pub type Res<T> = Result<T, Error>;

// ------------------------------------------------------------------------------------------------ the game data

/// One BF1943 resource: its name as the bundle holds it, its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `e['name']`.
    pub name: String,
    /// `e.get('type')`.
    pub kind: Option<String>,
}

/// BF1943's resources by key (`bf1943import.Source`: lower-case names).
pub trait Source: Sync {
    /// `src.entries.get(key)`.
    fn entry(&self, key: &str) -> Option<&Entry>;
    /// The keys, in any order (for [`levels_of`]).
    fn keys(&self) -> Vec<&str>;
    /// `src.data(key)`.
    ///
    /// # Errors
    /// A resource that does not read.
    fn data(&self, key: &str) -> Res<Vec<u8>>;
}

/// The game copy's index (`meshview.Index`, or a view of it that hides names).
pub trait Stock: Sync {
    /// `(bundle, entry name)` of an entry by its lower-case name (`ix.where.get(name)`).
    fn locate(&self, low: &str) -> Option<(&str, &str)>;
    /// The lower-case entry names (`ix.where`'s keys), in any order.
    fn names(&self) -> Vec<&str>;
    /// `ix.read(name)`: an entry's bytes, `None` when the game copy lacks it.
    ///
    /// # Errors
    /// A bundle that does not read.
    fn read(&self, name: &str) -> Res<Option<Vec<u8>>>;
}

/// The BFBC2 partition names (the index's `.dbx` entries without the extension).
#[must_use]
pub fn bc2_partitions(ix: &dyn Stock) -> HashSet<String> {
    ix.names()
        .into_iter()
        .filter_map(|n| n.strip_suffix(".dbx"))
        .map(str::to_owned)
        .collect()
}

/// A lower-case index name ending `.dbx` (the index's names are lower case).
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the index's names are lower case"
)]
fn is_dbx(k: &str) -> bool {
    k.ends_with(".dbx")
}

fn has(ix: &dyn Stock, low: &str) -> bool {
    ix.locate(low).is_some()
}

fn xml(data: &[u8]) -> Res<String> {
    dbx::to_xml(data).map_err(|e| Error::Value(e.to_string()))
}

/// The partitions a partition's XML names (`REF`'s first groups, lower case), sorted and unique.
///
/// # Errors
/// A partition that does not read.
pub fn refs(data: &[u8]) -> Res<Vec<String>> {
    let x = xml(data)?;
    let mut out: Vec<String> = REF
        .as_ref()
        .into_iter()
        .flat_map(|r| r.captures_iter(&x))
        .filter_map(|c| c.get(1).map(|m| lower(m.as_str())))
        .collect();
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// BF1943's level folders (`levels/<name>`) that hold level copies of emitter documents, sorted.
#[must_use]
pub fn levels_of(src: &dyn Source) -> Vec<String> {
    let mut out: Vec<String> = src
        .keys()
        .into_iter()
        .filter(|k| k.starts_with("levels/") && k.matches('/').count() >= 2)
        .map(|k| k.split('/').take(2).collect::<Vec<_>>().join("/"))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The source key of a level copy of an emitter document (BF1943 keeps them per level, under `emitters/...` and
/// under the fx folders' `emitter_<size>/` folders alike): the given level's, else (`fallback`) the first other
/// level's that has one - BF1943's Wake lacks the tree destruction's three emitters, which Guadalcanal, Iwo Jima and
/// Coral Sea have. `levels`: [`levels_of`] (made once by the caller).
#[must_use]
pub fn level_copy(src: &dyn Source, levels: &[String], name: &str, level: &str, fallback: bool) -> Option<String> {
    let k = format!("{level}/{name}.dbx");
    if src.entry(&k).is_some() {
        return Some(k);
    }
    if fallback {
        for lv in levels {
            let k = format!("{lv}/{name}.dbx");
            if lv != level && src.entry(&k).is_some() {
                return Some(k);
            }
        }
    }
    None
}

/// Whether a partition is an effect that draws particles (an `EffectAsset` with an `EmitterEntityData`): a
/// projectile's sound and tracer in one (BF1943's `bsfx_we_gen75mm_projectilefx` under `sound/`) is ported as an
/// effect, its sounds handed over; a plain sound wrapper is the sound porter's.
#[must_use]
pub fn effect_with_emitters(data: &[u8]) -> bool {
    let has = |n: &[u8]| memchr::memmem::find(data, n).is_some();
    has(b"Entity.EffectAsset") && has(b"Emitter.EmitterEntityData")
}

// ------------------------------------------------------------------------------------------------ the walk

/// What an effect reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// BF1943's, BFBC2 lacks it: ported.
    Port,
    /// A BF1943 texture BFBC2 lacks: converted.
    Texture,
    /// BFBC2's own, kept as a reference.
    Bfbc2,
    /// A BF1943 sound BFBC2 lacks: the sound porter's.
    Sound,
    /// In neither game.
    Missing,
}

impl Kind {
    /// The Python's name of the kind.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Port => "port",
            Self::Texture => "texture",
            Self::Bfbc2 => "bfbc2",
            Self::Sound => "sound",
            Self::Missing => "missing",
        }
    }
}

/// One partition the effect reaches: its name, the source key it is read from, what is done with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The partition (or texture) name.
    pub name: String,
    /// The source key (`None`: nothing to read).
    pub key: Option<String>,
    /// What is done with it.
    pub kind: Kind,
}

/// Everything an effect reaches, in the walk's order. An emitter document is the level's copy ([`level_copy`]:
/// another level's when the level has none and `fallback` is set - the source key says which).
///
/// # Errors
/// A partition that does not read.
pub fn walk(src: &dyn Source, ix: &dyn Stock, effect: &str, level: &str, fallback: bool) -> Res<Vec<Item>> {
    let mut levels: Option<Vec<String>> = None;
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut todo: std::collections::VecDeque<String> = std::collections::VecDeque::from([lower(effect)]);
    let item = |name: &str, key: Option<String>, kind| Item {
        name: name.to_owned(),
        key,
        kind,
    };
    while let Some(name) = todo.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let dbx_name = format!("{name}.dbx");
        if name.starts_with("textures/") {
            let res = format!("{name}.res");
            if has(ix, &dbx_name) || has(ix, &res) {
                out.push(item(&name, None, Kind::Bfbc2)); // BFBC2's own (DICE's PC textures: often larger)
            } else if src
                .entry(&res)
                .is_some_and(|e| e.kind.as_deref() == Some("XenonTexture"))
            {
                out.push(item(&name, Some(name.clone()), Kind::Texture));
                if src.entry(&dbx_name).is_some() {
                    out.push(item(&name, Some(dbx_name), Kind::Port));
                }
            } else {
                out.push(item(&name, None, Kind::Missing));
            }
            continue;
        }
        if has(ix, &dbx_name) && !name.starts_with("effects/") && !name.starts_with("emitters/") {
            out.push(item(&name, None, Kind::Bfbc2)); // sounds and the like: BFBC2's own
            continue;
        }
        if name.starts_with("sound/") && src.entry(&dbx_name).is_some() && !effect_with_emitters(&src.data(&dbx_name)?)
        {
            // a BF1943 sound BFBC2 lacks: the sound porter's (its sound templates, its waves converted) - aligning
            // it on effect / emitter templates would leave its Audio.* types unaligned and its waves out
            out.push(item(&name, Some(dbx_name), Kind::Sound));
            continue;
        }
        let mut key = dbx_name.clone();
        if src.entry(&key).is_none() {
            let lv = levels.get_or_insert_with(|| levels_of(src));
            key = level_copy(src, lv, &name, level, fallback).unwrap_or(key);
        }
        if src.entry(&key).is_none() {
            let kind = if has(ix, &dbx_name) { Kind::Bfbc2 } else { Kind::Missing };
            out.push(item(&name, None, kind));
            continue;
        }
        let next = refs(&src.data(&key)?)?;
        out.push(item(&name, Some(key), Kind::Port));
        todo.extend(next.into_iter().filter(|r| !seen.contains(r)));
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ the records

/// One thing the port did (`dbxport.Record`, with the partition a gathering caller sets and a caller's own line).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// `path`.
    pub path: String,
    /// `action`.
    pub action: String,
    /// `old`.
    pub old: Option<String>,
    /// `new`.
    pub new: Option<String>,
    /// `source`.
    pub source: Option<String>,
    /// `step`.
    pub step: Option<String>,
    /// `note`.
    pub note: Option<String>,
    /// `partition`.
    pub partition: Option<String>,
    /// `text`: the caller's own line.
    pub text: Option<String>,
}

impl Record {
    fn of(r: &dbxport::Record) -> Self {
        Self {
            path: r.path.clone(),
            action: r.action.to_owned(),
            old: r.old.clone(),
            new: r.new.clone(),
            source: r.source.clone(),
            step: r.step.clone(),
            note: r.note.clone(),
            partition: None,
            text: None,
        }
    }

    fn said(path: &str, action: &str, text: String) -> Self {
        Self {
            path: path.to_owned(),
            action: action.to_owned(),
            text: Some(text),
            ..Self::default()
        }
    }

    /// `Record.line()`: the caller's own line, else the line of its action.
    #[must_use]
    pub fn line(&self) -> String {
        if let Some(t) = &self.text {
            return t.clone();
        }
        let (a, p) = (self.action.as_str(), self.path.as_str());
        let s = |o: &Option<String>| o.clone().unwrap_or_else(|| "None".to_owned());
        let frm = self
            .source
            .as_ref()
            .filter(|s| !s.is_empty())
            .map_or(String::new(), |s| format!(" from {s}"));
        let note = self.note.as_ref().filter(|n| !n.is_empty());
        match a {
            "added" => {
                let v = self
                    .new
                    .as_ref()
                    .filter(|n| !n.is_empty())
                    .map_or(String::new(), |n| format!(": {n}"));
                format!("added {p} (BFBC2's value{v}{frm})")
            }
            "left out" => format!("left out {p} (BF1943 only)"),
            "duplicate" => format!("left out {p} (a second field of that name: BF1943's duplicate)"),
            "left out instance" => format!("left out instance {p}: {}", s(&self.note)),
            "type" => format!("type {p} written as {}", s(&self.new)),
            "untemplated" => format!("type {p}: no BFBC2 template - kept as BF1943 has it"),
            "template" => format!("template {p}: {}{frm}", s(&self.step)),
            "value" | "convention" | "bones" => {
                let why = note.map_or(String::new(), |n| format!(" ({n})"));
                format!("{a} {p}: {} -> {}{why}", s(&self.old), s(&self.new))
            }
            "UNMAPPED" => format!(
                "UNMAPPED {p}: BF1943 number {} has no known BFBC2 name - the template's value",
                s(&self.old)
            ),
            "relocated" => format!(
                "relocated {p}: {} -> {} (the guid is in that BFBC2 partition)",
                s(&self.old),
                s(&self.new)
            ),
            "substituted" => format!(
                "substituted {p} (refers to {}): {} ({}{frm})",
                s(&self.old),
                s(&self.new),
                s(&self.note)
            ),
            "nulled" | "unresolved" => format!(
                "nulled {p} (refers to {}): {}",
                s(&self.old),
                note.map_or("no BFBC2 value", String::as_str)
            ),
            "guarded" => format!("guarded {p}: {}{frm} refused ({}) - null", s(&self.new), s(&self.note)),
            "pruned" => format!("pruned {p}: {}", s(&self.note)),
            "emptied" => format!("emptied {p}: {}", s(&self.note)),
            "converted" | "renamed" | "nested" => {
                format!("{a} {p}{}", note.map_or(String::new(), |n| format!(": {n}")))
            }
            _ => {
                let mut out = format!("{a} {p}");
                if self.old.is_some() || self.new.is_some() {
                    let _ = write!(out, ": {} -> {}", s(&self.old), s(&self.new));
                }
                if let Some(n) = note {
                    let _ = write!(out, " ({n})");
                }
                out
            }
        }
    }
}

/// A report: its lines and the records they were made from (`dbxport.Report`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// The lines.
    pub lines: Vec<String>,
    /// The records.
    pub records: Vec<Record>,
}

impl Report {
    fn say(&mut self, path: &str, action: &str, text: String) {
        self.lines.push(text.clone());
        self.records.push(Record::said(path, action, text));
    }
}

/// The BF1943 sound partitions a report left for a sound porter (its `sound` records), sorted.
#[must_use]
pub fn sounds_of(report: &Report) -> Vec<String> {
    let mut out: Vec<String> = report
        .records
        .iter()
        .filter(|r| r.action == "sound")
        .map(|r| r.path.clone())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Python's `repr` of a list of strings.
#[must_use]
pub fn py_list(items: &[String]) -> String {
    let one = |s: &str| {
        let q = if s.contains('\'') && !s.contains('"') {
            '"'
        } else {
            '\''
        };
        let mut o = String::from(q);
        for c in s.chars() {
            match c {
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                '\r' => o.push_str("\\r"),
                '\t' => o.push_str("\\t"),
                c if c == q => {
                    o.push('\\');
                    o.push(c);
                }
                c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                    let _ = write!(o, "\\x{:02x}", c as u32);
                }
                c => o.push(c),
            }
        }
        o.push(q);
        o
    };
    format!("[{}]", items.iter().map(|s| one(s)).collect::<Vec<_>>().join(", "))
}

// ------------------------------------------------------------------------------------------------ templates

/// The rules and catalogue every partition port of a call takes.
pub struct PortContext<'a> {
    /// `dbxport.py`'s data tables as the caller has them.
    pub rules: &'a Rules,
    /// The catalogue (its class defaults), when the caller passed one.
    pub catalogue: Option<CatView<'a>>,
}

/// The instance types of a partition (`None` for an instance without one).
///
/// # Errors
/// A partition that does not read, or has no partition element.
pub fn instance_types(data: &[u8]) -> Res<Vec<Option<String>>> {
    if data.is_empty() || !dbx::is_binary(data) {
        return Err(Error::Value("not a binary .dbx file".to_owned()));
    }
    let tree = dbx::parse_tree(data).map_err(|e| Error::Value(e.to_string()))?;
    let p = tree
        .partition()
        .ok_or_else(|| Error::Value("no partition".to_owned()))?;
    if !p.is_container() {
        return Err(Error::Value("the partition holds no instances".to_owned()));
    }
    Ok(tree.instances().map(|n| n.attr("type").map(str::to_owned)).collect())
}

/// `dbxport.Templates(sources)` of `(name, bytes)` partitions (lower-case names): a partition the game copy lacks
/// is an error (`MissingTemplate`); one that does not read is left out (the reference's `skipped`).
///
/// # Errors
/// A missing partition, or one the port cannot hold.
pub fn build_templates(items: Vec<(String, Option<Vec<u8>>)>, rules: &Rules) -> Res<Templates> {
    let mut checked = Vec::with_capacity(items.len());
    for (name, data) in items {
        let Some(data) = data else {
            return Err(Error::MissingTemplate(format!(
                "template partition {name}: not in the game copy"
            )));
        };
        checked.push((name, data));
    }
    let parsed: Vec<Option<Arena>> = checked
        .par_iter()
        .map(|(_, d)| {
            if d.is_empty() || !dbx::is_binary(d) {
                return None;
            }
            dbx::parse_tree(d).ok().map(|t| Arena::from_tree(&t))
        })
        .collect();
    let mut t = Templates::new();
    for ((name, _), arena) in checked.into_iter().zip(parsed) {
        let Some(arena) = arena else { continue };
        let key = lower(&name.strip_suffix(".dbx").unwrap_or(&name).replace('\\', "/"));
        t.add(key, arena, rules).map_err(Error::from)?;
    }
    Ok(t)
}

/// What [`templates`] chose: the templates, the pool's partition names, the types none of them holds.
pub struct Chosen {
    /// The templates.
    pub templates: Templates,
    /// The partitions, in the order added.
    pub pool: Vec<String>,
    /// The types still needed when the search ended.
    pub need: HashSet<Option<String>>,
}

/// One emitter document's row: its instance types and those of them its bytes name, or `None` where it does not
/// read.
type Row = Option<(HashSet<String>, HashSet<String>)>;

/// Which instance types each BFBC2 emitter document holds, for [`templates`]' greedy search: per document (by
/// name) its row, read a batch of [`EMITTER_BATCH`] documents at a time in name order, only as far as a search
/// goes. A full index is kept on disk ([`cache_dir`]), keyed by the documents' places and their bundles' sizes and
/// times, and read back while they are unchanged; one index per game copy in a process.
pub struct EmitterIndex {
    /// The documents (`<...>/emitters/<...>.dbx` entries), sorted.
    pub cands: Vec<String>,
    places: Vec<(String, String)>,
    rows: HashMap<String, Row>,
    loaded: HashSet<usize>,
    key: String,
    /// Batches read from the bundles.
    pub batches_read: usize,
    /// Whether the rows came from the disk cache.
    pub from_disk: bool,
}

static INDEXES: LazyLock<Mutex<HashMap<String, Arc<Mutex<EmitterIndex>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Where [`EmitterIndex`] keeps its files: `BFBC2_EFFECTPORT_CACHE` (a folder; `0`: none), else the suite's cache
/// folder `effectport` ([`bc2_gamefs::budget`]). `None`: no disk cache.
#[must_use]
pub fn cache_dir() -> Option<String> {
    bc2_gamefs::budget::spec("effectport").and_then(|s| bc2_gamefs::budget::folder(s, None))
}

impl EmitterIndex {
    /// The process's index of the game copy `ix` shows (made, or read from the disk cache, on first use).
    #[must_use]
    pub fn of_index(ix: &dyn Stock) -> Arc<Mutex<Self>> {
        let mut cands: Vec<String> = ix
            .names()
            .into_iter()
            .filter(|k| k.contains("/emitters/") && is_dbx(k))
            .map(str::to_owned)
            .collect();
        cands.sort_unstable();
        let places: Vec<(String, String)> = cands
            .iter()
            .map(|n| {
                ix.locate(n)
                    .map_or_else(Default::default, |(b, e)| (b.to_owned(), e.to_owned()))
            })
            .collect();
        let key = Self::key_of(&cands, &places);
        let mut all = INDEXES.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(i) = all.get(&key) {
            return Arc::clone(i);
        }
        let mut me = Self {
            cands,
            places,
            rows: HashMap::new(),
            loaded: HashSet::new(),
            key: key.clone(),
            batches_read: 0,
            from_disk: false,
        };
        me.load_disk();
        let me = Arc::new(Mutex::new(me));
        all.insert(key, Arc::clone(&me));
        me
    }

    /// Forgets the process's indexes (tests).
    pub fn forget() {
        INDEXES.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }

    fn key_of(cands: &[String], places: &[(String, String)]) -> String {
        let mut h = bc2_gamefs::sha256::Sha256::default();
        h.update(format!("{INDEX_VERSION}\n").as_bytes());
        let mut bundles: Vec<&str> = Vec::new();
        for (n, (b, e)) in cands.iter().zip(places) {
            h.update(format!("{n}\t{b}\t{e}\n").as_bytes());
            bundles.push(b);
        }
        bundles.sort_unstable();
        bundles.dedup();
        for b in bundles {
            match std::fs::metadata(b) {
                Ok(m) => {
                    let t = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos());
                    let abs = std::path::absolute(b).unwrap_or_else(|_| PathBuf::from(b));
                    h.update(
                        format!(
                            "{}\t{}\t{t}\n",
                            bc2_gamefs::budget::normcase(&abs.to_string_lossy()),
                            m.len()
                        )
                        .as_bytes(),
                    );
                }
                Err(_) => h.update(format!("{b}\tmissing\n").as_bytes()),
            }
        }
        h.hexdigest()
    }

    fn path(&self) -> Option<PathBuf> {
        let d = cache_dir()?;
        Some(Path::new(&d).join(format!("emitters-{}.json", self.key.get(..24).unwrap_or(&self.key))))
    }

    fn batches(&self) -> usize {
        self.cands.len().div_ceil(EMITTER_BATCH)
    }

    fn load_disk(&mut self) {
        let Some(p) = self.path() else { return };
        let Ok(raw) = std::fs::read(&p) else { return };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&raw) else {
            return;
        };
        if v.get("version").and_then(serde_json::Value::as_u64) != Some(u64::from(INDEX_VERSION))
            || v.get("key").and_then(serde_json::Value::as_str) != Some(self.key.as_str())
        {
            return;
        }
        let Some(rows) = v.get("rows").and_then(serde_json::Value::as_object) else {
            return;
        };
        let strings = |x: Option<&serde_json::Value>| -> Option<HashSet<String>> {
            x?.as_array()?.iter().map(|s| s.as_str().map(str::to_owned)).collect()
        };
        let mut got = HashMap::with_capacity(rows.len());
        for (n, r) in rows {
            let row = if r.as_str() == Some("error") {
                None
            } else {
                let a = r.as_array();
                match (strings(a.and_then(|a| a.first())), strings(a.and_then(|a| a.get(1)))) {
                    (Some(t), Some(p)) => Some((t, p)),
                    _ => return, // damaged: read again
                }
            };
            got.insert(n.clone(), row);
        }
        let _ = bc2_gamefs::budget::utime_now(&p.to_string_lossy()); // used now: pruned least recently used first
        self.rows = got;
        self.loaded = (0..self.batches()).collect();
        self.from_disk = true;
    }

    fn save_disk(&self) {
        let Some(p) = self.path() else { return };
        let mut rows = BTreeMap::new();
        #[allow(clippy::iter_over_hash_type, reason = "collected into a sorted map")]
        for (n, r) in &self.rows {
            let v = match r {
                None => serde_json::Value::from("error"),
                Some((t, pr)) => {
                    let sorted = |s: &HashSet<String>| {
                        let mut v: Vec<&String> = s.iter().collect();
                        v.sort_unstable();
                        serde_json::Value::from(v.into_iter().cloned().collect::<Vec<_>>())
                    };
                    serde_json::Value::from(vec![sorted(t), sorted(pr)])
                }
            };
            rows.insert(n.clone(), v);
        }
        let doc = serde_json::json!({"version": INDEX_VERSION, "key": self.key, "rows": rows});
        let Ok(text) = serde_json::to_vec(&doc) else { return };
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if bc2_gamefs::atomic::write(&p, &[&text]).is_err() {
            return; // a cache that cannot be written is only slower
        }
        if let (Some(s), Some(d)) = (bc2_gamefs::budget::spec("effectport"), p.parent()) {
            let _ = bc2_gamefs::budget::maybe_prune(s, &d.to_string_lossy(), None);
        }
    }

    /// The rows of the batch starting at document `i` (read now if not yet).
    ///
    /// # Errors
    /// A bundle that does not read.
    pub fn batch(&mut self, i: usize) -> Res<()> {
        if self.loaded.contains(&i) {
            return Ok(());
        }
        let end = (i + EMITTER_BATCH).min(self.cands.len());
        let names = self.cands.get(i..end).unwrap_or_default().to_vec();
        let places = self.places.get(i..end).unwrap_or_default().to_vec();
        let data = read_places(&names, &places)?;
        let have: Vec<(&String, &Vec<u8>)> = names.iter().filter_map(|n| data.get(n).map(|d| (n, d))).collect();
        let rows: Vec<Row> = have
            .par_iter()
            .map(|(_, d)| {
                let types = instance_types(d).ok()?;
                let mut all = HashSet::new();
                let mut present = HashSet::new();
                for t in types {
                    let t = t?; // an instance without a type: the bytes decide
                    if t.is_empty() || memchr::memmem::find(d, t.as_bytes()).is_some() {
                        present.insert(t.clone());
                    }
                    all.insert(t);
                }
                Some((all, present))
            })
            .collect();
        for ((n, _), r) in have.iter().zip(rows) {
            self.rows.insert((*n).clone(), r);
        }
        self.loaded.insert(i);
        self.batches_read += 1;
        if self.loaded.len() == self.batches() {
            self.save_disk();
        }
        Ok(())
    }
}

/// The bytes of entries by their places (bundle, entry name), each bundle read once on all cores; an entry the
/// bundle does not hold is left out, a bundle that does not read fails.
fn read_places(names: &[String], places: &[(String, String)]) -> Res<HashMap<String, Vec<u8>>> {
    let mut by: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for (n, (b, e)) in names.iter().zip(places) {
        if b.is_empty() {
            continue;
        }
        let k = *at.entry(b.clone()).or_insert_with(|| {
            by.push((b.clone(), Vec::new()));
            by.len() - 1
        });
        if let Some((_, v)) = by.get_mut(k) {
            v.push((n.clone(), e.clone()));
        }
    }
    let items: Vec<(String, Vec<String>)> = by
        .iter()
        .map(|(b, v)| (b.clone(), v.iter().map(|(_, e)| e.clone()).collect()))
        .collect();
    let got = bc2_gamefs::bundles::read_many(&items);
    let mut out = HashMap::new();
    for ((b, pairs), r) in by.into_iter().zip(got) {
        let datas = r.map_err(|e| Error::Bundle(b.clone(), e))?;
        for ((n, _), d) in pairs.into_iter().zip(datas) {
            out.insert(n, d);
        }
    }
    Ok(out)
}

/// [`Templates`] from [`TEMPLATES`] plus the BFBC2 emitter documents covering `types` (greedy, by name order): the
/// first document in name order holding a type still needed (and naming one in its bytes) joins the pool, until
/// every type is covered. Decided from [`EmitterIndex`]'s rows; a document whose row cannot decide (it does not
/// read, or it holds a needed type its bytes do not name) is tested on its own bytes.
///
/// # Errors
/// A template the game copy lacks or that does not read, a bundle that does not read.
pub fn templates(ix: &dyn Stock, types: &HashSet<Option<String>>, rules: &Rules) -> Res<Chosen> {
    let mut pool: Vec<String> = TEMPLATES.iter().map(|s| (*s).to_owned()).collect();
    let mut datas: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    let mut have: HashSet<Option<String>> = HashSet::new();
    for p in &pool {
        let d = ix.read(&format!("{p}.dbx"))?;
        if let Some(d) = &d {
            have.extend(instance_types(d)?);
        } else {
            return Err(Error::MissingTemplate(format!(
                "template partition {p}: not in the game copy"
            )));
        }
        datas.insert(format!("{p}.dbx"), d);
    }
    let mut need: HashSet<Option<String>> = types.difference(&have).cloned().collect();
    if need.contains(&None) {
        // an instance without a type (a damaged partition): no document can name it
        return Err(Error::Value(
            "an instance without a type: no template can be found for it".to_owned(),
        ));
    }
    let mut chosen: Vec<String> = Vec::new();
    if !need.is_empty() {
        let idx = EmitterIndex::of_index(ix);
        let mut idx = idx.lock().unwrap_or_else(PoisonError::into_inner);
        let n_cands = idx.cands.len();
        let mut i = 0;
        'batches: while i < n_cands {
            if need.is_empty() {
                break;
            }
            idx.batch(i)?;
            let end = (i + EMITTER_BATCH).min(n_cands);
            for k in i..end {
                let Some(n) = idx.cands.get(k).cloned() else { continue };
                let Some(row) = idx.rows.get(&n) else { continue };
                let wanted = |t: &String| need.contains(&Some(t.clone()));
                let got: HashSet<String> = match row {
                    Some((all, present)) if !all.iter().any(wanted) || present.iter().any(wanted) => {
                        all.iter().filter(|t| wanted(t)).cloned().collect()
                    }
                    _ => {
                        let Some(b) = ix.read(&n)? else { continue };
                        let named = need
                            .iter()
                            .flatten()
                            .any(|t| memchr::memmem::find(&b, t.as_bytes()).is_some());
                        if !named {
                            continue;
                        }
                        instance_types(&b)?
                            .into_iter()
                            .flatten()
                            .filter(|t| wanted(t))
                            .collect()
                    }
                };
                if !got.is_empty() {
                    pool.push(n.strip_suffix(".dbx").unwrap_or(&n).to_owned());
                    chosen.push(n.clone());
                    #[allow(clippy::iter_over_hash_type, reason = "removed from a set")]
                    for t in got {
                        need.remove(&Some(t));
                    }
                }
                if need.is_empty() {
                    break 'batches;
                }
            }
            i = end;
        }
    }
    for n in &chosen {
        datas.insert(n.clone(), ix.read(n)?);
    }
    let items = pool
        .iter()
        .map(|p| (p.clone(), datas.remove(&format!("{p}.dbx")).flatten()))
        .collect();
    Ok(Chosen {
        templates: build_templates(items, rules)?,
        pool,
        need,
    })
}

// ------------------------------------------------------------------------------------------------ the port

/// Files in the order first written, a name written again keeping its place (a Python dict).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Files {
    /// `(name, bytes)`.
    pub items: Vec<(String, Vec<u8>)>,
    at: HashMap<String, usize>,
}

impl Files {
    /// `files[name] = data`.
    pub fn insert(&mut self, name: String, data: Vec<u8>) {
        if let Some(&i) = self.at.get(&name) {
            if let Some(slot) = self.items.get_mut(i) {
                slot.1 = data;
            }
            return;
        }
        self.at.insert(name.clone(), self.items.len());
        self.items.push((name, data));
    }

    /// `files.update(other)`.
    pub fn update(&mut self, other: Self) {
        for (n, d) in other.items {
            self.insert(n, d);
        }
    }

    /// How many.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// None.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// `check_defaults`: the [`DEFAULTS`] fields a ported partition holds with another value, as `default` records.
///
/// # Errors
/// A partition that does not read.
pub fn check_defaults(data: &[u8], report: &mut Report) -> Res<()> {
    let f = dbx::parse(data).map_err(|e| Error::Value(e.to_string()))?;
    let Some(p) = f.roots.first() else { return Ok(()) };
    for inst in p.children().iter().filter(|e| f.str(e.tag) == "instance") {
        let ty = attr(&f, inst, "type").unwrap_or("");
        let t = ty.rsplit('.').next().unwrap_or(ty);
        for (tt, fld, want) in DEFAULTS {
            if t != *tt {
                continue;
            }
            let Some(c) = child(&f, inst, fld) else { continue };
            let got = dbx::value(&f, c).map_err(|e| Error::Value(e.to_string()))?;
            if got != *want {
                let line = format!("DEFAULT {t}.{fld} = {got}, DICE usually {want}");
                report.records.push(Record {
                    path: format!("{t}.{fld}"),
                    action: "default".to_owned(),
                    old: Some(got),
                    new: Some((*want).to_owned()),
                    text: Some(line.clone()),
                    ..Record::default()
                });
                report.lines.push(line);
            }
        }
    }
    Ok(())
}

/// An element's attribute (the first of that name).
pub(crate) fn attr<'f>(f: &'f dbx::DbxFile, e: &dbx::Element, name: &str) -> Option<&'f str> {
    e.attributes
        .iter()
        .find(|(k, _)| f.str(*k) == name)
        .map(|(_, v)| f.str(*v))
}

/// `Element.child(name)`: the first direct child whose `name` attribute is `name`.
pub(crate) fn child<'e>(f: &dbx::DbxFile, e: &'e dbx::Element, name: &str) -> Option<&'e dbx::Element> {
    e.children().iter().find(|c| attr(f, c, "name") == Some(name))
}

/// The instance guids of a partition, lower case (`dbxport.guids_of`).
///
/// # Errors
/// A partition that does not read.
pub fn guids_of(data: &[u8]) -> Res<HashSet<String>> {
    let tree = dbx::parse_tree(data).map_err(|e| Error::Value(e.to_string()))?;
    let p = tree
        .partition()
        .ok_or_else(|| Error::Value("no partition".to_owned()))?;
    Ok(p.children()
        .filter(|n| n.tag() == "instance")
        .map(|n| lower(n.attr("guid").unwrap_or("")))
        .collect())
}

/// A port's options besides the effect: the plan made before (`port_many`'s), the templates found once, the
/// partitions' types read before.
pub struct Shared {
    /// The templates.
    pub chosen: Chosen,
}

/// `schema`: `'error'` (a shape no converter handles stops the port) or `'report'` (kept as BF1943's, reported).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schema {
    /// `'error'`.
    Error,
    /// `'report'`.
    Report,
}

fn types_of(src: &dyn Source, key: &str, seen: &mut HashMap<String, HashSet<Option<String>>>) -> Res<()> {
    if !seen.contains_key(key) {
        let t = instance_types(&src.data(key)?)?;
        seen.insert(key.to_owned(), t.into_iter().collect());
    }
    Ok(())
}

/// One partition's port, made side by side with the others of the effect.
enum Done {
    Dbx(Vec<u8>, Vec<dbxport::Record>),
    Texture(String, Vec<u8>),
    None,
}

/// `port`: one effect's files and report. The report's records: the effect's summary (`summary`), each
/// partition's dbxport records (their partition set), `sound` (a BF1943 sound for the caller's sound porter),
/// `missing`, `default`, `level`, `types`; its lines: the said ones and the dbxport lines of [`SHOWN`], indented
/// under the partition.
///
/// # Errors
/// As [`templates`], a partition or texture that does not port.
#[expect(clippy::too_many_arguments, reason = "effectport.port's arguments")]
pub fn port(
    src: &dyn Source,
    ix: &dyn Stock,
    effect: &str,
    level: &str,
    ctx: &PortContext<'_>,
    shared: Option<&Chosen>,
    plan: Option<Vec<Item>>,
    schema: Schema,
    seen: &mut HashMap<String, HashSet<Option<String>>>,
    bc2: &HashSet<String>,
) -> Res<(Files, Report)> {
    let plan = match plan {
        Some(p) if !p.is_empty() => p,
        _ => walk(src, ix, effect, level, true)?,
    };
    let mut types: HashSet<Option<String>> = HashSet::new();
    for it in plan.iter().filter(|it| it.kind == Kind::Port) {
        if let Some(k) = &it.key {
            types_of(src, k, seen)?;
            types.extend(seen.get(k).into_iter().flatten().cloned());
        }
    }
    let own;
    let chosen = if let Some(c) = shared {
        c
    } else {
        own = templates(ix, &types, ctx.rules)?;
        &own
    };
    let t = &chosen.templates;
    let mut missing: Vec<Option<String>> = types
        .iter()
        .filter(|x| x.as_ref().is_none_or(|s| !t.has(s)))
        .cloned()
        .collect();
    let mut report = Report::default();
    let count = |k: Kind| plan.iter().filter(|it| it.kind == k).count();
    report.say(
        effect,
        "summary",
        format!(
            "{effect}: {} partitions to port, {} sounds for the sound porter, {} textures, {} kept as BFBC2's, {} \
             missing; templates {}",
            count(Kind::Port),
            count(Kind::Sound),
            count(Kind::Texture),
            count(Kind::Bfbc2),
            count(Kind::Missing),
            py_list(&chosen.pool)
        ),
    );
    if !missing.is_empty() {
        if missing.iter().any(Option::is_none) {
            return Err(Error::Value("an instance without a type".to_owned()));
        }
        missing.sort_unstable();
        let names: Vec<String> = missing.into_iter().flatten().collect();
        report.say(
            effect,
            "types",
            format!(
                "types no BFBC2 template holds (the catalogue's class defaults, or kept as BF1943 has them): {}",
                py_list(&names)
            ),
        );
    }
    let level_low = format!("{}/", lower(level));
    for it in &plan {
        if let (Kind::Port, Some(key)) = (it.kind, &it.key)
            && key.starts_with("levels/")
            && !key.starts_with(&level_low)
        {
            let text = format!(
                "  LEVEL {}: {level} has no copy - {}'s (level_copy)",
                it.name,
                key.split('/').nth(1).unwrap_or("")
            );
            report.lines.push(text.clone());
            report.records.push(Record {
                source: Some(key.strip_suffix(".dbx").unwrap_or(key).to_owned()),
                ..Record::said(&it.name, "level", text)
            });
        }
    }
    let ours: HashSet<String> = plan
        .iter()
        .filter(|it| matches!(it.kind, Kind::Port | Kind::Texture | Kind::Sound))
        .map(|it| it.name.clone())
        .collect();
    let resolve = |q: &str| -> Result<bool, Fail> {
        let q = lower(q);
        Ok(ours.contains(&q) || bc2.contains(&q) || q == DEFAULT_NORMAL)
    };
    // the partitions ported here with their BF1943 guids: a reference into one is checked against what is written,
    // not against BFBC2's same-named partition (which the port replaces, and which may lack the guid)
    let mut inputs: Vec<Option<Vec<u8>>> = Vec::with_capacity(plan.len());
    let mut written: Ours = Ours::new();
    for it in &plan {
        inputs.push(match (it.kind, &it.key) {
            (Kind::Port, Some(k)) => {
                let d = src.data(k)?;
                written.insert(lower(&it.name), guids_of(&d)?);
                Some(d)
            }
            (Kind::Texture, _) => Some(src.data(&format!("{}.res", it.name))?),
            _ => None,
        });
    }
    let stand_ins: HashMap<SiKey, SiVal> = HashMap::new();
    let done: Vec<Res<Done>> = plan
        .par_iter()
        .zip(inputs.par_iter())
        .map(|(it, data)| -> Res<Done> {
            match (it.kind, data) {
                (Kind::Port, Some(d)) => {
                    let o = Options {
                        drop_untemplated: false,
                        drop_types: HashSet::new(),
                        prune: false,
                        required: HashMap::new(),
                        catalogue: ctx.catalogue.as_ref().map(|c| CatView {
                            cat: c.cat,
                            els: c.els,
                            defaults: c.defaults,
                        }),
                        ours: Some(&written),
                        stand_ins: &stand_ins,
                        donors: Vec::new(),
                        strict: false,
                        schema_error: schema == Schema::Error,
                        conventions: true,
                        bones: None,
                        name: lower(&it.name),
                        rules: ctx.rules,
                        resolve: Some(&resolve),
                    };
                    let p = dbxport::port(d, t, &o)?;
                    Ok(Done::Dbx(p.data, p.records))
                }
                (Kind::Texture, Some(d)) => {
                    let name = src
                        .entry(&format!("{}.res", it.name))
                        .map_or_else(|| format!("{}.res", it.name), |e| e.name.clone());
                    let pc = crate::texture::convert(d).map_err(|e| Error::Texture(e.to_string()))?;
                    Ok(Done::Texture(name, pc))
                }
                _ => Ok(Done::None),
            }
        })
        .collect();
    let mut files = Files::default();
    for (it, d) in plan.iter().zip(done) {
        match (it.kind, d?) {
            (Kind::Port, Done::Dbx(data, recs)) => {
                check_defaults(&data, &mut report)?;
                files.insert(format!("{}.dbx", it.name), data);
                for r in &recs {
                    let mut r = Record::of(r);
                    r.partition = Some(it.name.clone());
                    if SHOWN.contains(&r.action.as_str()) {
                        report.lines.push(format!("  {}: {}", it.name, r.line()));
                    }
                    report.records.push(r);
                }
            }
            (Kind::Texture, Done::Texture(name, pc)) => files.insert(name, pc),
            (Kind::Sound, _) => report.say(
                &it.name,
                "sound",
                format!("SOUND {}: BF1943's, for the sound porter", it.name),
            ),
            (Kind::Missing, _) => report.say(
                &it.name,
                "missing",
                format!("  MISSING {}: neither BF1943 nor BFBC2 has it", it.name),
            ),
            _ => {}
        }
    }
    Ok((files, report))
}

/// `port_many`: the files and report of several effects, the templates found once for all of them. `log` hears
/// each effect's line.
///
/// # Errors
/// As [`port`]; whatever `log` returns.
pub fn port_many(
    src: &dyn Source,
    ix: &dyn Stock,
    effects: &[String],
    level: &str,
    ctx: &PortContext<'_>,
    schema: Schema,
    log: &mut dyn FnMut(&str) -> Res<()>,
) -> Res<(Files, Report)> {
    let mut plans: HashMap<&str, Vec<Item>> = HashMap::new();
    for e in effects {
        if !plans.contains_key(e.as_str()) {
            plans.insert(e, walk(src, ix, e, level, true)?);
        }
    }
    let mut types: HashSet<Option<String>> = HashSet::new();
    let mut seen: HashMap<String, HashSet<Option<String>>> = HashMap::new();
    for e in effects {
        for it in plans.get(e.as_str()).into_iter().flatten() {
            if let (Kind::Port, Some(k)) = (it.kind, &it.key) {
                types_of(src, k, &mut seen)?;
                types.extend(seen.get(k).into_iter().flatten().cloned());
            }
        }
    }
    let shared = templates(ix, &types, ctx.rules)?;
    let bc2 = bc2_partitions(ix);
    let mut files = Files::default();
    let mut report = Report::default();
    for (n, e) in effects.iter().enumerate() {
        let plan = plans.get(e.as_str()).cloned();
        let (f, r) = port(src, ix, e, level, ctx, Some(&shared), plan, schema, &mut seen, &bc2)?;
        let k = f.len();
        files.update(f);
        report.records.extend(r.records);
        report.lines.extend(r.lines);
        log(&format!(
            "  effect {}/{} {}: {k} files",
            n + 1,
            effects.len(),
            e.rsplit('/').next().unwrap_or(e)
        ))?;
    }
    Ok((files, report))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    struct Src(HashMap<String, (Entry, Vec<u8>)>);

    impl Source for Src {
        fn entry(&self, key: &str) -> Option<&Entry> {
            self.0.get(key).map(|(e, _)| e)
        }
        fn keys(&self) -> Vec<&str> {
            self.0.keys().map(String::as_str).collect()
        }
        fn data(&self, key: &str) -> Res<Vec<u8>> {
            self.0
                .get(key)
                .map(|(_, d)| d.clone())
                .ok_or_else(|| Error::Value(key.to_owned()))
        }
    }

    fn src(keys: &[&str]) -> Src {
        Src(keys
            .iter()
            .map(|k| {
                let e = Entry {
                    name: (*k).to_owned(),
                    kind: None,
                };
                ((*k).to_owned(), (e, Vec::new()))
            })
            .collect())
    }

    // spec: XWW2-FX-1
    #[test]
    fn level_copies_the_levels_own_else_the_first_other() {
        let s = src(&[
            "levels/w/emitters/a.dbx",
            "levels/g/emitters/b.dbx",
            "levels/i/emitters/b.dbx",
            "levels/g/fx/x/emitter_m/em_c.dbx",
        ]);
        let lv = levels_of(&s);
        assert_eq!(lv, ["levels/g", "levels/i", "levels/w"]);
        assert_eq!(
            level_copy(&s, &lv, "emitters/a", "levels/w", true).as_deref(),
            Some("levels/w/emitters/a.dbx")
        );
        assert_eq!(
            level_copy(&s, &lv, "emitters/b", "levels/w", true).as_deref(),
            Some("levels/g/emitters/b.dbx")
        );
        assert_eq!(level_copy(&s, &lv, "emitters/b", "levels/w", false), None);
        assert_eq!(
            level_copy(&s, &lv, "fx/x/emitter_m/em_c", "levels/w", true).as_deref(),
            Some("levels/g/fx/x/emitter_m/em_c.dbx")
        );
    }

    #[test]
    fn effects_with_emitters_and_plain_sounds() {
        assert!(effect_with_emitters(
            b"..Entity.EffectAsset..Emitter.EmitterEntityData.."
        ));
        assert!(!effect_with_emitters(b"..Entity.EffectAsset..Audio.SoundEntityData.."));
    }

    #[test]
    fn record_lines_as_the_reference() {
        let r = |a: &str, old: Option<&str>, new: Option<&str>, note: Option<&str>, source: Option<&str>| Record {
            path: "P".into(),
            action: a.into(),
            old: old.map(Into::into),
            new: new.map(Into::into),
            note: note.map(Into::into),
            source: source.map(Into::into),
            ..Record::default()
        };
        assert_eq!(
            r("nulled", Some("x/y"), None, None, None).line(),
            "nulled P (refers to x/y): no BFBC2 value"
        );
        assert_eq!(
            r("nulled", Some("x"), None, Some(""), None).line(),
            "nulled P (refers to x): no BFBC2 value"
        );
        assert_eq!(
            r("substituted", Some("a"), Some("b"), Some("n"), Some("s")).line(),
            "substituted P (refers to a): b (n from s)"
        );
        assert_eq!(r("value", Some("1"), Some("2"), None, None).line(), "value P: 1 -> 2");
        assert_eq!(
            r("added", None, Some(""), None, Some("t")).line(),
            "added P (BFBC2's value from t)"
        );
        assert_eq!(r("enum", Some("1"), None, None, None).line(), "enum P: 1 -> None");
        assert_eq!(r("emptied", None, None, None, None).line(), "emptied P: None");
        assert_eq!(r("renamed", None, None, Some("a"), None).line(), "renamed P: a");
        assert_eq!(
            r("untemplated", None, None, None, None).line(),
            "type P: no BFBC2 template - kept as BF1943 has it"
        );
    }

    #[test]
    fn python_list_repr() {
        assert_eq!(py_list(&["a/b".into(), "it's".into()]), r#"['a/b', "it's"]"#);
        assert_eq!(py_list(&[]), "[]");
    }

    #[test]
    fn files_keep_their_first_place() {
        let mut f = Files::default();
        f.insert("a".into(), vec![1]);
        f.insert("b".into(), vec![2]);
        f.insert("a".into(), vec![3]);
        assert_eq!(f.items, vec![("a".to_owned(), vec![3]), ("b".to_owned(), vec![2])]);
    }
}
