//! `bfbc2 bc2loc get|search|movie|dump|langs|set ...`: `bfbc2-menu-tools\bc2loc.py`'s command line over
//! `bc2_formats::loc`. A file the Rust reader leaves to the reference with an exception only Python makes, and a text
//! holding a lone surrogate, hand the command line to the Python.

// The Python it ports indexes and slices freely; each index here is bounded by the check before it (as the
// reference's), and the menu group's 372 recorded lines and OPFOR's replay run them.
#![allow(clippy::indexing_slicing)]

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

use bc2_formats::loc;

use super::common::{
    self as mc, MOVIE_SETS, OMap, PeImage, basename, bundle_layers, bundle_movies, entries, find_exe, find_opfor,
    isfile, join, menu_err, movie_from_members, out_dir, parse_bigf, read_entries, resolve, write_text,
};
use crate::pyfmt::{docstring, str_repr};
use crate::pyproc::{self, Fail, Py, PyErr, PyResult};

const SOURCE: &str = include_str!("../../../../bfbc2-menu-tools/bc2loc.py");
const PC_PREFIX: &str = "WIN32_";

/// `LocFile`.
struct LocFile {
    data: Vec<u8>,
    strings: usize,
    /// hash -> raw text (the later entry of a repeated hash)
    raw: HashMap<u32, Vec<u8>>,
    order: Vec<u32>,
    /// the table's (hash, offset) in file order
    offsets: Vec<(u32, u32)>,
    changed: OMap<u32, Vec<u8>>,
    table: Option<Vec<u16>>,
}

impl LocFile {
    fn new(data: Vec<u8>, histogram: Option<Vec<u8>>, name: &str) -> PyResult<Self> {
        let parsed = match loc::parse(&data) {
            Ok(p) => p,
            Err(loc::Error::Magic) => {
                return Err(menu_err(format!(
                    "{name} is not a localization text file (language_<lang>_normal.bin)"
                )));
            }
            Err(loc::Error::Header) => {
                let w = |k: usize| u32::from_le_bytes([data[4 * k], data[4 * k + 1], data[4 * k + 2], data[4 * k + 3]]);
                return Err(menu_err(format!(
                    "{name}: the header does not match the file (size {}, {} entries)",
                    u64::from(w(1)) + 8,
                    w(2)
                )));
            }
            Err(_) => return Err(Fail::Python),
        };
        let mut raw = HashMap::new();
        let mut order = Vec::new();
        let mut offsets = Vec::new();
        for (i, (h, o, _)) in parsed.entries.iter().enumerate() {
            raw.insert(*h, parsed.raw(&data, i).to_vec());
            order.push(*h);
            offsets.push((*h, *o));
        }
        let table = match histogram.filter(|h| !h.is_empty()) {
            None => None,
            Some(h) => match loc::histogram(&h) {
                Ok(t) => Some(t),
                Err(_) if h.len() >= 4 && h[..4] != 0x39001u32.to_le_bytes() => {
                    return Err(menu_err(format!("{name}: its histogram file is not one")));
                }
                Err(_) => return Err(Fail::Python),
            },
        };
        Ok(Self {
            strings: parsed.strings,
            data,
            raw,
            order,
            offsets,
            changed: OMap::default(),
            table,
        })
    }

    fn text(&self, h: u32) -> PyResult<Option<String>> {
        match self.raw.get(&h) {
            None => Ok(None),
            Some(r) => loc::decode(r, self.table.as_deref()).map(Some).ok_or(Fail::Python),
        }
    }

    /// `lookup(key)`: (hash, the name it was found under, text).
    fn lookup(&self, key: &str) -> PyResult<Option<(u32, String, String)>> {
        if let Some(h) = hex_key(key) {
            return Ok(self.text(h)?.map(|t| (h, key.to_owned(), t)));
        }
        for name in [format!("{PC_PREFIX}{key}"), key.to_owned()] {
            let h = loc::khash_str(&name);
            if let Some(t) = self.text(h)? {
                return Ok(Some((h, name, t)));
            }
        }
        Ok(None)
    }

    #[allow(clippy::single_match_else)] // the reference's two cases
    fn set(&mut self, key: &str, text: &str) -> PyResult<()> {
        let h = match hex_key(key) {
            Some(h) => h,
            None => {
                let pc = loc::khash_str(&format!("{PC_PREFIX}{key}"));
                if self.raw.contains_key(&pc) {
                    pc
                } else {
                    loc::khash_str(key)
                }
            }
        };
        if text.chars().any(|c| c as u32 >= 0x80 || c == '\0') {
            return Err(menu_err(format!(
                "{key}: only ASCII text without NUL can be written ({})",
                str_repr(text)
            )));
        }
        let b = text.as_bytes().to_vec();
        self.raw.insert(h, b.clone());
        self.changed.insert(h, b);
        if !self.order.contains(&h) {
            self.order.push(h);
        }
        Ok(())
    }

    fn to_bytes(&self) -> PyResult<Vec<u8>> {
        let mut name16 = [0u8; 16];
        name16.copy_from_slice(&self.data[20..36]);
        let changed: Vec<(u32, Vec<u8>)> = self.changed.items.clone();
        loc::write(&name16, &self.data[self.strings..], &self.offsets, &changed).map_err(|_| Fail::Python)
    }
}

/// `re.fullmatch(r'0x[0-9A-Fa-f]{1,8}', key)`: the hash.
fn hex_key(key: &str) -> Option<u32> {
    let d = key.strip_prefix("0x")?;
    (!d.is_empty() && d.len() <= 8 && d.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(d, 16).ok())
        .flatten()
}

/// `languages(root)`: {language: bundle}.
fn languages(root: &str) -> PyResult<OMap<String, String>> {
    let mut out = OMap::default();
    for bundle in bundle_layers(root, "overlay-00.fbrb") {
        for e in entries(&bundle)? {
            if let Some(lang) = language_of(&e.name)
                && !e.is_deleted()
            {
                out.insert(lang.to_lowercase(), bundle.clone());
            }
        }
    }
    Ok(out)
}

/// `re.fullmatch(r'Localization/bin/language_(\w+?)_normal\.bin', name, re.I)`'s group.
fn language_of(name: &str) -> Option<String> {
    let (pre, suf) = ("localization/bin/language_", "_normal.bin");
    let low = name.to_lowercase();
    if low.len() != name.len() {
        return None;
    }
    if !low.starts_with(pre) || !low.ends_with(suf) || low.len() < pre.len() + suf.len() + 1 {
        return None;
    }
    let mid = &name[pre.len()..name.len() - suf.len()];
    (!mid.is_empty() && mid.chars().all(|c| c.is_alphanumeric() || c == '_')).then(|| mid.to_owned())
}

/// `load(lang, root, path)`.
fn load(lang: &str, root: Option<&str>, path: Option<&str>) -> PyResult<LocFile> {
    if let Some(path) = path.filter(|p| !p.is_empty()) {
        if !isfile(path) {
            return Err(Fail::Err(PyErr::file_not_found(path)));
        }
        let hist = histogram_name(path);
        let data = pyproc::read_file(path)?;
        let h = if isfile(&hist) && hist != path {
            Some(pyproc::read_file(&hist)?)
        } else {
            None
        };
        return LocFile::new(data, h, path);
    }
    let root = match root.filter(|r| !r.is_empty()) {
        Some(r) => r.to_owned(),
        None => find_opfor(None)?,
    };
    let langs = languages(&root)?;
    let ll = lang.to_lowercase();
    let Some(bundle) = langs.get(&ll).cloned() else {
        let mut have: Vec<&String> = langs.keys().collect();
        have.sort();
        let have: Vec<&str> = have.iter().map(|s| s.as_str()).collect();
        return Err(menu_err(format!(
            "no language \"{lang}\" in the game copy (have: {})",
            if have.is_empty() {
                "none".to_owned()
            } else {
                have.join(", ")
            }
        )));
    };
    let names = [
        format!("Localization/bin/language_{ll}_normal.bin"),
        format!("Localization/bin/language_{ll}_histogram.bin"),
    ];
    let mut by_low: HashMap<String, String> = HashMap::new();
    for e in entries(&bundle)? {
        by_low.insert(e.name.to_lowercase(), e.name.clone());
    }
    let wanted: Vec<String> = names
        .iter()
        .filter_map(|n| by_low.get(&n.to_lowercase()).cloned())
        .collect();
    let got = read_entries(&bundle, &wanted)?;
    let mut by: HashMap<String, Vec<u8>> = HashMap::new();
    for (k, v) in wanted.iter().zip(got) {
        by.insert(k.to_lowercase(), v);
    }
    let data = by.remove(&names[0].to_lowercase()).ok_or(Fail::Python)?;
    let hist = by.remove(&names[1].to_lowercase());
    LocFile::new(data, hist, &names[0])
}

/// `re.sub(r'_(normal|huge|numbers)\.bin$', '_histogram.bin', path, flags=re.I)`.
fn histogram_name(path: &str) -> String {
    let low = path.to_lowercase();
    if low.len() == path.len() {
        for kind in ["_normal.bin", "_huge.bin", "_numbers.bin"] {
            if low.ends_with(kind) {
                return format!("{}_histogram.bin", &path[..path.len() - kind.len()]);
            }
        }
    }
    path.to_owned()
}

/// `movie_constants_all(root)`.
fn movie_constants_all(root: Option<&str>) -> PyResult<BTreeSet<String>> {
    let root = match root.filter(|r| !r.is_empty()) {
        Some(r) => r.to_owned(),
        None => find_opfor(None)?,
    };
    let mut out = BTreeSet::new();
    let pristine = join(&root, &["original", "Package", "menu", "main", "mainmenu-00.fbrb"]);
    let mut sets: Vec<Vec<String>> = MOVIE_SETS.iter().map(|(_, rel)| bundle_layers(&root, rel)).collect();
    if isfile(&pristine) {
        sets.push(vec![pristine]);
    }
    for layers in sets {
        if layers.is_empty() {
            continue;
        }
        let mut by_bundle: OMap<String, Vec<String>> = OMap::default();
        for (_, (bundle, entry)) in &bundle_movies(&layers)?.items {
            by_bundle.entry_or(bundle.clone(), Vec::new).push(entry.clone());
        }
        for (bundle, entries) in &by_bundle.items {
            let mut keys: Vec<String> = Vec::new();
            for e in entries {
                if !keys.contains(e) {
                    keys.push(e.clone());
                }
            }
            for (entry, data) in keys.iter().zip(read_entries(bundle, &keys)?) {
                match parse_bigf(&data, entry).and_then(|m| movie_from_members(&m, entry)) {
                    Ok(m) => out.extend(m.consts.iter().filter(|c| c.starts_with('"')).map(|c| {
                        c.chars()
                            .skip(1)
                            .take(c.chars().count().saturating_sub(2))
                            .collect::<String>()
                    })),
                    Err(Fail::Err(e)) if e.kind == "MenuToolError" => {}
                    Err(e) => return Err(e),
                }
            }
        }
    }
    Ok(out)
}

/// `key_names(loc, root)`: {hash: key name}.
fn key_names(loc: &LocFile, root: Option<&str>) -> PyResult<HashMap<u32, String>> {
    let mut cands: BTreeSet<String> = BTreeSet::new();
    match find_exe(root, None).and_then(|p| PeImage::open(&p)) {
        Ok(img) => cands.extend(
            img.strings(2)
                .into_iter()
                .map(|(s, _)| s)
                .filter(|s| s.chars().count() < 120),
        ),
        Err(Fail::Err(e)) if e.kind == "MenuToolError" || e.kind == "FileNotFoundError" => {}
        Err(e) => return Err(e),
    }
    match movie_constants_all(root) {
        Ok(c) => cands.extend(c),
        Err(Fail::Err(e)) if e.kind == "MenuToolError" => {}
        Err(e) => return Err(e),
    }
    let mut out = HashMap::new();
    for s in &cands {
        for name in [s.clone(), format!("{PC_PREFIX}{s}")] {
            let h = loc::khash_str(&name);
            if loc.raw.contains_key(&h) {
                out.entry(h).or_insert(name);
            }
        }
    }
    Ok(out)
}

fn hex10(h: u32) -> String {
    format!("{h:#010x}")
}

#[expect(clippy::too_many_lines, reason = "bc2loc.main, command by command")]
#[allow(clippy::single_match_else)] // a key found or not, as bc2loc.main prints them
pub fn run(py: &mut Py, cmdline: &[String]) -> PyResult<i32> {
    let mut args = cmdline.to_vec();
    if args
        .first()
        .is_none_or(|a| !matches!(a.as_str(), "get" | "search" | "movie" | "dump" | "langs" | "set"))
    {
        py.print(&docstring(SOURCE))?;
        return Ok(i32::from(!args.is_empty()));
    }
    py.stdout_replace();
    let cmd = args.remove(0);
    let root = mc::take(&mut args, "--opfor")?;
    let root = root.as_deref();
    let lang = mc::take(&mut args, "--lang")?
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "english".to_owned());
    let path = mc::take(&mut args, "--file")?;
    let path = path.as_deref();
    let out = mc::take(&mut args, "--out")?;
    let no_keys = args.iter().any(|a| a == "--no-keys");
    args.retain(|a| a != "--no-keys");
    let opfor = || -> PyResult<String> {
        match root.filter(|r| !r.is_empty()) {
            Some(r) => Ok(r.to_owned()),
            None => find_opfor(None),
        }
    };
    if cmd == "langs" {
        let langs = languages(&opfor()?)?;
        let mut v: Vec<&(String, String)> = langs.items.iter().collect();
        v.sort();
        for (k, b) in v {
            py.print(&format!("{k:10} {b}"))?;
        }
        return Ok(0);
    }
    if cmd == "set" {
        let pairs: Vec<(String, String)> = args
            .iter()
            .filter_map(|a| a.split_once('=').map(|(k, t)| (k.to_owned(), t.to_owned())))
            .collect();
        let Some(out) = out.filter(|o| !o.is_empty() && !pairs.is_empty()) else {
            return Err(menu_err("set needs <key>=<text>... and --out <folder>"));
        };
        if path.is_some_and(|p| !p.is_empty()) && lang == "all" {
            return Err(menu_err("--file is one language's file: give it without --lang all"));
        }
        let langs: Vec<String> = if lang == "all" {
            let mut v: Vec<String> = languages(&opfor()?)?.keys().cloned().collect();
            v.sort();
            v
        } else {
            vec![lang.clone()]
        };
        std::fs::create_dir_all(&out).map_err(|_| Fail::Python)?;
        for lg in langs {
            let mut loc = load(&lg, root, path)?;
            for (k, t) in &pairs {
                loc.set(k, t)?;
            }
            let file = match path.filter(|p| !p.is_empty()) {
                Some(p) => basename(p),
                None => format!("language_{}_normal.bin", lg.to_lowercase()),
            };
            let dst = join(&out, &[&file]);
            pyproc::write_file(&dst, &loc.to_bytes()?)?;
            py.print(&format!("{dst}: {} text(s) set", pairs.len()))?;
        }
        return Ok(0);
    }
    let loc = load(&lang, root, path)?;
    match cmd.as_str() {
        "get" => {
            if args.is_empty() {
                return Err(menu_err("get needs a key"));
            }
            let mut missing = 0;
            for key in &args {
                match loc.lookup(key)? {
                    Some((h, name, text)) => {
                        py.print(&format!("{key} = {}   (as {name}, hash {})", str_repr(&text), hex10(h)))?;
                    }
                    None => {
                        missing += 1;
                        py.print(&format!(
                            "{key}: no text (hashes {} / {} not in the file)",
                            hex10(loc::khash_str(&format!("{PC_PREFIX}{key}"))),
                            hex10(loc::khash_str(key))
                        ))?;
                    }
                }
            }
            Ok(i32::from(missing == args.len()))
        }
        "search" => {
            if args.is_empty() {
                return Err(menu_err("search needs a text"));
            }
            let want = args.join(" ").to_lowercase();
            let names = if no_keys {
                HashMap::new()
            } else {
                key_names(&loc, root)?
            };
            let mut hits = 0;
            for &h in &loc.order {
                let t = loc.text(h)?.unwrap_or_default();
                if t.to_lowercase().contains(&want) {
                    hits += 1;
                    let name = names.get(&h).map_or("?", String::as_str);
                    py.print(&format!("{}  {name:40} {}", hex10(h), str_repr(&t)))?;
                }
            }
            py.print(&format!("{hits} text(s) contain \"{want}\""))?;
            Ok(0)
        }
        "movie" => {
            if args.is_empty() {
                return Err(menu_err("movie needs a movie"));
            }
            for spec in &args {
                let (label, members) = resolve(spec, root)?;
                let m = movie_from_members(&members, &label)?;
                let mut seen: BTreeSet<&String> = BTreeSet::new();
                let mut rows: Vec<(String, String, String)> = Vec::new();
                for c in &m.consts {
                    if c.starts_with('"') && seen.insert(c) {
                        let n = c.chars().count();
                        let key: String = c.chars().skip(1).take(n.saturating_sub(2)).collect();
                        if !key.is_empty()
                            && let Some((_, name, text)) = loc.lookup(&key)?
                        {
                            rows.push((key, name, text));
                        }
                    }
                }
                py.print(&format!(
                    "{label}: {} localization key(s) among {} string constants",
                    rows.len(),
                    seen.len()
                ))?;
                rows.sort();
                for (key, name, text) in rows {
                    py.print(&format!(
                        "  {key:40} {}{}",
                        str_repr(&text),
                        if name.starts_with(PC_PREFIX) {
                            "   (PC text)"
                        } else {
                            ""
                        }
                    ))?;
                }
            }
            Ok(0)
        }
        "dump" => {
            let names = if no_keys {
                HashMap::new()
            } else {
                key_names(&loc, root)?
            };
            let out = match out.filter(|o| !o.is_empty()) {
                Some(o) => o,
                None => join(&out_dir(&opfor()?)?, &[&format!("loc_{}.txt", lang.to_lowercase())]),
            };
            let mut text = String::new();
            for &h in &loc.order {
                let t = loc.text(h)?.unwrap_or_default();
                let _ = writeln!(
                    text,
                    "{}\t{}\t{}",
                    hex10(h),
                    names.get(&h).map_or("", String::as_str),
                    str_repr(&t)
                );
            }
            write_text(&out, &text)?;
            py.print(&format!(
                "{} entries ({} with their key) -> {out}",
                loc.order.len(),
                names.len()
            ))?;
            Ok(0)
        }
        _ => Ok(0),
    }
}
