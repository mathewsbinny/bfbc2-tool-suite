//! The game's localized texts as `bfbc2-menu-toolsc2loc.py`'s `LocFile` holds them, over `bc2_formats::loc`
//! (round OR14: out of `bfbc2.exe`'s `bc2loc` command line, so Project OPFOR's WW2 tools - `names.py`,
//! `ww2_awards.py`, `wake_install.py`, `menu_install.py` - read and write the texts through `bc2_native.LocFile`
//! instead of `bc2loc.py`'s Python API): a file's entries, `lookup` (the PC text `WIN32_<key>` first), `set` (ASCII
//! only), `to_bytes` (DICE's layout: the texts kept where they were, new ones appended, the table sorted by hash),
//! the languages of a game copy and `load`. Each refusal is the Python's (`MenuToolError` words); where the Python
//! would fail with an exception of its own, [`MenuError::Reference`].

#![allow(
    clippy::indexing_slicing,
    reason = "each index is bounded by the check before it, as the reference's"
)]

use std::collections::HashMap;

use bc2_formats::loc;
use bc2_pyrt::fmt::str_repr;

use bc2_pyrt::py::{OMap, isfile};

use crate::common::{bundle_layers, entries, find_opfor, read_entries};
use crate::error::{MenuError, MenuResult, read_file};

/// The PC texts' key prefix.
pub const PC_PREFIX: &str = "WIN32_";

fn menu_err(msg: impl Into<String>) -> MenuError {
    MenuError::Menu(msg.into())
}

/// `LocFile`.
pub struct LocFile {
    pub data: Vec<u8>,
    pub strings: usize,
    /// hash -> raw text (the later entry of a repeated hash)
    pub raw: HashMap<u32, Vec<u8>>,
    pub order: Vec<u32>,
    /// the table's (hash, offset) in file order
    pub offsets: Vec<(u32, u32)>,
    pub changed: OMap<u32, Vec<u8>>,
    pub table: Option<Vec<u16>>,
}

impl LocFile {
    pub fn new(data: Vec<u8>, histogram: Option<Vec<u8>>, name: &str) -> MenuResult<Self> {
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
            Err(_) => return Err(MenuError::Reference),
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
                Err(_) => return Err(MenuError::Reference),
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

    pub fn text(&self, h: u32) -> MenuResult<Option<String>> {
        match self.raw.get(&h) {
            None => Ok(None),
            Some(r) => loc::decode(r, self.table.as_deref())
                .map(Some)
                .ok_or(MenuError::Reference),
        }
    }

    /// `lookup(key)`: (hash, the name it was found under, text).
    pub fn lookup(&self, key: &str) -> MenuResult<Option<(u32, String, String)>> {
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
    pub fn set(&mut self, key: &str, text: &str) -> MenuResult<()> {
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

    pub fn to_bytes(&self) -> MenuResult<Vec<u8>> {
        let mut name16 = [0u8; 16];
        name16.copy_from_slice(&self.data[20..36]);
        let changed: Vec<(u32, Vec<u8>)> = self.changed.items.clone();
        loc::write(&name16, &self.data[self.strings..], &self.offsets, &changed).map_err(|_| MenuError::Reference)
    }
}

/// `re.fullmatch(r'0x[0-9A-Fa-f]{1,8}', key)`: the hash.
pub fn hex_key(key: &str) -> Option<u32> {
    let d = key.strip_prefix("0x")?;
    (!d.is_empty() && d.len() <= 8 && d.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(d, 16).ok())
        .flatten()
}

/// `languages(root)`: {language: bundle}.
pub fn languages(root: &str) -> MenuResult<OMap<String, String>> {
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
pub fn language_of(name: &str) -> Option<String> {
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
pub fn load(lang: &str, root: Option<&str>, path: Option<&str>) -> MenuResult<LocFile> {
    if let Some(path) = path.filter(|p| !p.is_empty()) {
        if !isfile(path) {
            return Err(MenuError::not_found(path));
        }
        let hist = histogram_name(path);
        let data = read_file(path)?;
        let h = if isfile(&hist) && hist != path {
            Some(read_file(&hist)?)
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
    let data = by.remove(&names[0].to_lowercase()).ok_or(MenuError::Reference)?;
    let hist = by.remove(&names[1].to_lowercase());
    LocFile::new(data, hist, &names[0])
}

/// `re.sub(r'_(normal|huge|numbers)\.bin$', '_histogram.bin', path, flags=re.I)`.
pub fn histogram_name(path: &str) -> String {
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
