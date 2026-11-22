//! `culltree.py`'s reference row cache (`_RowCache`, round E3m) read and written by `bfbc2.exe culltree` (round CT1,
//! `rust/BENCHMARKS.md` "CT1"): one pickle per reference level file, named by the SHA-1 of its
//! `normcase(abspath(path))` (24 hex digits), holding
//! `{'code', 'path', 'level': (size, key), 'trees': {tree: {'stat': (size, key), 'rows' or 'error'}},
//! 'ref' or 'notlevel'}` (key: the first 128 bits of the file's SHA-256, round PR1b-1c; until then `st_mtime_ns`).
//! The rows are `{blueprint: [(label, leaf count, [((x, y, z), w, Max)], None)]}`.
//!
//! The Python and the Rust share the entries: `code` is `culltree.py`'s own (`CACHE_VERSION` and a digest of the
//! file), which `bfbc2.exe` computes from the copy of `culltree.py` it was built with, so an entry is used only by
//! code that reads the reference levels as that file does (both give the same rows: the parity tests). An entry
//! this reader does not know (another code, a damaged or foreign file, another shape) is a miss, as Python's
//! `pickle.load` failing is; it is then written again. Writes go through `bc2_gamefs::atomic::write` (a temporary
//! file beside the entry, `<entry>.tmp-<pid>-<n>`, renamed over it).
//!
//! The Rust also keeps a listing entry per `--ref-dir` folder (`list-<sha1>.pkl`, its own: `culltree.py` searches
//! the folders afresh each run): `{'kind', 'code', 'folder', 'files': [(path below the folder, size, mtime_ns, holds
//! the level mark)]}`, the size and time as the folder listing gives them. A file listed with the same size and time
//! is not read again. Both kinds are `.pkl` files of the culltree cache, so `bfbc2core.caches` counts and prunes
//! them under the cache's cap (`BFBC2_CULLTREE_CACHE_MB`) and the overall budget.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bc2_pyrt::pickle::{self, Value};
use bc2_pyrt::pickle_write;
use bc2_pyrt::py::{OMap, abspath, join};

use crate::culltool::{Entry, Found, Instance, Listed, RowStore, ShapeLeaf, Stat, TreeEntry};

/// `normcase(abspath(path))`.
fn norm(path: &str) -> String {
    abspath(path).replace('/', "\\").to_lowercase()
}

/// `culltree._code_id()` for this text of `culltree.py`: `f'{CACHE_VERSION}-{sha1(file)[:16]}'`; None when the
/// text names no `CACHE_VERSION`.
#[must_use]
pub fn code_id(source: &str) -> Option<String> {
    let version = source.lines().find_map(|l| l.strip_prefix("CACHE_VERSION = "))?.trim();
    let digest = sha1_smol::Sha1::from(source.as_bytes()).digest().to_string();
    Some(format!("{version}-{}", digest.get(..16)?))
}

/// The row cache in a folder: `get` / `put` as `_RowCache`'s. `touch` marks an entry used (`caches.touch`);
/// `wrote` says whether the run added or changed an entry (the caller then prunes the folder, as `put` does).
pub struct PickleStore {
    /// the cache's folder (`culltree.cache_folder()`)
    pub folder: String,
    /// `culltree._code_id()` of the `culltree.py` the binary was built with ([`code_id`])
    pub code: String,
    /// `caches.touch`
    pub touch: fn(&str),
    /// an entry was written this run
    pub wrote: AtomicBool,
}

impl PickleStore {
    /// A store with nothing written yet.
    pub fn new(folder: String, code: String, touch: fn(&str)) -> Self {
        Self {
            folder,
            code,
            touch,
            wrote: AtomicBool::new(false),
        }
    }

    /// `_file(path)`.
    pub fn file(&self, path: &str) -> String {
        let h = sha1_smol::Sha1::from(norm(path).as_bytes()).digest().to_string();
        join(&self.folder, &[&format!("{}.pkl", &h[..24])])
    }
}

fn int(v: &Value) -> Option<i128> {
    match v {
        Value::Int(i) => Some(i128::from(*i)),
        Value::Long(b) => {
            if b.len() > 16 {
                return None;
            }
            let fill = if b.last().is_some_and(|x| x & 0x80 != 0) {
                0xff
            } else {
                0
            };
            let mut all = [fill; 16];
            all.get_mut(..b.len())?.copy_from_slice(b);
            Some(i128::from_le_bytes(all))
        }
        _ => None,
    }
}

fn float(v: &Value) -> Option<f64> {
    match v {
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// A tuple's or list's items.
fn seq(v: &Value) -> Option<Vec<Value>> {
    match v {
        Value::Tuple(_) | Value::List(_) => Some(v.elements()),
        _ => None,
    }
}

fn stat(v: &Value) -> Option<Stat> {
    match seq(v)?.as_slice() {
        [s, m] => Some((u64::try_from(int(s)?).ok()?, int(m)?)),
        _ => None,
    }
}

fn rows(v: &Value) -> Option<Found> {
    let Value::Dict(d) = v else { return None };
    let mut found = Found::default();
    let mut labels: Vec<Arc<str>> = Vec::new();
    for (bp, xs) in d.borrow().iter() {
        let mut list = Vec::new();
        for x in seq(xs)? {
            let [label, k, leaves, obj] = seq(&x)?.try_into().ok()?;
            if !matches!(obj, Value::None) {
                return None;
            }
            let mut ls: Vec<ShapeLeaf> = Vec::new();
            for leaf in seq(&leaves)? {
                let [c, w, mx] = seq(&leaf)?.try_into().ok()?;
                let [x, y, z] = seq(&c)?.try_into().ok()?;
                ls.push(([float(&x)?, float(&y)?, float(&z)?], float(&w)?, float(&mx)?));
            }
            let label = label.as_str()?;
            let label = if let Some(l) = labels.iter().find(|l| ***l == *label) {
                l.clone()
            } else {
                labels.push(Arc::from(label));
                labels.last()?.clone()
            };
            list.push(Instance {
                label,
                k: usize::try_from(int(&k)?).ok()?,
                leaves: Arc::from(ls),
                obj: None,
            });
        }
        found.insert(bp.as_str()?.to_owned(), list);
    }
    Some(found)
}

/// The entry as `ref_rows` reads it (None: a shape it does not read).
fn entry(e: &Value, st: Stat) -> Option<Entry> {
    let mut out = Entry {
        level: st,
        ..Entry::default()
    };
    // `if e.get('notlevel')`
    match e.get("notlevel") {
        None | Some(Value::None | Value::Bool(false)) => {}
        Some(Value::Bool(true)) => {
            out.notlevel = true;
            return Some(out);
        }
        Some(_) => return None,
    }
    out.tree_ref = match e.get("ref")? {
        Value::None => None,
        Value::Str(s) => Some(s.to_string()),
        _ => return None,
    };
    let Value::Dict(trees) = e.get("trees")? else {
        return None;
    };
    for (k, t) in trees.borrow().iter() {
        let st = stat(&t.get("stat")?)?;
        // `'error' in t` first
        let rows = match t.get("error") {
            Some(m) => Err(m.as_str()?.to_owned()),
            None => Ok(rows(&t.get("rows")?)?),
        };
        out.trees.push((k.as_str()?.to_owned(), TreeEntry { stat: st, rows }));
    }
    Some(out)
}

fn s(x: &str) -> Value {
    Value::Str(Rc::from(x))
}

fn tuple(v: Vec<Value>) -> Value {
    Value::Tuple(Rc::new(v))
}

fn dict(v: Vec<(Value, Value)>) -> Value {
    Value::Dict(Rc::new(RefCell::new(v)))
}

fn stat_value(st: Stat) -> Value {
    tuple(vec![pickle_write::int(i128::from(st.0)), pickle_write::int(st.1)])
}

fn rows_value(found: &Found) -> Value {
    let mut labels: OMap<String, Value> = OMap::default();
    let mut items = Vec::with_capacity(found.items.len());
    for (bp, xs) in &found.items {
        let mut list = Vec::with_capacity(xs.len());
        for x in xs {
            let label = labels.entry_or(x.label.to_string(), || s(&x.label)).clone();
            let leaves = x
                .leaves
                .iter()
                .map(|(c, w, mx)| {
                    tuple(vec![
                        tuple(vec![Value::Float(c[0]), Value::Float(c[1]), Value::Float(c[2])]),
                        Value::Float(*w),
                        Value::Float(*mx),
                    ])
                })
                .collect();
            list.push(tuple(vec![
                label,
                Value::Int(i64::try_from(x.k).unwrap_or(i64::MAX)),
                Value::List(Rc::new(RefCell::new(leaves))),
                Value::None,
            ]));
        }
        items.push((s(bp), Value::List(Rc::new(RefCell::new(list)))));
    }
    dict(items)
}

impl PickleStore {
    /// The entry as `_RowCache.put` pickles it (`e`'s keys in the order `ref_rows` sets them).
    fn value(&self, path: &str, e: &Entry) -> Value {
        let trees = e
            .trees
            .iter()
            .map(|(k, t)| {
                let mut kv = vec![(s("stat"), stat_value(t.stat))];
                match &t.rows {
                    Ok(f) => kv.push((s("rows"), rows_value(f))),
                    Err(m) => kv.push((s("error"), s(m))),
                }
                (s(k), dict(kv))
            })
            .collect();
        let mut kv = vec![
            (s("code"), s(&self.code)),
            (s("path"), s(&norm(path))),
            (s("level"), stat_value(e.level)),
            (s("trees"), dict(trees)),
        ];
        if e.notlevel {
            kv.push((s("notlevel"), Value::Bool(true)));
        } else {
            kv.push((s("ref"), e.tree_ref.as_deref().map_or(Value::None, s)));
        }
        dict(kv)
    }
}

impl RowStore for PickleStore {
    fn get(&self, path: &str, st: Stat) -> Option<Entry> {
        let f = self.file(path);
        let data = std::fs::read(&f).ok()?;
        let e = pickle::loads(&data).ok()?;
        if !matches!(e, Value::Dict(_)) {
            return None;
        }
        let ok = e.get("code").as_ref().and_then(Value::as_str) == Some(self.code.as_str())
            && e.get("level").as_ref().and_then(stat) == Some(st)
            && e.get("path").as_ref().and_then(Value::as_str) == Some(norm(path).as_str());
        if !ok {
            return None;
        }
        let out = entry(&e, st)?;
        (self.touch)(&f);
        Some(out)
    }

    fn put(&self, path: &str, e: &Entry) {
        self.write(&self.file(path), &self.value(path, e));
    }

    fn get_list(&self, folder: &str) -> Option<Vec<Listed>> {
        let f = self.list_file(folder);
        let e = pickle::loads(&std::fs::read(&f).ok()?).ok()?;
        let ok = e.get("kind").as_ref().and_then(Value::as_str) == Some(LIST_KIND)
            && e.get("code").as_ref().and_then(Value::as_str) == Some(self.code.as_str())
            && e.get("folder").as_ref().and_then(Value::as_str) == Some(norm(folder).as_str());
        if !ok {
            return None;
        }
        let mut out = Vec::new();
        for x in seq(&e.get("files")?)? {
            let [r, size, ns, hit] = seq(&x)?.try_into().ok()?;
            let Value::Bool(hit) = hit else { return None };
            out.push((r.as_str()?.to_owned(), u64::try_from(int(&size)?).ok()?, int(&ns)?, hit));
        }
        (self.touch)(&f);
        Some(out)
    }

    fn put_list(&self, folder: &str, list: &[Listed]) {
        let files = list
            .iter()
            .map(|(r, size, ns, hit)| {
                tuple(vec![
                    s(r),
                    pickle_write::int(i128::from(*size)),
                    pickle_write::int(*ns),
                    Value::Bool(*hit),
                ])
            })
            .collect();
        let v = dict(vec![
            (s("kind"), s(LIST_KIND)),
            (s("code"), s(&self.code)),
            (s("folder"), s(&norm(folder))),
            (s("files"), Value::List(Rc::new(RefCell::new(files)))),
        ]);
        self.write(&self.list_file(folder), &v);
    }
}

/// The listing cache's entries (Rust's own: `culltree.py` lists its `--ref-dir` folders afresh each run).
const LIST_KIND: &str = "culltree-listing-1";

impl PickleStore {
    /// A `--ref-dir` folder's listing entry: `list-<sha1 of normcase(abspath(folder))>.pkl` (24 hex digits; the
    /// row cache's names have no prefix).
    pub fn list_file(&self, folder: &str) -> String {
        let h = sha1_smol::Sha1::from(norm(folder).as_bytes()).digest().to_string();
        join(&self.folder, &[&format!("list-{}.pkl", &h[..24])])
    }

    /// The value pickled to `f` ([`bc2_gamefs::atomic::write`]).
    fn write(&self, f: &str, v: &Value) {
        let Ok(data) = pickle_write::dumps(v) else {
            return;
        };
        let done = std::fs::create_dir_all(&self.folder)
            .and_then(|()| Ok(bc2_gamefs::atomic::write(std::path::Path::new(f), &[&data])?));
        if done.is_err() {
            return; // a cache that cannot be written is not used
        }
        self.wrote.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip() {
        let dir = std::env::temp_dir().join(format!("cullcache-test-{}", std::process::id()));
        let store = PickleStore::new(dir.to_string_lossy().into_owned(), "1-abc".to_owned(), |_| {});
        let mut found = Found::default();
        found.insert(
            "objects/a".to_owned(),
            vec![Instance {
                label: Arc::from("MP_005"),
                k: 2,
                leaves: Arc::from(vec![([1.5, -0.25, 3.0], 12.0, 0.5), ([0.1, 0.2, 0.3], 4.0, 1e-40)]),
                obj: None,
            }],
        );
        let mut e = Entry {
            level: (123, 1_700_000_000_123_456_700),
            tree_ref: Some("levels/mp_005/staticculltree_exclusive/abc".to_owned()),
            ..Entry::default()
        };
        e.set_tree(
            "c:\\x\\tree.dbx".to_owned(),
            TreeEntry {
                stat: (9, -5),
                rows: Ok(found),
            },
        );
        e.set_tree(
            "c:\\x\\other.dbx".to_owned(),
            TreeEntry {
                stat: (1, 2),
                rows: Err("damaged".to_owned()),
            },
        );
        let p = "C:/levels/MP_005.dbx";
        store.put(p, &e);
        assert!(store.wrote.load(Ordering::Relaxed));
        assert!(store.get(p, (123, 0)).is_none(), "another stat is a miss");
        let back = store.get(p, e.level).unwrap();
        assert_eq!(back.tree_ref, e.tree_ref);
        assert_eq!(back.trees.len(), 2);
        let rows = back.trees[0].1.rows.as_ref().unwrap();
        assert_eq!(
            rows.items[0].1[0].leaves,
            e.trees[0].1.rows.as_ref().unwrap().items[0].1[0].leaves
        );
        assert_eq!(back.trees[1].1.rows.as_ref().unwrap_err(), "damaged");
        let other = PickleStore::new(store.folder.clone(), "1-def".to_owned(), |_| {});
        assert!(other.get(p, e.level).is_none(), "another code's entry is a miss");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn code_id_as_python() {
        // culltree._code_id() of a text: CACHE_VERSION and the first 16 hex digits of its SHA-1
        let src = "x = 1\nCACHE_VERSION = 1\n";
        let d = sha1_smol::Sha1::from(src.as_bytes()).digest().to_string();
        assert_eq!(code_id(src).unwrap(), format!("1-{}", &d[..16]));
        assert!(code_id("no version").is_none());
    }
}
