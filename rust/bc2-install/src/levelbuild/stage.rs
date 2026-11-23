//! The staging folders and their bundles (`wake_install.write_stage`, `fetch_stock`, `pack_level`, `tree_files`,
//! `patch_bundle`, `build_ww2_common` / `common_signature`): each folder with its `fbrb.manifest` (the order the
//! entries load in), packed by the suite's packer; a shared bundle unpacked, its entries replaced in place or added
//! at the end, packed again.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::UNIX_EPOCH;

use super::closure::{Src, Take};
use super::{OMap, Result, bundle_read, pjoin};
use crate::pytext::{basename, relpath, splitlines};

/// The manifest's first line, as FbrbTool reads it.
pub const MANIFEST_HEAD: &str = "# fbrb.manifest - entry order matters. name|type|flags(hex)|size2|size  zipped=1\n";

fn stamp(p: &str) -> Option<(u64, u128)> {
    let m = fs::metadata(p).ok()?;
    let ns = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    Some((m.len(), ns))
}

/// What a staged file was written from: the bytes' identity (the caller's: the same bytes object) or a source
/// file's (path, size, time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagedKey {
    /// The caller's identity of a bytes value.
    Bytes(u64),
    /// A source file as it was copied.
    File(String, u64, u128),
}

/// `_STAGED`: each staged file's source and its own size and time once written (a build writes the level folder
/// twice; a file whose source and own state did not change is not written again).
#[derive(Debug, Default)]
pub struct Staged {
    rec: HashMap<String, (StagedKey, (u64, u128))>,
}

impl Staged {
    /// Forgets every record (`_STAGED.clear()`: a new build).
    pub fn clear(&mut self) {
        self.rec.clear();
    }

    fn already(&self, dst: &str, key: &StagedKey) -> bool {
        self.rec
            .get(dst)
            .is_some_and(|(k, st)| k == key && stamp(dst).is_some_and(|now| now == *st))
    }

    fn record(&mut self, dst: &str, key: StagedKey) {
        if let Some(st) = stamp(dst) {
            self.rec.insert(dst.to_owned(), (key, st));
        }
    }
}

/// The process's staging records (the bindings' `write_stage` uses them; `clear` at each build).
pub static STAGED: LazyLock<Mutex<Staged>> = LazyLock::new(|| Mutex::new(Staged::default()));

/// `shutil.copyfile(src, dst)`: the bytes only (the new file's time is now).
///
/// # Errors
/// Either file cannot be opened.
pub fn copy_file(src: &str, dst: &str) -> Result<()> {
    let mut i = fs::File::open(src).map_err(|e| format!("{src}: {e}"))?;
    let mut o = fs::File::create(dst).map_err(|e| format!("{dst}: {e}"))?;
    io::copy(&mut i, &mut o).map_err(|e| format!("{dst}: {e}"))?;
    Ok(())
}

/// One file to stage: its lower name, name, type and source; `ident`, for bytes, the caller's identity of them.
#[derive(Debug)]
pub struct StageFile<'a> {
    /// Its lower name (the sort key).
    pub low: &'a str,
    /// Its name in the bundle.
    pub name: &'a str,
    /// Its bundle type (`None` written as Python wrote it: `None`).
    pub kind: Option<&'a str>,
    /// Its bytes, or the file holding them.
    pub src: StageSrc<'a>,
}

/// A staged file's source.
#[derive(Debug)]
pub enum StageSrc<'a> {
    /// Bytes, with the caller's identity of them.
    Bytes(&'a [u8], u64),
    /// A file.
    Path(&'a str),
    /// Nothing to write (a stock entry not fetched: the Python wrote nothing either).
    None,
}

/// `write_stage(stage, files, last)`: the files copied into the folder with its `fbrb.manifest` (sorted by lower
/// name, `last` at the end: entries load in manifest order). Returns the manifest lines.
///
/// # Errors
/// A file cannot be written or read.
pub fn write_stage(
    stage: &str,
    files: &[StageFile<'_>],
    last: Option<&str>,
    staged: &mut Staged,
) -> Result<Vec<String>> {
    let mut order: Vec<&StageFile<'_>> = files.iter().collect();
    order.sort_by(|a, b| (Some(a.low) == last, a.low).cmp(&(Some(b.low) == last, b.low)));
    let mut lines = Vec::with_capacity(order.len());
    let mut made = std::collections::HashSet::new();
    for f in order {
        let dst = pjoin(stage, f.name);
        let folder = super::dirname(&dst).to_owned();
        if made.insert(folder.clone()) {
            fs::create_dir_all(&folder).map_err(|e| format!("{folder}: {e}"))?;
        }
        match f.src {
            StageSrc::Bytes(data, ident) => {
                let key = StagedKey::Bytes(ident);
                if !staged.already(&dst, &key) {
                    fs::write(&dst, data).map_err(|e| format!("{dst}: {e}"))?;
                    staged.record(&dst, key);
                }
            }
            StageSrc::Path(src) if !same_abs(src, &dst) => {
                let (size, ns) = stamp(src).ok_or_else(|| format!("FileNotFoundError: {src}"))?;
                let key = StagedKey::File(src.to_owned(), size, ns);
                if !staged.already(&dst, &key) {
                    copy_file(src, &dst)?;
                    staged.record(&dst, key);
                }
            }
            _ => {}
        }
        let size = fs::metadata(&dst)
            .map_err(|e| format!("FileNotFoundError: {dst}: {e}"))?
            .len();
        lines.push(format!(
            "{}|{}|00010000|{size}|{size}",
            f.name,
            f.kind.unwrap_or("None")
        ));
    }
    let mut text = String::from(MANIFEST_HEAD);
    text.push_str(&lines.join("\n"));
    text.push('\n');
    let man = pjoin(stage, "fbrb.manifest");
    fs::write(&man, text).map_err(|e| format!("{man}: {e}"))?;
    Ok(lines)
}

fn same_abs(a: &str, b: &str) -> bool {
    let n = |p: &str| p.replace('/', "\\").to_lowercase();
    n(a) == n(b)
}

/// `fetch_stock(files)`: the stock sources read into bytes (one read per bundle, the bundles in the order first met).
///
/// # Errors
/// A bundle cannot be read.
pub fn fetch_stock(files: &mut OMap<Take>) -> Result<()> {
    let mut by: OMap<Vec<(String, String)>> = OMap::new();
    for (low, t) in files.iter() {
        if let Src::Stock(b, n) = &t.src {
            if !by.contains(b) {
                by.insert(b, Vec::new());
            }
            if let Some(v) = by.get_mut(b) {
                v.push((low.to_owned(), n.clone()));
            }
        }
    }
    for (b, items) in by.into_vec() {
        let names: Vec<String> = items.iter().map(|(_, n)| n.clone()).collect();
        let mut data = bundle_read(&b, &names)?;
        for (low, n) in items {
            if let (Some(t), Some(d)) = (files.get_mut(&low), data.get(&n)) {
                t.src = Src::Bytes(d.clone());
            }
        }
        data.clear();
    }
    Ok(())
}

/// `cc.pack_many(items)`: each (folder, bundle) packed (FbrbTool's bytes), on all cores; the first failure as
/// `cc.pack_many` stops (`failed: pack <folder> <bundle>` and the error).
///
/// # Errors
/// A folder that cannot be packed.
pub fn pack_many(items: &[(String, String)]) -> Result<()> {
    for ((f, o), r) in items.iter().zip(bc2_gamefs::bundles::pack_many(items)) {
        if let Err(e) = r {
            return Err(format!("failed: pack {f} {o}\n{e}"));
        }
    }
    Ok(())
}

/// The report's line of a packed bundle (`_pack_lines`).
///
/// # Errors
/// The bundle is not there.
pub fn pack_line(kind: &str, n: usize, out: &str) -> Result<String> {
    let size = fs::metadata(out).map_err(|e| format!("{out}: {e}"))?.len();
    Ok(format!("bundle {kind}-00.fbrb: {n} entries, {} bytes", thousands(size)))
}

/// Python's `f'{n:,}'`.
#[must_use]
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `shutil.rmtree(p)` when it is a folder.
///
/// # Errors
/// It cannot be removed.
pub fn rmtree(p: &str) -> Result<()> {
    if Path::new(p).is_dir() {
        fs::remove_dir_all(p).map_err(|e| format!("{p}: {e}"))?;
    }
    Ok(())
}

/// `cc.run(cc.FBRB, 'unpack', bundle, out)`: FbrbTool's unpack in process (`SystemExit` with its output when it
/// fails).
///
/// # Errors
/// The unpack failed.
pub fn unpack(bundle: &str, out: &str) -> Result<()> {
    let args = vec!["unpack".to_owned(), bundle.to_owned(), out.to_owned()];
    let o = bc2_tools::fbrbtool::run(&args, None);
    if o.code != 0 {
        return Err(format!(
            "failed: FbrbTool unpack {bundle} {out}\n{}\n{}",
            o.stdout, o.stderr
        ));
    }
    Ok(())
}

fn read_text(p: &str) -> Result<String> {
    let b = fs::read(p).map_err(|e| format!("{p}: {e}"))?;
    let s = String::from_utf8(b).map_err(|e| format!("UnicodeDecodeError: {p}: {e}"))?;
    Ok(s.replace("\r\n", "\n").replace('\r', "\n"))
}

/// A shared bundle's entries replaced in place or added: `(name, type, path)` sorted by name; returns (added,
/// replaced). `keep_spelling`: a replaced entry keeps the bundle's spelling of its name (ww2_common: the file moved to
/// it), else the file is written under the name as given.
fn edit_folder(sd: &str, files: &[(String, String, String)], keep_spelling: bool) -> Result<(usize, usize)> {
    let man = pjoin(sd, "fbrb.manifest");
    let text = read_text(&man)?;
    let mut lines: Vec<String> = splitlines(&text).into_iter().map(str::to_owned).collect();
    let mut at: HashMap<String, usize> = HashMap::new();
    for (i, l) in lines.iter().enumerate() {
        if l.contains('|') {
            at.insert(l.split('|').next().unwrap_or_default().to_lowercase(), i);
        }
    }
    let (mut added, mut replaced) = (0, 0);
    for (name, t, path) in files {
        let dst = pjoin(sd, name);
        let folder = super::dirname(&dst);
        fs::create_dir_all(folder).map_err(|e| format!("{folder}: {e}"))?;
        copy_file(path, &dst)?;
        let size = fs::metadata(&dst).map_err(|e| format!("{dst}: {e}"))?.len();
        if let Some(&i) = at.get(&name.to_lowercase()) {
            let p: Vec<String> = lines
                .get(i)
                .map(|l| l.split('|').map(str::to_owned).collect())
                .unwrap_or_default();
            let g = |k: usize| p.get(k).cloned().unwrap_or_default();
            if keep_spelling && g(0) != *name {
                let to = pjoin(sd, &g(0));
                fs::rename(&dst, &to).map_err(|e| format!("{dst}: {e}"))?;
            }
            if let Some(l) = lines.get_mut(i) {
                *l = format!("{}|{}|{}|{size}|{size}", g(0), g(1), g(2));
            }
            replaced += 1;
        } else {
            lines.push(format!("{name}|{t}|00010000|{size}|{size}"));
            added += 1;
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    fs::write(&man, out).map_err(|e| format!("{man}: {e}"))?;
    Ok((added, replaced))
}

/// `patch_bundle(bundle, files, report, label)`: the bundle `source` (the game's file, or its copy from before the
/// install) unpacked into `<work>\<label>`, `files` ({name: (type, path)}) added or replaced in place (an entry keeps
/// its place and flags), packed to `<work>\<basename of bundle>`. Returns the built bundle.
///
/// # Errors
/// The unpack, a copy or the pack failed.
pub fn patch_bundle(
    game: &str,
    work: &str,
    bundle: &str,
    source: &str,
    files: &[(String, String, String)],
    label: &str,
    report: &mut Vec<String>,
) -> Result<String> {
    let sd = pjoin(work, label);
    rmtree(&sd)?;
    unpack(source, &sd)?;
    let mut sorted: Vec<(String, String, String)> = files.to_vec();
    sorted.sort();
    let (added, replaced) = edit_folder(&sd, &sorted, false)?;
    let out = pjoin(work, basename(bundle));
    pack_many(&[(sd, out.clone())])?;
    let size = fs::metadata(&out).map_err(|e| format!("{out}: {e}"))?.len();
    report.push(format!(
        "{}: {added} added, {replaced} replaced ({} bytes)",
        relpath(bundle, game),
        thousands(size)
    ));
    Ok(out)
}

/// The kinds of the WW2 common bundles.
pub const COMMON_KINDS: [&str; 3] = ["level", "ingame", "loader"];

type CommonSig = Vec<(String, String, String, String, String, u64, u128)>;

/// `_COMMON_DONE`: the last build of the WW2 common bundles this process made: its inputs' signature, the bundles
/// and the report lines.
static COMMON_DONE: LazyLock<Mutex<Option<(CommonSig, Vec<(String, String)>, Vec<String>)>>> =
    LazyLock::new(|| Mutex::new(None));

/// `build_ww2_common(files, report)`: `<work>\ww2_common\<kind>-00.fbrb`, nam_common's bundles with `files` ({kind:
/// [(name, type, path)]}) replacing their entries in place (nam_common's spelling of a name kept) or added. Built once
/// a process while nothing it is made of changed; the next levels' copies are that build's. Returns {kind: bundle}.
///
/// # Errors
/// The unpack, a copy or the pack failed.
pub fn build_ww2_common(
    game: &str,
    work: &str,
    files: &HashMap<String, Vec<(String, String, String)>>,
    report: &mut Vec<String>,
) -> Result<Vec<(String, String)>> {
    let mut sig: CommonSig = Vec::new();
    for kind in COMMON_KINDS {
        let src = pjoin(game, &format!("Package/levels/nam_common/{kind}-00.fbrb"));
        let (s, ns) = stamp(&src).ok_or_else(|| format!("FileNotFoundError: {src}"))?;
        sig.push((src, String::new(), String::new(), String::new(), String::new(), s, ns));
        let mut list: Vec<&(String, String, String)> = files.get(kind).map(|v| v.iter().collect()).unwrap_or_default();
        list.sort_by_key(|(n, _, _)| n.to_lowercase());
        for (name, t, path) in list {
            let (s, ns) = stamp(path).ok_or_else(|| format!("FileNotFoundError: {path}"))?;
            sig.push((
                kind.into(),
                name.to_lowercase(),
                name.clone(),
                t.clone(),
                path.clone(),
                s,
                ns,
            ));
        }
    }
    {
        let done = COMMON_DONE.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((s, built, lines)) = done.as_ref()
            && *s == sig
            && built.iter().all(|(_, p)| Path::new(p).is_file())
        {
            let folder = pjoin(work, "ww2_common");
            fs::create_dir_all(&folder).map_err(|e| format!("{folder}: {e}"))?;
            let mut out = Vec::new();
            for (kind, path) in built {
                let to = pjoin(&folder, &format!("{kind}-00.fbrb"));
                if !same_abs(path, &to) {
                    copy_file(path, &to)?;
                }
                out.push((kind.clone(), to));
            }
            report.extend(lines.iter().cloned());
            return Ok(out);
        }
    }
    let from = report.len();
    let mut built = Vec::new();
    let mut stages = Vec::new();
    let mut counts = Vec::new();
    for kind in COMMON_KINDS {
        let src = pjoin(game, &format!("Package/levels/nam_common/{kind}-00.fbrb"));
        let sd = pjoin(work, &format!("ww2_common/{kind}"));
        rmtree(&sd)?;
        unpack(&src, &sd)?;
        let mut list: Vec<(String, String, String)> = files.get(kind).cloned().unwrap_or_default();
        list.sort_by_key(|(n, _, _)| n.to_lowercase());
        counts.push(edit_folder(&sd, &list, true)?);
        let out = pjoin(work, &format!("ww2_common/{kind}-00.fbrb"));
        built.push((kind.to_owned(), out.clone()));
        stages.push((sd, out));
    }
    pack_many(&stages)?;
    for (((sd, out), kind), (added, replaced)) in stages.iter().zip(COMMON_KINDS).zip(counts) {
        rmtree(sd)?;
        let n = bc2_gamefs::bundles::entries_path(out)
            .map_err(|e| format!("{out}: {e}"))?
            .entries
            .len();
        let size = fs::metadata(out).map_err(|e| format!("{out}: {e}"))?.len();
        report.push(format!(
            "ww2_common/{kind}-00: nam_common's {n} entries, {replaced} replaced, {added} added ({} bytes)",
            thousands(size)
        ));
    }
    *COMMON_DONE.lock().unwrap_or_else(PoisonError::into_inner) =
        Some((sig, built.clone(), report.get(from..).unwrap_or_default().to_vec()));
    Ok(built)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_as_python() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
