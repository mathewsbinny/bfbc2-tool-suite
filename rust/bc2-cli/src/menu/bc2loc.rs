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
    self as mc, MOVIE_SETS, OMap, basename, bundle_layers, bundle_movies, find_exe, find_opfor, isfile, join,
    movie_from_members, out_dir, parse_bigf, read_entries, resolve, write_text,
};
use super::menu_err;
use crate::pyfmt::{docstring, str_repr};
use crate::pyproc::{self, Fail, Py, PyResult};
use bc2_menu::error::MenuError;
use bc2_menu::loc::{LocFile, PC_PREFIX, languages, load};

const SOURCE: &str = include_str!("../../../../bfbc2-menu-tools/bc2loc.py");

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
                    Err(MenuError::Menu(_)) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }
    Ok(out)
}

/// `key_names(loc, root)`: {hash: key name}.
fn key_names(loc: &LocFile, root: Option<&str>) -> PyResult<HashMap<u32, String>> {
    let mut cands: BTreeSet<String> = BTreeSet::new();
    match find_exe(root, None).and_then(|p| super::open_exe(&p)) {
        Ok(img) => cands.extend(
            img.strings(2)
                .into_iter()
                .map(|(s, _)| s)
                .filter(|s| s.chars().count() < 120),
        ),
        Err(e) if matches!(e, MenuError::Menu(_)) || e.is_file_not_found() => {}
        Err(e) => return Err(e.into()),
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
            None => Ok(find_opfor(None)?),
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
