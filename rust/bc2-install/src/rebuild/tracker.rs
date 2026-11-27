//! The step records (Project OPFOR's `stepstamps.py` until round OR7): what each build step read, wrote, listed,
//! tested and ran when it last ran, in `work\ww2\_rebuild\track\<key>.json`, and the decision whether it must run
//! again (spec XDEP-STEP-1, XDEP-STEP-7).
//!
//! A step runs with OPFOR's `stepstamps_hook` on its `PYTHONPATH`: every Python process of the step writes what it
//! touched to `<scratch>\<pid>.json` at exit (`reads`, `writes`, `rmtrees`, `lists`, `probes`, `progs`, `modules`).
//! [`Tracker::finish`] joins those with a scan of the folders the step writes (what a program that is not Python
//! wrote there) into the record; [`Tracker::why`] says why a recorded step must run again. The record's format is
//! tracker version [`VERSION`]: the paths are Python's `os.path.normcase(os.path.abspath(p))` (lower case,
//! backslashes), the digests sha1, cached by `[size, mtime_ns]` in `hashcache.json`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{Value, json};

use crate::text::py_list_repr;

/// The tracker version a record carries; a record of another version reruns its step. 2: the Rust core's files,
/// probes, scoped scans (OR0); 3: edits (WW2-ORC1); 4: the cache budget's calls and an oversized folder's partial
/// stamps are not inputs, a record holds the step's peak memory and time (OR7).
pub const VERSION: i64 = 4;
/// An output a step that will run first rewrites (a dry run's state).
pub const PENDING: &str = "?";
/// `wake_install`'s pickled install indexes at the top of `work\ww2` and the cache budget's markers there: caches and
/// their bookkeeping, not a step's output for the scans (`stepstamps.CACHE_FILES`).
pub const CACHE_FILES: [&str; 5] = [
    "install_index.pkl",
    "install_dbnames.pkl",
    "install_templates.pkl",
    ".pruned",
    ".prune.lock",
];

/// `{path: [size, mtime_ns]}` of files (None: not a file).
pub type Stamps = BTreeMap<String, Option<(u64, i128)>>;

/// Python's `os.path.normcase(os.path.abspath(p))` on Windows: absolute, backslashes, lower case.
#[must_use]
pub fn norm(p: &Path) -> String {
    // std::path::absolute is GetFullPathNameW on Windows, as abspath: separators and dot segments resolved
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut out = abs.to_string_lossy().replace('/', "\\").to_lowercase();
    if out.len() > 3 && out.ends_with('\\') {
        out.pop();
    }
    out
}

/// The record's file name for a step's command line (`stepstamps.key`).
#[must_use]
pub fn key(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .take(60)
        .collect();
    let mut h = sha1_smol::Sha1::new();
    h.update(name.as_bytes());
    format!("{safe}_{}.json", &h.digest().to_string()[..10])
}

/// `[size, mtime_ns]` of a file (None: no such file, or not a file).
#[must_use]
pub fn stamp(p: &Path) -> Option<(u64, i128)> {
    let m = fs::metadata(p).ok()?;
    if !m.is_file() {
        return None;
    }
    let t = m.modified().ok()?;
    let ns = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i128::try_from(d.as_nanos()).unwrap_or(i128::MAX),
        Err(e) => -i128::try_from(e.duration().as_nanos()).unwrap_or(i128::MAX),
    };
    Some((m.len(), ns))
}

/// The digest of a folder's names as the hook takes it: sha1 of Python's `repr(sorted(os.listdir(p)))`
/// (`repr(None)` when it cannot be listed).
#[must_use]
pub fn listing(p: &Path) -> String {
    let text = match fs::read_dir(p) {
        Ok(rd) => {
            let mut names: Vec<String> = rd
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            py_list_repr(&names)
        }
        Err(_) => "None".to_owned(),
    };
    let mut h = sha1_smol::Sha1::new();
    h.update(text.as_bytes());
    h.digest().to_string()
}

/// What a path is now: `f` a file, `d` a folder, `-` nothing (the hook's probes).
#[must_use]
pub fn probe_state(p: &Path) -> &'static str {
    match fs::metadata(p) {
        Ok(m) if m.is_dir() => "d",
        Ok(_) => "f",
        Err(_) => "-",
    }
}

/// Every folder above the given (normalized) files, up to the drive.
pub fn ancestors<'a>(paths: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for p in paths {
        let mut d = parent(p);
        while let Some(x) = d {
            if !out.insert(x.to_owned()) {
                break;
            }
            d = parent(x);
        }
    }
    out
}

/// The folder above a normalized path (None at the drive).
#[must_use]
pub fn parent(p: &str) -> Option<&str> {
    let i = p.rfind('\\')?;
    let d = &p[..i];
    // "c:" is the drive: its parent is itself in Python (os.path.dirname('c:\\') == 'c:\\')
    if d.is_empty() || d.ends_with(':') {
        return None;
    }
    Some(d)
}

fn sha1_of(p: &Path) -> io::Result<String> {
    let mut f = fs::File::open(p)?;
    let mut h = sha1_smol::Sha1::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(buf.get(..n).unwrap_or_default());
    }
    Ok(h.digest().to_string())
}

/// sha1 of files by path, cached by `[size, mtime_ns]` in `<folder>\hashcache.json` (`stepstamps.Hashes`).
#[derive(Debug)]
pub struct Hashes {
    path: PathBuf,
    cache: HashMap<String, (u64, i128, String)>,
    dirty: bool,
}

impl Hashes {
    /// The cache in `folder` (empty when missing or unreadable).
    #[must_use]
    pub fn new(folder: &Path) -> Self {
        let path = folder.join("hashcache.json");
        let mut cache = HashMap::new();
        if let Ok(d) = fs::read(&path)
            && let Ok(Value::Object(m)) = serde_json::from_slice::<Value>(&d)
        {
            for (k, v) in m {
                if let Some([s, t, h]) = v.as_array().map(Vec::as_slice)
                    && let (Some(s), Some(t), Some(h)) = (s.as_u64(), num_i128(t), h.as_str())
                {
                    cache.insert(k, (s, t, h.to_owned()));
                }
            }
        }
        Self {
            path,
            cache,
            dirty: false,
        }
    }

    /// The file's sha1 now (None: no such file).
    pub fn of(&mut self, p: &str) -> Option<String> {
        let now = stamp(Path::new(p))?;
        if let Some(c) = self.cache.get(p)
            && (c.0, c.1) == now
        {
            return Some(c.2.clone());
        }
        let h = sha1_of(Path::new(p)).ok()?;
        self.cache.insert(p.to_owned(), (now.0, now.1, h.clone()));
        self.dirty = true;
        Some(h)
    }

    /// The file's sha1 when it had the stamp `at` (from the cache, or now when it still has it; None when that is no
    /// longer known).
    pub fn of_at(&mut self, p: &str, at: (u64, i128)) -> Option<String> {
        if let Some(c) = self.cache.get(p)
            && (c.0, c.1) == at
        {
            return Some(c.2.clone());
        }
        if stamp(Path::new(p)) != Some(at) {
            return None;
        }
        self.of(p)
    }

    /// Written back (sorted keys) when anything was added.
    ///
    /// # Errors
    /// When the cache file cannot be written.
    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let m: BTreeMap<&String, Value> = self
            .cache
            .iter()
            .map(|(k, (s, t, h))| (k, json!([s, i128_json(*t), h])))
            .collect();
        let text = serde_json::to_vec(&m).map_err(io::Error::other)?;
        if let Some(d) = self.path.parent() {
            fs::create_dir_all(d)?;
        }
        bc2_gamefs::atomic::write_rebuildable(&self.path, &[&text])?;
        self.dirty = false;
        Ok(())
    }
}

fn num_i128(v: &Value) -> Option<i128> {
    v.as_i64()
        .map(i128::from)
        .or_else(|| v.as_u64().map(i128::from))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn i128_json(t: i128) -> Value {
    i64::try_from(t).map_or_else(|_| Value::String(t.to_string()), Value::from)
}

/// `{path: [size, mtime_ns]}` of every file under `root` but its top-level folders in `skip` and the caches;
/// `only`: just these top-level names (None: all of them) - `stepstamps.scan`.
#[must_use]
pub fn scan(root: &Path, skip: &BTreeSet<String>, only: Option<&[String]>) -> Stamps {
    let mut out = BTreeMap::new();
    if !root.is_dir() {
        return out;
    }
    let tops: Vec<String> = match only {
        Some(o) => o.iter().cloned().collect::<BTreeSet<_>>().into_iter().collect(),
        None => fs::read_dir(root).map_or_default(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        }),
    };
    for top in tops {
        let t = root.join(&top);
        if skip.contains(&top) || !t.exists() {
            continue;
        }
        if t.is_file() {
            if !is_cache(&top) {
                out.insert(norm(&t), stamp(&t));
            }
            continue;
        }
        walk(&t, &mut out);
    }
    out
}

fn is_cache(name: &str) -> bool {
    let low = name.to_lowercase();
    CACHE_FILES.contains(&low.as_str())
}

fn walk(dir: &Path, out: &mut Stamps) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.filter_map(Result::ok) {
        let p = e.path();
        // as os.walk: into folders (a junction too), files by their stamps
        match fs::metadata(&p) {
            Ok(m) if m.is_dir() => walk(&p, out),
            Ok(_) => {
                if !is_cache(&e.file_name().to_string_lossy()) {
                    out.insert(norm(&p), stamp(&p));
                }
            }
            Err(_) => {}
        }
    }
}

/// One step's record (the JSON object; `step`, `tracker`, `python`, `code`, `inputs`, `lists`, `probes`, `outputs`,
/// `edits`, `scanned`, and since version 4 `peak_mb` and `seconds`).
#[derive(Debug, Clone, Default)]
pub struct Record {
    /// The step's command line.
    pub step: String,
    /// The tracker version.
    pub tracker: i64,
    /// Python's `sys.version`.
    pub python: String,
    /// Its modules and programs: sha1 (None: gone).
    pub code: BTreeMap<String, Option<String>>,
    /// Files read before writing them.
    pub inputs: BTreeMap<String, Option<String>>,
    /// Folders listed that it did not write into: the digest of their names.
    pub lists: BTreeMap<String, String>,
    /// Paths tested without reading: `f`, `d` or `-`.
    pub probes: BTreeMap<String, String>,
    /// Files written (None: removed).
    pub outputs: BTreeMap<String, Option<String>>,
    /// Files under `work` read and then rewritten in place: the version read.
    pub edits: BTreeMap<String, Option<String>>,
    /// The top-level folders scanned (None: all).
    pub scanned: Option<Vec<String>>,
    /// The step's peak memory (all its processes' commit), MiB, when measured.
    pub peak_mb: Option<f64>,
    /// The step's wall time, seconds, when measured.
    pub seconds: Option<f64>,
}

fn str_map(v: Option<&Value>) -> BTreeMap<String, Option<String>> {
    v.and_then(Value::as_object).map_or_default(|m| {
        m.iter()
            .map(|(k, v)| (k.clone(), v.as_str().map(str::to_owned)))
            .collect()
    })
}

fn str_map_req(v: Option<&Value>) -> BTreeMap<String, String> {
    v.and_then(Value::as_object).map_or_default(|m| {
        m.iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
            .collect()
    })
}

impl Record {
    /// From its JSON.
    #[must_use]
    pub fn from_json(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        Some(Self {
            step: o.get("step")?.as_str()?.to_owned(),
            tracker: o.get("tracker").and_then(Value::as_i64).unwrap_or(0),
            python: o.get("python").and_then(Value::as_str).unwrap_or("").to_owned(),
            code: str_map(o.get("code")),
            inputs: str_map(o.get("inputs")),
            lists: str_map_req(o.get("lists")),
            probes: str_map_req(o.get("probes")),
            outputs: str_map(o.get("outputs")),
            edits: str_map(o.get("edits")),
            scanned: o
                .get("scanned")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()),
            peak_mb: o.get("peak_mb").and_then(Value::as_f64),
            seconds: o.get("seconds").and_then(Value::as_f64),
        })
    }

    /// As JSON (sorted keys).
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "step": self.step, "tracker": self.tracker, "python": self.python, "code": self.code,
            "inputs": self.inputs, "lists": self.lists, "probes": self.probes, "outputs": self.outputs,
            "edits": self.edits, "scanned": self.scanned, "peak_mb": self.peak_mb, "seconds": self.seconds,
        })
    }
}

/// A file's state as the steps before a step leave it this run: its digest (or [`PENDING`]), the step that wrote it
/// last and whether that step ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    /// sha1, None (removed) or [`PENDING`].
    pub sha: Option<String>,
    /// The writer's index in the step list.
    pub step: usize,
    /// Whether the writer ran this run.
    pub ran: bool,
}

/// One hook record of a step's process.
#[derive(Debug, Default)]
struct Proc {
    reads: Vec<(String, (u64, i128))>,
    writes: Vec<String>,
    rmtrees: Vec<String>,
    lists: Vec<(String, String)>,
    probes: Vec<(String, String)>,
    progs: Vec<(String, Vec<String>)>,
    modules: Vec<String>,
}

fn proc_of(v: &Value) -> Proc {
    let strs = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .map_or_default(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
    };
    let pairs = |k: &str| -> Vec<(String, String)> {
        v.get(k).and_then(Value::as_object).map_or_default(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect()
        })
    };
    Proc {
        reads: v.get("reads").and_then(Value::as_object).map_or_default(|m| {
            m.iter()
                .filter_map(|(k, at)| {
                    let a = at.as_array()?;
                    Some((k.clone(), (a.first()?.as_u64()?, num_i128(a.get(1)?)?)))
                })
                .collect()
        }),
        writes: strs("writes"),
        rmtrees: strs("rmtrees"),
        lists: pairs("lists"),
        probes: pairs("probes"),
        progs: v.get("progs").and_then(Value::as_object).map_or_default(|m| {
            m.iter()
                .map(|(k, f)| {
                    let files = f
                        .as_array()
                        .map_or_default(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect());
                    (k.clone(), files)
                })
                .collect()
        }),
        modules: strs("modules"),
    }
}

/// The records of one rebuild's steps (folder: `work\ww2\_rebuild\track`).
#[derive(Debug)]
pub struct Tracker {
    /// The record folder.
    pub folder: PathBuf,
    /// `work\ww2`, normalized.
    pub work: String,
    work_path: PathBuf,
    /// The top-level folders of `work\ww2` never scanned.
    pub skip: BTreeSet<String>,
    /// The digests.
    pub hashes: Hashes,
}

impl Tracker {
    /// The tracker of `folder` over `work`; removes the scratch folders a stopped run left.
    ///
    /// # Errors
    /// When the folder cannot be made.
    pub fn new(folder: &Path, work: &Path, skip: BTreeSet<String>) -> io::Result<Self> {
        fs::create_dir_all(folder)?;
        for e in fs::read_dir(folder)?.filter_map(Result::ok) {
            if e.file_name().to_string_lossy().starts_with("_running") {
                let _ = fs::remove_dir_all(e.path());
            }
        }
        Ok(Self {
            folder: folder.to_path_buf(),
            work: norm(work),
            work_path: work.to_path_buf(),
            skip,
            hashes: Hashes::new(folder),
        })
    }

    /// The step's record (None: none, unreadable or another step's).
    #[must_use]
    pub fn record(&self, name: &str) -> Option<Record> {
        let d = fs::read(self.folder.join(key(name))).ok()?;
        let v: Value = serde_json::from_slice(&d).ok()?;
        let r = Record::from_json(&v)?;
        (r.step == name).then_some(r)
    }

    /// The step's record removed (a step that fails or is stopped has none: it reruns).
    pub fn forget(&self, name: &str) {
        let _ = fs::remove_file(self.folder.join(key(name)));
    }

    /// The scratch folder of a step's processes' records.
    #[must_use]
    pub fn scratch(&self, name: &str) -> PathBuf {
        let k = key(name);
        self.folder.join(format!("_running_{}", &k[..k.len() - 5]))
    }

    /// Before a step: its scratch folder made empty, its record forgotten, and the stamps of the outputs under work
    /// (`only`: just these top-level folders; None: all of work).
    ///
    /// # Errors
    /// When the scratch folder cannot be made.
    pub fn begin(&self, name: &str, only: Option<&[String]>) -> io::Result<(PathBuf, Stamps)> {
        let scratch = self.scratch(name);
        let _ = fs::remove_dir_all(&scratch);
        fs::create_dir_all(&scratch)?;
        self.forget(name);
        Ok((scratch, scan(&self.work_path, &self.skip, only)))
    }

    fn under_work(&self, p: &str) -> bool {
        p.len() > self.work.len() && p.starts_with(&self.work) && p.as_bytes().get(self.work.len()) == Some(&b'\\')
    }

    /// After a step that succeeded: its record from its processes' records and the outputs' stamps, written
    /// (`stepstamps.Tracker.finish`).
    ///
    /// # Errors
    /// When a process record or the record cannot be read or written.
    #[allow(
        clippy::too_many_lines,
        reason = "one pass over the processes' records, as stepstamps.finish"
    )]
    pub fn finish(
        &mut self,
        name: &str,
        python: &str,
        scratch: &Path,
        before: &Stamps,
        only: Option<&[String]>,
        measured: (Option<f64>, Option<f64>),
    ) -> io::Result<Record> {
        let mut files: Vec<PathBuf> = fs::read_dir(scratch)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        let mut procs = Vec::new();
        for f in &files {
            let v: Value = serde_json::from_slice(&fs::read(f)?).map_err(io::Error::other)?;
            procs.push(proc_of(&v));
        }
        let after = scan(&self.work_path, &self.skip, only);
        let mut written: BTreeSet<String> = BTreeSet::new();
        for pr in &procs {
            written.extend(pr.writes.iter().cloned());
            for t in &pr.rmtrees {
                let pre = format!("{t}\\");
                written.extend(before.keys().filter(|p| p.starts_with(&pre)).cloned());
            }
        }
        for p in before.keys().chain(after.keys()) {
            if before.get(p) != after.get(p) {
                written.insert(p.clone());
            }
        }
        // insertion order matters for which process's read counts (the first), as Python's dicts
        let mut inputs: Vec<(String, Option<String>)> = Vec::new();
        let mut input_set: BTreeSet<String> = BTreeSet::new();
        let mut code: BTreeMap<String, Option<String>> = BTreeMap::new();
        let mut lists: BTreeMap<String, String> = BTreeMap::new();
        let wrote_in: BTreeSet<&str> = written.iter().filter_map(|p| parent(p)).collect();
        for (i, pr) in procs.iter().enumerate() {
            let others: BTreeSet<&String> = if procs.len() > 1 {
                procs
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .flat_map(|(_, o)| o.writes.iter())
                    .collect()
            } else {
                BTreeSet::new()
            };
            for (p, at) in &pr.reads {
                if others.contains(p) || input_set.contains(p) {
                    continue;
                }
                let h = self.hashes.of_at(p, *at);
                inputs.push((p.clone(), h));
                input_set.insert(p.clone());
            }
            for m in &pr.modules {
                let h = self.hashes.of(m);
                code.insert(m.clone(), h);
            }
            for (prog, pfiles) in &pr.progs {
                if Path::new(prog).is_file() {
                    let h = self.hashes.of(prog);
                    code.insert(prog.clone(), h);
                }
                for p in pfiles {
                    if !written.contains(p) && !input_set.contains(p) {
                        let h = self.hashes.of(p);
                        inputs.push((p.clone(), h));
                        input_set.insert(p.clone());
                    }
                }
            }
            for (d, digest) in &pr.lists {
                let under_rm = pr.rmtrees.iter().any(|t| d == t || d.starts_with(&format!("{t}\\")));
                if !wrote_in.contains(d.as_str()) && !under_rm {
                    lists.entry(d.clone()).or_insert_with(|| digest.clone());
                }
            }
        }
        inputs.retain(|(p, _)| !code.contains_key(p));
        // a file under work the step wrote is its output; the version it read before rewriting it is an edit
        let mut edits = BTreeMap::new();
        let mut kept = BTreeMap::new();
        for (p, h) in inputs {
            if written.contains(&p) && self.under_work(&p) {
                edits.insert(p, h);
            } else {
                kept.insert(p, h);
            }
        }
        let wrote_under = ancestors(written.iter().map(String::as_str));
        let mut probes = BTreeMap::new();
        for pr in &procs {
            for (p, st) in &pr.probes {
                if probes.contains_key(p)
                    || kept.contains_key(p)
                    || written.contains(p)
                    || code.contains_key(p)
                    || wrote_under.contains(p)
                    || *p == self.work
                    || self.under_work(p)
                {
                    continue;
                }
                probes.insert(p.clone(), st.clone());
            }
        }
        let outputs = written.iter().map(|p| (p.clone(), self.hashes.of(p))).collect();
        let rec = Record {
            step: name.to_owned(),
            tracker: VERSION,
            python: python.to_owned(),
            code,
            inputs: kept,
            lists,
            probes,
            outputs,
            edits,
            scanned: only.map(|o| o.iter().cloned().collect::<BTreeSet<_>>().into_iter().collect()),
            peak_mb: measured.0,
            seconds: measured.1,
        };
        let text = serde_json::to_vec(&rec.to_json()).map_err(io::Error::other)?;
        bc2_gamefs::atomic::write(&self.folder.join(key(name)), &[&text])?;
        self.hashes.save()?;
        let _ = fs::remove_dir_all(scratch);
        Ok(rec)
    }

    /// Why the step must run (None: it need not) - `stepstamps.Tracker.why`. `state`: the files as the steps before it
    /// leave them this run; `last_writer`: the output of the last step that writes each file; `later_dirs`: the
    /// folders the steps after it write files into.
    pub fn why(
        &mut self,
        rec: Option<&Record>,
        python: &str,
        state: &HashMap<String, FileState>,
        last_writer: &HashMap<String, Option<String>>,
        later_dirs: &BTreeSet<String>,
    ) -> Option<String> {
        let Some(rec) = rec else {
            return Some("no record".to_owned());
        };
        if rec.tracker != VERSION {
            return Some("tracker changed".to_owned());
        }
        if rec.python != python {
            return Some("Python changed".to_owned());
        }
        for (p, sha) in &rec.code {
            if self.hashes.of(p) != *sha {
                return Some(format!("code changed: {p}"));
            }
        }
        for (p, sha) in &rec.inputs {
            let have = match state.get(p) {
                Some(s) => s.sha.clone(),
                None => self.hashes.of(p),
            };
            if have.as_deref() == Some(PENDING) {
                return Some(format!("reads a file a step before it rewrites: {p}"));
            }
            if sha.is_none() || have != *sha {
                return Some(format!("input changed: {p}"));
            }
        }
        let own = ancestors(rec.outputs.keys().filter(|p| self.under_work(p)).map(String::as_str));
        for (d, digest) in &rec.lists {
            if !later_dirs.contains(d) && !own.contains(d) && listing(Path::new(d)) != *digest {
                return Some(format!("folder changed: {d}"));
            }
        }
        for (p, was) in &rec.probes {
            if !last_writer.contains_key(p) && probe_state(Path::new(p)) != was {
                return Some(format!("tested path changed: {p}"));
            }
        }
        for (p, sha) in &rec.outputs {
            if state.get(p).is_some_and(|s| s.ran) {
                return Some(format!("output rewritten by a step that ran: {p}"));
            }
            let now = self.hashes.of(p);
            let last = last_writer.get(p).cloned().unwrap_or_else(|| sha.clone());
            if now != *sha && now != last {
                return Some(format!("output changed on disk: {p}"));
            }
        }
        None
    }

    /// The earlier skipped steps whose recorded output a step about to run reads, or edits in place, but the disk no
    /// longer holds (a later step rewrote it): their indexes (`stepstamps.Tracker.stale_inputs`).
    pub fn stale_inputs(&mut self, rec: Option<&Record>, state: &HashMap<String, FileState>) -> BTreeSet<usize> {
        let mut back = BTreeSet::new();
        let Some(rec) = rec else { return back };
        for p in rec.inputs.keys().chain(rec.edits.keys()) {
            if let Some(s) = state.get(p)
                && !s.ran
                && self.hashes.of(p) != s.sha
            {
                back.insert(s.step);
            }
        }
        back
    }
}
