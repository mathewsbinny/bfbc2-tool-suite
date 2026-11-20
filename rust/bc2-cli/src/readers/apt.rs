//! `bfbc2 apt info|chars|dump|find <movie> ...`: `bfbc2-console-readers\apt\apt.py`'s command line over
//! `bc2_formats::apt` (round E3n's dump and constants) and `bigf` (round F0). Every read outside the movie (the
//! reference's `struct.error` / `IndexError`), a BIGF index the reader refuses, a movie without its `.apt` or `.const`
//! member, and a movie the decoder refuses are handed to the Python.

#![allow(
    clippy::indexing_slicing,
    clippy::nonminimal_bool,
    clippy::case_sensitive_file_extension_comparisons,
    reason = "a port of apt.py: indexes after their bounds are checked, its conditions and comparisons as written"
)]

use std::collections::BTreeMap;

use bc2_formats::{apt, bigf};

use super::{latin1, lower};
use crate::pyfmt::docstring;
use crate::pyproc::{Fail, Py, PyErr, PyResult, read_file};

const SOURCE: &str = include_str!("../../../../bfbc2-console-readers/apt/apt.py");
const SIG: u32 = 0x0987_6543;

/// `CHAR_TYPES.get(t)`.
fn char_type(t: u32) -> Option<&'static str> {
    Some(match t {
        1 => "shape",
        2 => "text",
        3 => "font",
        4 => "button",
        5 => "sprite",
        6 => "sound",
        7 => "image",
        8 => "morph",
        9 => "movie",
        10 => "statictext",
        11 => "none",
        12 => "video",
        _ => return None,
    })
}

fn type_name(t: u32) -> String {
    char_type(t).map_or_else(|| format!("type{t}"), str::to_owned)
}

/// What `apt.Movie(path)` keeps.
struct Movie {
    name: String,
    a: Vec<u8>,
    big: bool,
    movie: i64,
    consts: Vec<String>,
}

impl Movie {
    fn word(&self, o: i64) -> PyResult<[u8; 4]> {
        let w = usize::try_from(o)
            .ok()
            .and_then(|o| self.a.get(o..)?.first_chunk::<4>().copied())
            .ok_or(Fail::Python)?;
        Ok(if self.big { [w[3], w[2], w[1], w[0]] } else { w })
    }

    fn u(&self, o: i64) -> PyResult<u32> {
        self.word(o).map(u32::from_le_bytes)
    }

    fn i(&self, o: i64) -> PyResult<i32> {
        self.word(o).map(i32::from_le_bytes)
    }

    fn f(&self, o: i64) -> PyResult<f32> {
        self.word(o).map(f32::from_le_bytes)
    }

    /// `Movie.s(p)`: the C string at p, `null` for 0 or past the end.
    fn s(&self, p: i64) -> String {
        if p <= 0 || p as u64 >= self.a.len() as u64 {
            return "null".to_owned();
        }
        let rest = self.a.get(p as usize..).unwrap_or_default();
        let e = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
        latin1(rest.get(..e).unwrap_or_default())
    }

    /// `Movie.characters()`.
    fn characters(&self) -> PyResult<Vec<(u32, u32, Option<u32>)>> {
        let m = self.movie;
        let (count, ptr) = (self.u(m + 20)?, self.u(m + 24)?);
        let mut out = Vec::new();
        for c in 0..count {
            let cp = self.u(i64::from(ptr) + 4 * i64::from(c))?;
            if cp == 0 || u64::from(cp) + 8 > self.a.len() as u64 || self.u(i64::from(cp) + 4)? != SIG {
                out.push((c, cp, None));
                continue;
            }
            out.push((c, cp, Some(self.u(i64::from(cp))?)));
        }
        Ok(out)
    }

    /// `Movie.describe(c, cp, t)` for a character with a type.
    fn describe(&self, c: u32, cp: u32, t: u32) -> PyResult<String> {
        let at = i64::from(cp);
        Ok(match t {
            5 => format!("char {c}: sprite @{cp:#x} frames={}", self.u(at + 8)?),
            7 => format!("char {c}: image @{cp:#x} texture={}", self.u(at + 8)?),
            1 => {
                let v: Vec<String> = (0..4)
                    .map(|k| self.f(at + 8 + 4 * k).map(|x| apt::format_g(f64::from(x))))
                    .collect::<PyResult<_>>()?;
                format!(
                    "char {c}: shape @{cp:#x} bounds=({},{})-({},{}) geometry={}",
                    v[0],
                    v[1],
                    v[2],
                    v[3],
                    self.u(at + 24)?
                )
            }
            9 => format!("char {c}: movie @{cp:#x} frames={}", self.u(at + 8)?),
            _ => format!("char {c}: {} @{cp:#x}", type_name(t)),
        })
    }
}

/// `apt.read_bigf(path)`: the members, a repeated name in its first place with its last data.
fn read_bigf(path: &str) -> PyResult<Vec<(String, Vec<u8>)>> {
    let b = read_file(path)?;
    let Some(members) = bigf::parse(&b) else {
        if b.get(..4) != Some(b"BIGF".as_slice()) {
            return Err(PyErr::new("ValueError", format!("{path} is not a BIGF archive (a menu .res)")).into());
        }
        return Err(Fail::Python);
    };
    Ok(members
        .into_iter()
        .map(|(n, r)| (n, b.get(r).unwrap_or_default().to_vec()))
        .collect())
}

/// `apt.Movie(path)`.
fn open(path: &str) -> PyResult<Movie> {
    let (name, a, c) = if path.to_lowercase().ends_with(".apt") {
        let (stem, _) = bc2_pyrt::py::splitext(&bc2_pyrt::py::basename(path));
        let a = read_file(path)?;
        let c = read_file(&format!("{}.const", bc2_pyrt::py::splitext(path).0))?;
        (stem, a, c)
    } else {
        let members = read_bigf(path)?;
        let apt = members
            .iter()
            .map(|(k, _)| k)
            .find(|k| k.to_lowercase().ends_with(".apt"))
            .ok_or(Fail::Python)?
            .clone();
        let name = apt.get(..apt.len() - 4).ok_or(Fail::Python)?.to_owned();
        let find = |k: &str| members.iter().find(|(n, _)| n == k).map(|(_, d)| d.clone());
        let a = find(&apt).ok_or(Fail::Python)?;
        let c = find(&format!("{name}.const")).ok_or(Fail::Python)?;
        (name, a, c)
    };
    if !c.starts_with(b"Apt constant file") {
        return Err(PyErr::new("ValueError", "not an APT constant file").into());
    }
    let w: [u8; 4] = c.get(20..24).and_then(|s| s.try_into().ok()).ok_or(Fail::Python)?;
    let (le, be) = (i64::from(u32::from_le_bytes(w)), i64::from(u32::from_be_bytes(w)));
    let n = a.len() as i64;
    let mut big = !(0 < le && le < n) && 0 < be && be < n;
    let at = |o: i64| usize::try_from(o).ok().and_then(|o| a.get(o + 4..o + 8));
    if 0 < be
        && be <= n - 8
        && at(be) == Some(b"\x09\x87\x65\x43".as_slice())
        && !(0 < le && le <= n - 8 && at(le) == Some(b"\x43\x65\x87\x09".as_slice()))
    {
        big = true;
    }
    let movie = if big { be } else { le };
    let consts = apt::constants(&c, big).map_err(|_| Fail::Python)?;
    Ok(Movie {
        name,
        a,
        big,
        movie,
        consts,
    })
}

/// Python's `str.lower()` of a needle (Latin-1 only, else the Python answers).
fn needle_lower(s: &str) -> PyResult<String> {
    lower(s)
}

/// `re.finditer(re.escape(needle), a, re.I)`'s match starts: ASCII case folding, left to right without overlaps (an
/// empty needle matches at every position).
fn matches(a: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() {
        return (0..=a.len()).collect();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= a.len() {
        if a[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let cmd = args.first().map(String::as_str);
    if args.len() < 2 || !matches!(cmd, Some("info" | "chars" | "dump" | "find")) {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    let mv = open(&args[1])?;
    match cmd {
        Some("info") => {
            let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
            for t in mv.characters()?.into_iter().filter_map(|(_, _, t)| t) {
                *kinds.entry(type_name(t)).or_default() += 1;
            }
            let m = mv.movie;
            let text = format!(
                "{}: {}, {}x{}, {} frame(s), {} constants; characters: {}",
                mv.name,
                if mv.big { "big-endian (Xbox 360)" } else { "little-endian (PC)" },
                mv.i(m + 28)?,
                mv.i(m + 32)?,
                mv.u(m + 8)?,
                mv.consts.len(),
                kinds.iter().map(|(k, v)| format!("{v} {k}")).collect::<Vec<_>>().join(", ")
            );
            py.print(&text)?;
        }
        Some("chars") => {
            let want = args.get(2).map(String::as_str).map(lower).transpose()?;
            for (c, cp, t) in mv.characters()? {
                let Some(t) = t else { continue };
                if want.as_deref().is_none_or(str::is_empty) || char_type(t) == want.as_deref() {
                    py.print(&mv.describe(c, cp, t)?)?;
                }
            }
        }
        Some("dump") => {
            let d = apt::decode(
                apt::MovieRef {
                    name: &mv.name,
                    apt: &mv.a,
                    big: mv.big,
                    movie: mv.movie,
                    consts: &mv.consts,
                },
                false,
            )
            .map_err(|_| Fail::Python)?;
            let text = format!("{}\n", d.lines.join("\n"));
            if let Some(out) = args.get(2) {
                bc2_pyrt::py::write_text(out, &text)?;
                py.print(&format!("{out}: {} lines", text.matches('\n').count()))?;
            } else {
                py.stdout_replace();
                py.write(&text)?;
            }
        }
        _ => {
            let Some(text) = args.get(2) else {
                py.print(&docstring(SOURCE))?;
                return Ok(1);
            };
            let needle = needle_lower(text)?;
            for (k, v) in mv.consts.iter().enumerate() {
                if v.to_lowercase().contains(&needle) {
                    py.print(&format!("constant {k}: {v}"))?;
                }
            }
            for m in matches(&mv.a, text.as_bytes()) {
                let st = mv.a[..m].iter().rposition(|&c| c == 0).map_or(0, |p| p + 1);
                py.print(&format!(".apt string at {st:#x}: \"{}\"", mv.s(st as i64)))?;
            }
        }
    }
    Ok(0)
}
