//! `bfbc2-menu-tools\menucommon.py`'s pieces the menu command lines use: finding the game copy, movies from files
//! or the game's bundles, BIGF members, OPFOR's patch format, the exe's data; and the bits of Python they lean on
//! (an ordered dict, `str.strip` / `split` / `splitlines`, `ntpath`, `glob`).

// The Python it ports indexes and slices freely; each index here is bounded by the check before it (as the
// reference's), and the menu group's 372 recorded lines and OPFOR's replay run them.
#![allow(clippy::indexing_slicing)]

use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;
use std::path::Path;

use bc2_formats::apt::{self, MovieRef};
use bc2_formats::{bigf, fbrb};
use bc2_harness::pe;

use crate::pyproc::{self, Fail, PyErr, PyResult};

// ------------------------------------------------------------------------------------------------ Python's pieces

/// A Python dict: insertion order kept, a key set again keeps its place.
#[derive(Debug, Clone)]
pub struct OMap<K, V> {
    pub items: Vec<(K, V)>,
    ix: HashMap<K, usize>,
}

impl<K: Eq + Hash + Clone, V> Default for OMap<K, V> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            ix: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone, V> OMap<K, V> {
    pub fn insert(&mut self, k: K, v: V) {
        if let Some(&i) = self.ix.get(&k) {
            if let Some(e) = self.items.get_mut(i) {
                e.1 = v;
            }
        } else {
            self.ix.insert(k.clone(), self.items.len());
            self.items.push((k, v));
        }
    }

    pub fn get(&self, k: &K) -> Option<&V> {
        self.ix.get(k).and_then(|&i| self.items.get(i)).map(|e| &e.1)
    }

    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        match self.ix.get(k) {
            Some(&i) => self.items.get_mut(i).map(|e| &mut e.1),
            None => None,
        }
    }

    pub fn contains(&self, k: &K) -> bool {
        self.ix.contains_key(k)
    }

    /// `setdefault`.
    #[allow(clippy::needless_pass_by_value)] // dict.setdefault(k, v()): the key is stored when new
    pub fn entry_or(&mut self, k: K, v: impl FnOnce() -> V) -> &mut V {
        if !self.ix.contains_key(&k) {
            self.insert(k.clone(), v());
        }
        let i = self.ix.get(&k).copied().unwrap_or(0);
        &mut self.items[i].1
    }

    /// `pop(k, None)`.
    pub fn remove(&mut self, k: &K) {
        if self.ix.remove(k).is_some() {
            self.items.retain(|e| &e.0 != k);
            self.ix = self.items.iter().enumerate().map(|(i, e)| (e.0.clone(), i)).collect();
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.items.iter().map(|e| &e.0)
    }
}

/// BIGF members (or a `.apt` / `.const` pair): `{name: bytes}`.
pub type Members = OMap<String, Vec<u8>>;

/// `str.isspace()`.
pub fn py_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0b}'
            | '\u{0c}'
            | '\r'
            | '\u{1c}'..='\u{1f}'
            | ' '
            | '\u{85}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

/// `s.strip()`.
pub fn strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

/// `s.split()`.
pub fn split(s: &str) -> Vec<&str> {
    s.split(py_space).filter(|t| !t.is_empty()).collect()
}

/// `s.splitlines()` (the line ends dropped).
pub fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let end = match c {
            '\r' => {
                if let Some(&(_, '\n')) = it.peek() {
                    it.next();
                    i + 2
                } else {
                    i + 1
                }
            }
            '\n' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}' => {
                i + c.len_utf8()
            }
            _ => continue,
        };
        out.push(&s[start..i]);
        start = end;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// A text file read as `open(path, encoding='utf-8', errors='replace').read()` does (universal newlines).
pub fn read_text(path: &str) -> PyResult<String> {
    let b = pyproc::read_file(path)?;
    let t = String::from_utf8_lossy(&b);
    Ok(t.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `open(path, 'w', encoding='utf-8').write(text)` (`\n` written as `\r\n`).
pub fn write_text(path: &str, text: &str) -> PyResult<()> {
    pyproc::write_file(path, text.replace('\n', "\r\n").as_bytes())
}

fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// `ntpath.splitdrive(p)[0]`'s length (a drive letter or a UNC share).
fn drive_len(p: &str) -> usize {
    let b = p.as_bytes();
    if b.len() >= 2 && b[1] == b':' {
        return 2;
    }
    if b.len() >= 2 && is_sep(char::from(b[0])) && is_sep(char::from(b[1])) {
        // \\server\share
        let rest = &p[2..];
        if let Some(i) = rest.find(is_sep) {
            let after = &rest[i + 1..];
            let j = after.find(is_sep).unwrap_or(after.len());
            if i > 0 && j > 0 {
                return 2 + i + 1 + j;
            }
        }
    }
    0
}

/// `ntpath.split(p)`: (head, tail).
pub fn nt_split(p: &str) -> (String, String) {
    let d = drive_len(p);
    let (drive, rest) = p.split_at(d);
    let i = rest.rfind(is_sep).map_or(0, |i| i + 1);
    let (head, tail) = rest.split_at(i);
    let trimmed = head.trim_end_matches(is_sep);
    let head = if trimmed.is_empty() { head } else { trimmed };
    (format!("{drive}{head}"), tail.to_owned())
}

/// `os.path.basename`.
pub fn basename(p: &str) -> String {
    nt_split(p).1
}

/// `os.path.dirname`.
pub fn dirname(p: &str) -> String {
    nt_split(p).0
}

/// `os.path.splitext`: (root, ext).
pub fn splitext(p: &str) -> (String, String) {
    let sep = p.rfind(is_sep).map_or(0, |i| i + 1);
    let dot = p.rfind('.').filter(|&d| d >= sep);
    if let Some(d) = dot {
        // leading dots of the name are not an extension
        if p[sep..d].chars().any(|c| c != '.') {
            return (p[..d].to_owned(), p[d..].to_owned());
        }
    }
    (p.to_owned(), String::new())
}

/// `os.path.join(a, *names)` for names that are neither absolute nor carry a drive.
pub fn join(a: &str, names: &[&str]) -> String {
    let mut out = a.to_owned();
    for n in names {
        if out.is_empty() || out.ends_with(is_sep) || (out.len() == 2 && out.ends_with(':')) {
            out.push_str(n);
        } else {
            out.push('\\');
            out.push_str(n);
        }
    }
    out
}

/// `os.path.isfile`.
pub fn isfile(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_file()
}

/// `os.path.isdir`.
pub fn isdir(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_dir()
}

/// `os.path.abspath` (Windows: `GetFullPathNameW`, as `std::path::absolute`).
pub fn abspath(p: &str) -> String {
    std::path::absolute(p).map_or_else(|_| p.to_owned(), |a| a.to_string_lossy().into_owned())
}

/// `fnmatch` of a name against a pattern (`*`, `?`, `[seq]`, `[!seq]`), case folded as Windows' `normcase` does.
fn fnmatch(name: &[char], pat: &[char]) -> bool {
    let Some((&p, rest)) = pat.split_first() else {
        return name.is_empty();
    };
    match p {
        '*' => (0..=name.len()).any(|k| fnmatch(&name[k..], rest)),
        '?' => !name.is_empty() && fnmatch(&name[1..], rest),
        '[' => {
            let Some(close) = rest.iter().skip(1).position(|&c| c == ']').map(|i| i + 1) else {
                return name.first() == Some(&'[') && fnmatch(&name[1..], rest);
            };
            let Some(&n) = name.first() else { return false };
            let set = &rest[..close];
            let (neg, set) = match set.split_first() {
                Some(('!', s)) => (true, s),
                _ => (false, set),
            };
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == '-' {
                    hit |= set[i] <= n && n <= set[i + 2];
                    i += 3;
                } else {
                    hit |= set[i] == n;
                    i += 1;
                }
            }
            hit != neg && fnmatch(&name[1..], &rest[close + 1..])
        }
        c => name.first() == Some(&c) && fnmatch(&name[1..], rest),
    }
}

fn fold(s: &str) -> Vec<char> {
    s.replace('/', "\\").to_lowercase().chars().collect()
}

/// `sorted(glob.glob(pattern))` for a pattern whose wildcards are in its last part (else the Python answers).
pub fn glob(pattern: &str) -> PyResult<Vec<String>> {
    let (head, tail) = nt_split(pattern);
    if head.contains(['*', '?', '[']) {
        return Err(Fail::Python);
    }
    let dir = if head.is_empty() { ".".to_owned() } else { head.clone() };
    let pat = fold(&tail);
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') && !tail.starts_with('.') {
                continue;
            }
            if fnmatch(&fold(&name), &pat) {
                out.push(if head.is_empty() { name } else { join(&head, &[&name]) });
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `args.index(flag)`, the value after it taken out (aptcheck's and bc2loc's `take`: the first occurrence).
pub fn take(args: &mut Vec<String>, flag: &str) -> PyResult<Option<String>> {
    let Some(i) = args.iter().position(|a| a == flag) else {
        return Ok(None);
    };
    if i + 1 >= args.len() {
        return Err(menu_err(format!("{flag} needs a value")));
    }
    let v = args.remove(i + 1);
    args.remove(i);
    Ok(Some(v))
}

/// `take_flag`.
pub fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(i) = args.iter().position(|a| a == flag) {
        args.remove(i);
        true
    } else {
        false
    }
}

/// `MenuToolError(msg)`.
pub fn menu_err(msg: impl Into<String>) -> Fail {
    Fail::Err(PyErr::new("MenuToolError", msg))
}

/// `bytes.hex(' ').upper()`.
pub fn hex_spaced(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

// ------------------------------------------------------------------------------------------------ the game copy

const MENU_BUNDLE: &str = "menu/main/mainmenu-00.fbrb";
/// `MOVIE_SETS`.
pub const MOVIE_SETS: [(&str, &str); 7] = [
    ("menu", "menu/main/mainmenu-00.fbrb"),
    ("mp_hud", "levels/mp_common/ingame-00.fbrb"),
    ("mp_loader", "levels/mp_common/loader-00.fbrb"),
    ("sp_hud", "levels/sp_common/ingame-00.fbrb"),
    ("sp_loader", "levels/sp_common/loader-00.fbrb"),
    ("mp2_hud", "levels/mp2_common/ingame-00.fbrb"),
    ("nam_hud", "levels/nam_common/ingame-00.fbrb"),
];

/// `find_opfor(given)`: given, `OPFOR_ROOT`, then `project-opfor-bfbc2` beside the suite or a folder above it; the
/// first with a `game` folder, absolute.
pub fn find_opfor(given: Option<&str>) -> PyResult<String> {
    let mut cands: Vec<String> = Vec::new();
    cands.extend(given.filter(|g| !g.is_empty()).map(str::to_owned));
    if let Ok(v) = std::env::var("OPFOR_ROOT")
        && !v.is_empty()
    {
        cands.push(v);
    }
    let suite = abspath(&pyproc::suite_root().to_string_lossy());
    let mut d = suite.clone();
    cands.push(join(&d, &["project-opfor-bfbc2"]));
    loop {
        let up = dirname(&d);
        if up == d {
            break;
        }
        d = up;
        cands.push(join(&d, &["project-opfor-bfbc2"]));
    }
    for c in cands {
        if isdir(&join(&c, &["game"])) {
            return Ok(abspath(&c));
        }
    }
    Err(menu_err(
        "Project OPFOR BFBC2 not found - pass --opfor <folder> or set OPFOR_ROOT",
    ))
}

/// `out_dir(root)`: `<OPFOR>\work\tools_out`, made.
pub fn out_dir(root: &str) -> PyResult<String> {
    let d = join(root, &["work", "tools_out"]);
    std::fs::create_dir_all(&d).map_err(|_| Fail::Python)?;
    Ok(d)
}

/// `bundle_layers(root, rel)`: Dist\win32\<rel>, then Package\<rel>, those that exist.
pub fn bundle_layers(root: &str, rel: &str) -> Vec<String> {
    let parts: Vec<&str> = rel.split('/').collect();
    let mut dist = vec!["game", "Dist", "win32"];
    dist.extend(&parts);
    let mut pkg = vec!["game", "Package"];
    pkg.extend(&parts);
    [join(root, &dist), join(root, &pkg)]
        .into_iter()
        .filter(|p| isfile(p))
        .collect()
}

/// `fbrb.entries(path)` (an error: the Python's own).
pub fn entries(path: &str) -> PyResult<Vec<fbrb::Entry>> {
    fbrb::entries_path(path).map(|h| h.entries).map_err(|_| Fail::Python)
}

/// `fbrb.read(path, names)`: the data of each name, in order.
pub fn read_entries(path: &str, names: &[String]) -> PyResult<Vec<Vec<u8>>> {
    let low: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    fbrb::read_path(path, &low).map_err(|_| Fail::Python)
}

fn char_slice(s: &str, from: usize, to_end_minus: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if from + to_end_minus > c.len() {
        return String::new();
    }
    c[from..c.len() - to_end_minus].iter().collect()
}

#[allow(clippy::case_sensitive_file_extension_comparisons)] // lower-cased first, as the reference
/// `bundle_movies(paths)`: `{movie name (lower case): (bundle, entry name)}` of the `Menu/<name>.res` movies.
pub fn bundle_movies(paths: &[String]) -> PyResult<OMap<String, (String, String)>> {
    let mut out = OMap::default();
    for p in paths {
        for e in entries(p)? {
            let low = e.name.to_lowercase();
            let rest: String = low.chars().skip(5).collect();
            if low.starts_with("menu/") && low.ends_with(".res") && !rest.contains('/') {
                let name = char_slice(&e.name, 5, 4).to_lowercase();
                if e.is_deleted() {
                    out.remove(&name);
                } else if e.type_name == "SwfMovie" {
                    out.insert(name, (p.clone(), e.name.clone()));
                }
            }
        }
    }
    Ok(out)
}

/// `parse_bigf(b, where)`.
pub fn parse_bigf(b: &[u8], place: &str) -> PyResult<Members> {
    if b.get(..4) != Some(b"BIGF".as_slice()) {
        return Err(menu_err(format!("{place} is not a menu movie (.res, a BIGF archive)")));
    }
    let got = bigf::parse(b).ok_or(Fail::Python)?;
    let mut out = Members::default();
    for (name, r) in got {
        out.insert(name, b.get(r).unwrap_or_default().to_vec());
    }
    Ok(out)
}

#[allow(clippy::case_sensitive_file_extension_comparisons)] // lower-cased first, as the reference
/// `resolve(spec, root)`: (label, members).
pub fn resolve(spec: &str, root: Option<&str>) -> PyResult<(String, Members)> {
    if isfile(spec) {
        if spec.to_lowercase().ends_with(".apt") {
            let base = splitext(spec).0;
            let name = basename(&base);
            let cpath = format!("{base}.const");
            if !isfile(&cpath) {
                return Err(menu_err(format!(
                    "{spec}: its constant file {name}.const is missing beside it"
                )));
            }
            let mut m = Members::default();
            m.insert(format!("{name}.apt"), pyproc::read_file(spec)?);
            m.insert(format!("{name}.const"), pyproc::read_file(&cpath)?);
            return Ok((spec.to_owned(), m));
        }
        return Ok((spec.to_owned(), parse_bigf(&pyproc::read_file(spec)?, spec)?));
    }
    let low = spec.to_lowercase();
    let (name, movies) = if low.contains(".fbrb:") {
        let i = spec.rfind(':').unwrap_or(0);
        let (bundle, name) = (&spec[..i], &spec[i + 1..]);
        if !isfile(bundle) {
            return Err(Fail::Err(PyErr::file_not_found(bundle)));
        }
        (name.to_owned(), bundle_movies(&[bundle.to_owned()])?)
    } else if low.starts_with("pristine:") {
        let root = match root.filter(|r| !r.is_empty()) {
            Some(r) => r.to_owned(),
            None => find_opfor(None)?,
        };
        let mut parts = vec!["original", "Package"];
        parts.extend(MENU_BUNDLE.split('/'));
        let bundle = join(&root, &parts);
        if !isfile(&bundle) {
            return Err(menu_err(format!(
                "no pristine menu bundle at {bundle} (OPFOR's build_menus.bat keeps it there)"
            )));
        }
        let name = spec.split_once(':').map_or("", |x| x.1).to_owned();
        (name, bundle_movies(&[bundle])?)
    } else if spec.contains('\\') || spec.contains('/') || low.ends_with(".res") || low.ends_with(".apt") {
        return Err(Fail::Err(PyErr::file_not_found(spec)));
    } else {
        let root = match root.filter(|r| !r.is_empty()) {
            Some(r) => r.to_owned(),
            None => find_opfor(None)?,
        };
        (spec.to_owned(), bundle_movies(&bundle_layers(&root, MENU_BUNDLE))?)
    };
    let nl = name.to_lowercase();
    let key = if nl.ends_with(".res") {
        char_slice(&nl, 0, 4)
    } else {
        nl
    };
    let Some((bundle, entry)) = movies.get(&key).cloned() else {
        let mut known: Vec<&String> = movies.keys().collect();
        known.sort();
        let known: Vec<&str> = known.iter().take(12).map(|s| s.as_str()).collect();
        return Err(menu_err(format!(
            "no menu movie \"{name}\" in the bundle (known: {} ...)",
            known.join(", ")
        )));
    };
    let data = read_entries(&bundle, std::slice::from_ref(&entry))?;
    let b = data.into_iter().next().unwrap_or_default();
    Ok((format!("{bundle}:{entry}"), parse_bigf(&b, &entry)?))
}

/// An `apt.Movie` (its constructor's findings: byte order, the movie's offset, the constants).
pub struct Movie {
    pub name: String,
    pub a: Vec<u8>,
    pub c: Vec<u8>,
    pub big: bool,
    #[allow(clippy::struct_field_names)] // apt.Movie.movie
    pub movie: i64,
    pub consts: Vec<String>,
}

impl Movie {
    /// `apt.Movie(data=(name, a, c))`; `Ok(Err(text))` where it raises `ValueError(text)`.
    pub fn new(name: &str, a: &[u8], c: &[u8]) -> PyResult<Result<Self, String>> {
        if !c.starts_with(b"Apt constant file") {
            return Ok(Err("not an APT constant file".to_owned()));
        }
        let w = c.get(20..24).ok_or(Fail::Python)?;
        let le = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
        let be = u32::from_be_bytes([w[0], w[1], w[2], w[3]]);
        let n = a.len() as u64;
        let (le64, be64) = (u64::from(le), u64::from(be));
        let inside = |v: u64| 0 < v && v < n;
        let mut big = !inside(le64) && inside(be64);
        let sig = |v: u64, want: &[u8]| 0 < v && v + 8 <= n && a.get(v as usize + 4..v as usize + 8) == Some(want);
        if sig(be64, b"\x09\x87\x65\x43") && !sig(le64, b"\x43\x65\x87\x09") {
            big = true;
        }
        let movie = i64::from(if big { be } else { le });
        let consts = apt::constants(c, big).map_err(|_| Fail::Python)?;
        Ok(Ok(Self {
            name: name.to_owned(),
            a: a.to_vec(),
            c: c.to_vec(),
            big,
            movie,
            consts,
        }))
    }

    pub fn mref(&self) -> MovieRef<'_> {
        MovieRef {
            name: &self.name,
            apt: &self.a,
            big: self.big,
            movie: self.movie,
            consts: &self.consts,
        }
    }

    /// `cu(o)`: a u32 of the `.const` in the movie's byte order.
    pub fn cu(&self, o: usize) -> PyResult<u64> {
        let w = self.c.get(o..o + 4).ok_or(Fail::Python)?;
        let a = [w[0], w[1], w[2], w[3]];
        Ok(u64::from(if self.big {
            u32::from_be_bytes(a)
        } else {
            u32::from_le_bytes(a)
        }))
    }
}

/// `movie_from_members(members, label)`.
pub fn movie_from_members(members: &Members, label: &str) -> PyResult<Movie> {
    let Some(aptname) = members.keys().find(|k| k.to_lowercase().ends_with(".apt")) else {
        let names: Vec<&str> = members.keys().map(String::as_str).collect();
        return Err(menu_err(format!(
            "{label} has no .apt member (members: {})",
            names.join(", ")
        )));
    };
    let name = char_slice(aptname, 0, 4);
    let want = format!("{name}.const").to_lowercase();
    let Some(constk) = members.keys().find(|k| k.to_lowercase() == want) else {
        return Err(menu_err(format!("{label} has no {name}.const member")));
    };
    let (a, c) = (
        members.get(aptname).map_or(&[][..], Vec::as_slice),
        members.get(constk).map_or(&[][..], Vec::as_slice),
    );
    match Movie::new(&name, a, c)? {
        Ok(m) => Ok(m),
        Err(e) => Err(menu_err(format!("{label} is not a readable APT movie: {e}"))),
    }
}

// ------------------------------------------------------------------------------------------------ patch files

/// A patch line's new byte, or `@label`.
#[derive(Debug, Clone)]
pub enum NewByte {
    Byte(u8),
    Label(String),
}

/// `PatchLine`.
#[derive(Debug, Clone)]
pub struct PatchLine {
    pub number: usize,
    pub append: bool,
    pub member: String,
    pub offset: usize,
    pub expected: Vec<u8>,
    pub new: Vec<NewByte>,
    pub label: String,
    pub text: String,
    pub comment: String,
}

/// `int(t, 16)` for an ASCII token: `Ok(None)` where Python raises `ValueError`; a token Python may read otherwise
/// (Unicode digits) or a value past u64 hands off.
fn int16(t: &str) -> PyResult<Option<i128>> {
    if !t.is_ascii() {
        return Err(Fail::Python);
    }
    let (neg, s) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .map_or(s, |r| r.strip_prefix('_').unwrap_or(r));
    if s.is_empty() || s.starts_with('_') || s.ends_with('_') || s.contains("__") {
        return Ok(None);
    }
    let digits: String = s.chars().filter(|&c| c != '_').collect();
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(None);
    }
    if digits.len() > 24 {
        return Err(Fail::Python);
    }
    let v = i128::from_str_radix(&digits, 16).map_err(|_| Fail::Python)?;
    Ok(Some(if neg { -v } else { v }))
}

type Problems = Vec<(usize, String)>;

/// `parse_patch(path)`.
#[expect(clippy::too_many_lines, reason = "menucommon.parse_patch, case by case")]
pub fn parse_patch(path: &str) -> PyResult<(Vec<PatchLine>, Problems)> {
    let text = read_text(path)?;
    let mut lines = Vec::new();
    let mut problems = Vec::new();
    let mut above = String::new();
    for (k, raw) in splitlines(&text).into_iter().enumerate() {
        let n = k + 1;
        let mut line = strip(raw).to_owned();
        if line.is_empty() {
            above.clear();
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            let c = strip(rest);
            if !c.is_empty() {
                c.clone_into(&mut above);
            }
            continue;
        }
        let comment_from = if line.starts_with("append ") {
            line.rfind('"').unwrap_or(0)
        } else {
            0
        };
        let hash_at = line[comment_from..].find('#').map(|i| i + comment_from);
        let comment = hash_at.map_or_else(|| above.clone(), |h| strip(&line[h + 1..]).to_owned());
        if let Some(h) = hash_at {
            line = strip(&line[..h]).to_owned();
        }
        if line.starts_with("append ") {
            let (q1, q2) = (line.find('"'), line.rfind('"'));
            let head = match q1 {
                Some(q) if q > 0 => split(&line[..q]),
                _ => split(&line),
            };
            let (Some(q1), Some(q2)) = (q1, q2) else {
                problems.push((n, "an append line is: append <member> <label> \"text\"".to_owned()));
                continue;
            };
            if q2 <= q1 || head.len() != 3 {
                problems.push((n, "an append line is: append <member> <label> \"text\"".to_owned()));
                continue;
            }
            let text = line[q1 + 1..q2].to_owned();
            if !text.is_ascii() {
                problems.push((n, "append text must be ASCII (AptTool writes it as ASCII)".to_owned()));
            }
            lines.push(PatchLine {
                number: n,
                append: true,
                member: head[1].to_owned(),
                offset: 0,
                expected: Vec::new(),
                new: Vec::new(),
                label: head[2].to_owned(),
                text,
                comment,
            });
            continue;
        }
        if line.matches("->").count() != 1 {
            problems.push((
                n,
                "a patch line is: <member> <hex offset> <expected bytes> -> <new bytes>".to_owned(),
            ));
            continue;
        }
        let (lhs, rhs) = line.split_once("->").unwrap_or(("", ""));
        let (left, right) = (split(lhs), split(rhs));
        if left.len() < 3 {
            problems.push((n, "missing member, offset or expected bytes".to_owned()));
            continue;
        }
        let bad_hex = || {
            format!(
                "offset and expected bytes must be hex (bytes as two digits): {}",
                left[1..].join(" ")
            )
        };
        let Some(offset) = int16(left[1])? else {
            problems.push((n, bad_hex()));
            continue;
        };
        let mut expected = Vec::new();
        let mut ok = true;
        for t in &left[2..] {
            match int16(t)? {
                Some(v) if (0..256).contains(&v) => expected.push(v as u8),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || left[2..].iter().any(|t| t.chars().count() != 2) {
            problems.push((n, bad_hex()));
            continue;
        }
        if offset < 0 || offset > i128::from(u32::MAX) * 4 {
            return Err(Fail::Python);
        }
        let mut new = Vec::new();
        let mut bad = false;
        for t in &right {
            if let Some(l) = t.strip_prefix('@') {
                new.push(NewByte::Label(l.to_owned()));
            } else if t.len() == 2 && t.chars().all(|c| c.is_ascii_hexdigit()) {
                new.push(NewByte::Byte(u8::from_str_radix(t, 16).unwrap_or(0)));
            } else {
                bad = true;
            }
        }
        if bad || new.is_empty() {
            problems.push((
                n,
                format!(
                    "new bytes must be hex bytes (two digits) or @label: {}",
                    right.join(" ")
                ),
            ));
            continue;
        }
        lines.push(PatchLine {
            number: n,
            append: false,
            member: left[0].to_owned(),
            offset: offset as usize,
            expected,
            new,
            label: String::new(),
            text: String::new(),
            comment,
        });
    }
    Ok((lines, problems))
}

#[allow(clippy::single_match_else)] // a label defined or not, as the reference
/// `apply_patch(members, lines)`: (new members, {member: offsets the lines name}, problems).
pub fn apply_patch(members: &Members, lines: &[PatchLine]) -> (Members, OMap<String, BTreeSet<usize>>, Problems) {
    let mut data = members.clone();
    let mut lower: HashMap<String, String> = HashMap::new();
    for k in members.keys() {
        lower.insert(k.to_lowercase(), k.clone());
    }
    let mut labels: HashMap<String, usize> = HashMap::new();
    let mut named: OMap<String, BTreeSet<usize>> = OMap::default();
    let mut problems = Vec::new();
    for pl in lines {
        let Some(m) = lower.get(&pl.member.to_lowercase()).cloned() else {
            let have: Vec<&str> = data.keys().map(String::as_str).collect();
            problems.push((
                pl.number,
                format!("no member {} in the movie (members: {})", pl.member, have.join(", ")),
            ));
            continue;
        };
        let Some(buf) = data.get_mut(&m) else { continue };
        if pl.append {
            while buf.len() % 4 != 0 {
                buf.push(0);
            }
            if labels.contains_key(&pl.label) {
                problems.push((pl.number, format!("label {} defined twice", pl.label)));
            }
            labels.insert(pl.label.clone(), buf.len());
            buf.extend(pl.text.chars().map(|c| if c.is_ascii() { c as u8 } else { b'?' }));
            buf.push(0);
            continue;
        }
        let mut new = Vec::new();
        for t in &pl.new {
            match t {
                NewByte::Byte(b) => new.push(*b),
                NewByte::Label(l) => match labels.get(l) {
                    Some(&v) => new.extend((v as u32).to_le_bytes()),
                    None => {
                        problems.push((
                            pl.number,
                            format!("undefined label @{l} (an append line must define it first)"),
                        ));
                        new.extend([0u8; 4]);
                    }
                },
            }
        }
        if new.len() != pl.expected.len() {
            problems.push((
                pl.number,
                format!(
                    "size change: {} expected byte(s), {} new - patches must keep the size (AptTool refuses it)",
                    pl.expected.len(),
                    new.len()
                ),
            ));
            continue;
        }
        if pl.offset + pl.expected.len() > buf.len() {
            problems.push((
                pl.number,
                format!(
                    "offset {:X} + {} is past the end of {m} ({:X})",
                    pl.offset,
                    pl.expected.len(),
                    buf.len()
                ),
            ));
            continue;
        }
        let found = &buf[pl.offset..pl.offset + pl.expected.len()];
        if found != pl.expected.as_slice() {
            problems.push((
                pl.number,
                format!(
                    "{m} {:X}: expected {}, found {} - wrong source movie, or the line overlaps an earlier one",
                    pl.offset,
                    hex_spaced(&pl.expected),
                    hex_spaced(found)
                ),
            ));
            continue;
        }
        buf[pl.offset..pl.offset + new.len()].copy_from_slice(&new);
        named
            .entry_or(m.clone(), BTreeSet::new)
            .extend(pl.offset..pl.offset + new.len());
    }
    (data, named, problems)
}

// ------------------------------------------------------------------------------------------------ the exe

/// `PEImage`: a 32-bit exe's sections, for its data.
pub struct PeImage {
    pub path: String,
    pub b: Vec<u8>,
    pub h: pe::raw::Header,
}

impl PeImage {
    pub fn open(path: &str) -> PyResult<Self> {
        let b = pyproc::read_file(path)?;
        let h = match pe::raw::parse(&b) {
            Ok(h) => h,
            Err(pe::raw::RawError::NotExe) => {
                return Err(menu_err(format!("{path} is not a Windows program (.exe)")));
            }
            Err(pe::raw::RawError::Not32) => return Err(menu_err(format!("{path} is not a 32-bit Windows program"))),
            Err(_) => return Err(Fail::Python),
        };
        Ok(Self {
            path: path.to_owned(),
            b,
            h,
        })
    }

    /// `off(va)`.
    pub fn off(&self, va: u64) -> Option<usize> {
        for s in &self.h.sections {
            let (vs, rs) = (u64::from(s.vsize), u64::from(s.rsize));
            let hit = if vs != 0 {
                s.va <= va && va < s.va + vs.min(rs)
            } else {
                s.va <= va && va < s.va + rs
            };
            if hit {
                return usize::try_from(va - s.va + u64::from(s.raw)).ok();
            }
        }
        None
    }

    /// `va(off)`.
    pub fn va(&self, off: usize) -> Option<u64> {
        let o = off as u64;
        self.h
            .sections
            .iter()
            .find(|s| u64::from(s.raw) <= o && o < u64::from(s.raw) + u64::from(s.rsize))
            .map(|s| s.va + o - u64::from(s.raw))
    }

    /// `cstr(va, limit)`.
    pub fn cstr(&self, va: u64, limit: usize) -> Option<String> {
        if va == 0 {
            return None;
        }
        let o = self.off(va)?;
        let hay = self.b.get(o..(o + limit + 1).min(self.b.len()))?;
        let e = hay.iter().position(|&c| c == 0)?;
        let s = &hay[..e];
        (!s.is_empty() && s.iter().all(|&c| (32..127).contains(&c))).then(|| s.iter().map(|&c| char::from(c)).collect())
    }

    /// `strings(minimum)`: {text: [va]} in insertion order.
    pub fn strings(&self, minimum: usize) -> Vec<(String, Vec<u64>)> {
        pe::raw::strings(&self.b, &self.h, minimum)
    }

    /// `find_cstring(text)` (ASCII text).
    pub fn find_cstring(&self, text: &str) -> Vec<u64> {
        pe::raw::find_cstring(&self.b, &self.h, text.as_bytes())
    }

    /// `find_dword(value)` over the data sections.
    pub fn find_dword(&self, value: u32) -> Vec<usize> {
        let rows: Vec<(u32, u32)> = self.h.data_sections().iter().map(|s| (s.raw, s.rsize)).collect();
        pe::raw::find_dword(&self.b, &rows, value)
    }
}

/// `find_exe(root, given)`.
pub fn find_exe(root: Option<&str>, given: Option<&str>) -> PyResult<String> {
    if let Some(g) = given.filter(|g| !g.is_empty()) {
        if !isfile(g) {
            return Err(Fail::Err(PyErr::file_not_found(g)));
        }
        return Ok(g.to_owned());
    }
    let root = match root.filter(|r| !r.is_empty()) {
        Some(r) => r.to_owned(),
        None => find_opfor(None)?,
    };
    let p = join(&root, &["game", "BFBC2Game.exe"]);
    if !isfile(&p) {
        return Err(menu_err(format!(
            "no BFBC2Game.exe in {} - pass --exe <path>",
            dirname(&p)
        )));
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_strings_and_paths() {
        assert_eq!(splitlines("a\r\nb\rc\n\nd"), vec!["a", "b", "c", "", "d"]);
        assert_eq!(splitlines("a\n"), vec!["a"]);
        assert_eq!(split(" a\t b\u{1c}c "), vec!["a", "b", "c"]);
        assert_eq!(splitext("C:\\x\\t.apt"), ("C:\\x\\t".to_owned(), ".apt".to_owned()));
        assert_eq!(splitext("C:\\x\\.apt"), ("C:\\x\\.apt".to_owned(), String::new()));
        assert_eq!(basename("C:\\a/b.patch"), "b.patch");
        assert_eq!(dirname("C:\\a\\b"), "C:\\a");
        assert_eq!(dirname("C:\\b"), "C:\\");
        assert_eq!(int16("1_0").ok().flatten(), Some(16));
        assert_eq!(int16("0x24").ok().flatten(), Some(0x24));
        assert_eq!(int16("zz").ok().flatten(), None);
        assert!(fnmatch(&fold("A1.PATCH"), &fold("a?.patch")));
        assert!(!fnmatch(&fold("c.txt"), &fold("*.patch")));
    }
}
