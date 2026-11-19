//! texqa's `analyse`: the QA row of one texture, with the keys in the order the Python sets them, its flags and
//! their words.

use std::sync::LazyLock;

use regex::Regex;

use super::decode::{DErr, DResult, Img, QaErr, Tex, name_of, read_pc, read_xbox};
use super::measure::{self as ms, Arr};
use crate::pyfmt::float_repr;
use crate::pyjson::Value;
use crate::pyproc::Fail;

const PSNR_SAME: f64 = 35.0;
const PSNR_OTHER: f64 = 20.0;
const BETTER: f64 = 6.0;
const CHAIN_TOL: (f64, f64) = (3.0, 0.25);
const CHAIN_ABS: f64 = 24.0;
const CHAIN_CORR: f64 = 0.75;
const SRGB: u32 = 1;
const HALVED: u32 = 2;

static COMPOSITE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(^|/)levels/[^/]+/emittertexture(alpha|normals)\.res$").ok());
static SPLIT: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"[_\-. ]+").ok());
static DIGITS: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"\d+$").ok());

/// `COMPOSITE.search(name.lower())`.
pub fn composite(lower: &str) -> bool {
    COMPOSITE.as_ref().is_some_and(|r| r.is_match(lower))
}

const ROLE_TOKENS: &[(&str, &[&str])] = &[
    (
        "normal",
        &[
            "n",
            "nm",
            "nrm",
            "normal",
            "normals",
            "norm",
            "dxn",
            "bump",
            "normalmap",
        ],
    ),
    ("specular", &["s", "spec", "specular", "sm", "sp", "gloss"]),
    (
        "mask",
        &["m", "a", "ao", "mask", "alpha", "occ", "oc", "op", "opacity", "sss"],
    ),
    (
        "color",
        &[
            "d", "cm", "c", "col", "color", "colour", "diffuse", "df", "dif", "alb", "albedo", "rgb",
        ],
    ),
];

fn role_formats(r: &str) -> Option<&'static [(u32, f64)]> {
    Some(match r {
        "normal" => &[(18, 0.88), (19, 0.10), (20, 0.01)],
        "color" => &[(0, 0.64), (2, 0.30), (1, 0.03), (10, 0.01), (9, 0.01)],
        "specular" => &[(0, 0.87), (2, 0.05), (9, 0.04), (10, 0.03)],
        "mask" => &[(0, 0.80), (10, 0.06), (2, 0.05), (18, 0.05), (19, 0.03)],
        "ui" => &[(2, 0.93), (0, 0.06), (9, 0.01)],
        _ => return None,
    })
}

fn role_srgb(r: &str) -> f64 {
    match r {
        "color" => 0.97,
        "specular" => 0.88,
        "ui" => 1.0,
        _ => 0.0,
    }
}

/// `role(name)`.
pub fn role(name: &str) -> &'static str {
    let low = name.to_lowercase().replace('\\', "/");
    if low.starts_with("awards/") || low.starts_with("menu/") || low.starts_with("ui/") || low.contains("/ui/") {
        return "ui";
    }
    let stem = low.rsplit('/').next().unwrap_or("");
    let stem = stem.strip_suffix(".res").unwrap_or(stem);
    let (Some(split), Some(digits)) = (SPLIT.as_ref(), DIGITS.as_ref()) else {
        return "other";
    };
    let toks: Vec<&str> = split.split(stem).filter(|x| !x.is_empty()).collect();
    for tok in toks.iter().rev() {
        let bare = digits.replace(tok, "");
        for (r, words) in ROLE_TOKENS {
            if words.contains(tok) || words.contains(&bare.as_ref()) {
                return r;
            }
        }
    }
    "other"
}

/// Python's `str()` of a float in an f-string.
fn fs(x: f64) -> String {
    float_repr(x)
}

/// `f'{x:.{n}%}'`.
pub fn pct(x: f64, n: usize) -> String {
    format!("{:.n$}%", x * 100.0)
}

fn name_or(fmt: u32) -> String {
    name_of(i128::from(fmt)).map_or_else(|| fmt.to_string(), str::to_owned)
}

fn name_none(fmt: u32) -> String {
    name_of(i128::from(fmt)).map_or_else(|| "None".to_owned(), str::to_owned)
}

/// A dict being built: keys keep their first place.
#[derive(Default)]
pub struct Row(pub Vec<(String, Value)>);

impl Row {
    pub fn set(&mut self, k: &str, v: Value) {
        if let Some(slot) = self.0.iter_mut().find(|(key, _)| key == k) {
            slot.1 = v;
        } else {
            self.0.push((k.to_owned(), v));
        }
    }
}

struct Flags(Vec<[String; 4]>);

impl Flags {
    fn add(&mut self, code: &str, sev: &str, cause: &str, detail: String) {
        self.0.push([code.to_owned(), sev.to_owned(), cause.to_owned(), detail]);
    }
}

fn fl(v: f64) -> Value {
    Value::Float(v)
}

fn list_f(v: &[f64]) -> Value {
    Value::floats(v)
}

fn idx(ch: &str) -> Vec<usize> {
    ch.chars().filter_map(|c| "rgba".find(c)).collect()
}

fn opt_f(v: Option<f64>) -> Value {
    v.map_or(Value::Null, Value::Float)
}

/// What a texture's analysis ends with: its row, texqa's own error (an unreadable row), or a hand-off.
pub enum Out {
    Row(Value),
    Err(String),
}

fn to_out(e: DErr) -> Result<Out, Fail> {
    match e {
        DErr::Qa(QaErr(m)) => Ok(Out::Err(format!("TexQaError: {m}"))),
        DErr::Py => Err(Fail::Python),
    }
}

fn stats(a: &Img, ch: &str) -> Value {
    let v = Arr::of(a, &idx(ch));
    let r2 = |x: Vec<f64>| Value::List(x.into_iter().map(|q| Value::Float(ms::py_round(q, 2))).collect());
    Value::dict([("mean", r2(ms::mean0(&v.d, v.c))), ("std", r2(ms::std0(&v.d, v.c)))])
}

fn chain_errors(t: &Tex) -> Result<Vec<Option<[f64; 2]>>, Fail> {
    let ch = idx(&t.channels());
    let mut out = Vec::new();
    for m in 1..t.mips as usize {
        let (Some(a), Some(b)) = (t.level(m - 1, 0), t.level(m, 0)) else {
            out.push(None);
            continue;
        };
        if b.h.min(b.w) < 4 {
            out.push(None);
            continue;
        }
        let p = ms::to_size(&Arr::of(a, &ch), b.h, b.w)?;
        let q = Arr::of(b, &ch);
        out.push(match p {
            None => None,
            Some(p) => {
                let d: Vec<f64> = p.d.iter().zip(&q.d).map(|(x, y)| (x - y).abs()).collect();
                Some([ms::py_round(ms::mean(&d), 2), ms::correlation(&p.d, &q.d)?])
            }
        });
    }
    Ok(out)
}

fn chain_value(c: &[Option<[f64; 2]>]) -> Value {
    Value::List(c.iter().map(|x| x.map_or(Value::Null, |v| list_f(&v))).collect())
}

fn degenerate(t: &Tex) -> Vec<(String, String)> {
    let Some(a) = t.level(0, 0) else {
        return vec![("degenerate: no levels".into(), "the texture has no level".into())];
    };
    let mut out = Vec::new();
    let ch = t.channels();
    let col: String = ch.chars().filter(|&c| c != 'a').collect();
    let alpha = || a.px.chunks_exact(4).map(|p| p[3]);
    if ch.contains('a') && alpha().max().unwrap_or(0) == 0 {
        out.push(("degenerate: all transparent".into(), "alpha 0 everywhere".into()));
    }
    if !col.is_empty() && !matches!(t.fmt, 19 | 20) {
        let v = Arr::of(a, &idx(&col));
        let mx = v.d.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mn = v.d.iter().copied().fold(f64::INFINITY, f64::min);
        if mx <= 4.0 {
            out.push(("degenerate: all black".into(), format!("largest value {mx:.0}")));
        } else if mn >= 251.0 {
            out.push(("degenerate: all white".into(), format!("smallest value {mn:.0}")));
        }
    }
    if a.h * a.w >= 64
        && idx(&ch).iter().all(|&c| {
            let v: Vec<f64> = a.px.chunks_exact(4).map(|p| f64::from(p[c])).collect();
            ms::std(&v) < 0.5
        })
    {
        let vals: Vec<String> = idx(&ch).iter().map(|&c| a.at(0, 0, c).to_string()).collect();
        out.push((
            "degenerate: constant".into(),
            format!("one value: [{}]", vals.join(", ")),
        ));
    }
    if !t.floats.is_empty() {
        let bad: usize = t
            .floats
            .iter()
            .map(|f| f.iter().filter(|x| !x.is_finite()).count())
            .sum();
        if bad > 0 {
            out.push((
                "degenerate: NaN / infinity".into(),
                format!("{bad} half floats not finite"),
            ));
        }
    }
    out
}

fn normal_check(t: &Tex) -> Option<Vec<(String, Value)>> {
    let a = t.level(0, 0)?;
    let v: Vec<f64> =
        a.px.chunks_exact(4)
            .flat_map(|p| (0..3).map(move |c| f64::from(p[c]) / 127.5 - 1.0))
            .collect();
    let n = v.len() / 3;
    if matches!(t.fmt, 19 | 20) {
        let x: Vec<f64> = (0..n).map(|i| v[i * 3]).collect();
        let y: Vec<f64> = (0..n).map(|i| v[i * 3 + 1]).collect();
        let r2: Vec<f64> = (0..n).map(|i| x[i] * x[i] + y[i] * y[i]).collect();
        let out = r2.iter().filter(|&&r| r > 1.05).count();
        let z: Vec<f64> = r2.iter().map(|r| (1.0 - r).clamp(0.0, 1.0).sqrt()).collect();
        return Some(vec![
            ("mean_x".into(), fl(ms::py_round(ms::mean(&x), 3))),
            ("mean_y".into(), fl(ms::py_round(ms::mean(&y), 3))),
            (
                "outside_disc".into(),
                fl(ms::py_round((0.0 + out as f64) / n as f64, 4)),
            ),
            ("mean_z".into(), fl(ms::py_round(ms::mean(&z), 3))),
        ]);
    }
    let unit = (0..n)
        .filter(|&i| {
            let mut s = 0.0;
            for c in 0..3 {
                s += v[i * 3 + c] * v[i * 3 + c];
            }
            let q = s.sqrt();
            q > 0.8 && q < 1.2
        })
        .count();
    let means = ms::mean0(&v, 3);
    let mut am = 0;
    for k in 1..3 {
        if means[k] > means[am] {
            am = k;
        }
    }
    Some(vec![
        ("mean_x".into(), fl(ms::py_round(means[0], 3))),
        ("mean_y".into(), fl(ms::py_round(means[1], 3))),
        ("mean_z".into(), fl(ms::py_round(means[2], 3))),
        ("z_channel".into(), Value::Str(["r", "g", "b"][am].into())),
        ("unit_share".into(), fl(ms::py_round((0.0 + unit as f64) / n as f64, 4))),
    ])
}

struct Lv {
    level: usize,
    size: [usize; 2],
    psnr: f64,
    ssim: Option<f64>,
    value: Value,
}

fn compare_levels(ours: &Tex, rf: &Tex, ch: &str) -> Result<Vec<Lv>, Fail> {
    let ix = idx(ch);
    let mut out = Vec::new();
    for (s, m, a) in &ours.levels {
        if *s != 0 && *m != 0 {
            continue;
        }
        let mut b = None;
        for (s2, _, x) in &rf.levels {
            if s2 == s && x.h >= a.h && x.w >= a.w {
                b = Some(x);
            }
        }
        let Some(b) = b else { continue };
        let Some((x, y)) = ms::common(&Arr::of(a, &ix), &Arr::of(b, &ix))? else {
            continue;
        };
        let p = ms::psnr(&x.d, &y.d);
        let ss = ms::ssim(&x, &y);
        let mx =
            x.d.iter()
                .zip(&y.d)
                .map(|(p, q)| (p - q).abs())
                .fold(f64::NEG_INFINITY, f64::max);
        let diff: Vec<f64> = x.d.iter().zip(&y.d).map(|(p, q)| p - q).collect();
        let md: Vec<f64> = ms::mean0(&diff, ix.len())
            .into_iter()
            .map(|q| ms::py_round(q, 2))
            .collect();
        let size = [a.w, a.h];
        let value = Value::dict([
            ("face", Value::Int(*s as i128)),
            ("level", Value::Int(*m as i128)),
            (
                "size",
                Value::List(vec![Value::Int(a.w as i128), Value::Int(a.h as i128)]),
            ),
            ("psnr", fl(p)),
            ("ssim", opt_f(ss)),
            ("max_error", fl(ms::py_round(mx, 1))),
            ("mean_diff", list_f(&md)),
        ]);
        out.push(Lv {
            level: *m,
            size,
            psnr: p,
            ssim: ss,
            value,
        });
    }
    Ok(out)
}

const ORIENT: [&str; 6] = [
    "flipped vertically",
    "mirrored",
    "rotated 180",
    "transposed",
    "rotated 90",
    "rotated 270",
];

/// `best_match`'s answer: the PSNR as they are, and the better permutation / orientation (how, its PSNR).
type Match = (f64, Option<(String, f64)>);

/// `best_match`: (psnr, Some((how, psnr))) or None (no common size).
fn best_match(ours: &Img, rf: &Img, ch: &str) -> Result<Option<Match>, Fail> {
    let all = [0, 1, 2, 3];
    let Some((a, b)) = ms::common(&Arr::of(ours, &all), &Arr::of(rf, &all))? else {
        return Ok(None);
    };
    let small = |mut x: Arr| -> Result<Arr, Fail> {
        while x.h.min(x.w) > 128 {
            x = ms::box_(&x)?;
        }
        Ok(x)
    };
    let (a, b) = (small(a)?, small(b)?);
    if (a.h, a.w) != (b.h, b.w) {
        return Ok(None);
    }
    let cols: Vec<usize> = ch
        .chars()
        .filter(|&c| c != 'a')
        .filter_map(|c| "rgba".find(c))
        .collect();
    let bc = b.pick(&cols);
    let base = ms::psnr(&a.pick(&cols).d, &bc.d);
    if base == f64::INFINITY || base >= PSNR_SAME {
        return Ok(Some((base, None)));
    }
    let mut best: (f64, Option<String>) = (base, None);
    let spread = cols.iter().filter(|&&i| ms::std(&a.chan(i)) > 2.0).count();
    let letters = |p: &[usize]| p.iter().map(|&i| ["r", "g", "b", "a"][i]).collect::<String>();
    if spread >= 2 {
        for perm in permutations(&cols) {
            if perm == cols {
                continue;
            }
            let p = ms::psnr(&a.pick(&perm).d, &bc.d);
            if p > best.0 {
                best = (p, Some(format!("channels {} (as {})", letters(&perm), letters(&cols))));
            }
        }
    }
    for (k, nm) in ORIENT.iter().enumerate() {
        let x = a.orient(k);
        if (x.h, x.w) != (b.h, b.w) {
            continue;
        }
        let p = ms::psnr(&x.pick(&cols).d, &bc.d);
        if p > best.0 {
            best = (p, Some((*nm).to_owned()));
        }
    }
    if let Some(how) = best.1
        && best.0 - base >= BETTER
        && (best.0 >= 25.0 || best.0 - base >= 2.0 * BETTER)
    {
        return Ok(Some((base, Some((how, best.0)))));
    }
    Ok(Some((base, None)))
}

/// `itertools.permutations(v)` in its order.
fn permutations(v: &[usize]) -> Vec<Vec<usize>> {
    if v.len() <= 1 {
        return vec![v.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..v.len() {
        let mut rest = v.to_vec();
        let x = rest.remove(i);
        for mut p in permutations(&rest) {
            p.insert(0, x);
            out.push(p);
        }
    }
    out
}

fn alpha_used(t: &Tex) -> bool {
    t.level(0, 0)
        .is_some_and(|a| t.channels().contains('a') && a.px.chunks_exact(4).map(|p| p[3]).min().unwrap_or(255) < 250)
}

/// `analyse(name, ours, source, None, orig, 'xbox')` (no DICE copy: texqa's DICE index is the Python's).
pub fn analyse(name: &str, ours_res: &[u8], source: Option<&[u8]>, orig_res: Option<&[u8]>) -> Result<Out, Fail> {
    match analyse_in(name, ours_res, source, orig_res) {
        Ok(v) => Ok(Out::Row(v)),
        Err(e) => to_out(e),
    }
}

#[allow(clippy::too_many_lines)]
fn analyse_in(name: &str, ours_res: &[u8], source: Option<&[u8]>, orig_res: Option<&[u8]>) -> DResult<Value> {
    let r = role(name);
    let mut row = Row::default();
    row.set("name", Value::Str(name.to_owned()));
    row.set("role", Value::Str(r.to_owned()));
    row.set("flags", Value::List(vec![]));
    let mut flags = Flags(Vec::new());
    let ours = read_pc(ours_res)?;
    let och = ours.channels();
    let mut o = Row::default();
    o.set("format", Value::Int(i128::from(ours.fmt)));
    o.set("flags", Value::Int(i128::from(ours.flags)));
    o.set("type", Value::Int(i128::from(ours.typ)));
    o.set(
        "size",
        Value::List(vec![Value::Int(i128::from(ours.w)), Value::Int(i128::from(ours.h))]),
    );
    o.set("mips", Value::Int(i128::from(ours.mips)));
    o.set("channels", Value::Str(och.clone()));
    let Some(a0) = ours.level(0, 0) else {
        return Err(DErr::Qa(QaErr(format!(
            "{name}: no level 0 to read ({} mip levels)",
            ours.mips
        ))));
    };
    o.set("stats", stats(a0, &och));
    for e in &ours.size_errors {
        flags.add("level size", "error", "converter", e.clone());
    }
    let full = 32 - ours.w.max(ours.h).leading_zeros();
    o.set("full_chain", Value::Int(i128::from(full)));
    let mut src: Option<Tex> = None;
    let mut source_error = None;
    if let Some(s) = source {
        match read_xbox(s) {
            Ok(t) => src = Some(t),
            Err(DErr::Qa(QaErr(m))) => source_error = Some(m),
            Err(DErr::Py) => return Err(DErr::Py),
        }
    }
    let orig = match orig_res {
        Some(b) => Some(read_pc(b)?),
        None => None,
    };
    let src_deg: Vec<String> = src
        .as_ref()
        .map_or_else(Vec::new, |s| degenerate(s).into_iter().map(|x| x.0).collect());
    for (code, detail) in degenerate(&ours) {
        let cause = if src_deg.contains(&code) {
            "source"
        } else if src.is_some() {
            "converter"
        } else {
            "unknown"
        };
        flags.add(&code, "error", cause, detail);
    }
    o.set("zero_blocks", fl(ms::py_round(ours.zero_blocks, 4)));
    if let Some(s) = &src
        && ours.zero_blocks > 0.02 + 2.0 * s.zero_blocks
    {
        flags.add(
            "zero blocks",
            "warn",
            "converter",
            format!(
                "{} of level 0 blocks are all zero bytes (the source {})",
                pct(ours.zero_blocks, 1),
                pct(s.zero_blocks, 1)
            ),
        );
    }
    let ce = chain_errors(&ours)?;
    let mut sce = None;
    if ours.mips < full && ours.mips > 1 && src.as_ref().is_none_or(|s| ours.mips < s.mips) {
        flags.add(
            "mips: short chain",
            "warn",
            if src.is_some() { "converter" } else { "unknown" },
            format!("{} levels of {full}", ours.mips)
                + &src
                    .as_ref()
                    .map_or(String::new(), |s| format!(" (the source has {})", s.mips)),
        );
    }
    if let Some(s) = &src {
        let sc = chain_errors(s)?;
        for (m, (x, y)) in ce.iter().zip(&sc).enumerate() {
            let m = m + 1;
            let (Some(x), Some(y)) = (x, y) else { continue };
            if x[0] > (y[0] + CHAIN_TOL.0).max(y[0] * (1.0 + CHAIN_TOL.1)) {
                let bad = x[1] < CHAIN_CORR.min(y[1] - 0.1);
                flags.add(
                    if bad {
                        "mips: level not its parent"
                    } else {
                        "mips: noisier chain"
                    },
                    if bad { "error" } else { "info" },
                    "converter",
                    format!(
                        "level {m}: {} of 255 from level {} box-filtered, correlation {} (the source's own {}, {})",
                        fs(x[0]),
                        m - 1,
                        fs(x[1]),
                        fs(y[0]),
                        fs(y[1])
                    ),
                );
                break;
            }
        }
        sce = Some(sc);
    } else {
        for (m, x) in ce.iter().enumerate() {
            if let Some(x) = x
                && x[0] > CHAIN_ABS
                && x[1] < CHAIN_CORR
            {
                flags.add(
                    "mips: level not its parent",
                    "warn",
                    "unknown",
                    format!(
                        "level {}: {} of 255 from level {m} box-filtered, correlation {}",
                        m + 1,
                        fs(x[0]),
                        fs(x[1])
                    ),
                );
                break;
            }
        }
    }
    // the keys so far: ours (its dict built), source_error, chain, source_chain
    row.set("ours", Value::Dict(o.0));
    if let Some(e) = source_error {
        row.set("source_error", Value::Str(e));
    }
    row.set("chain", chain_value(&ce));
    if let Some(sc) = &sce {
        row.set("source_chain", chain_value(sc));
    }
    let cause = |same: bool| -> &'static str {
        if src.is_none() {
            "unknown"
        } else if same {
            "source"
        } else {
            "converter"
        }
    };
    let same_pic = src
        .as_ref()
        .is_some_and(|s| (s.fmt, s.w, s.h) == (ours.fmt, ours.w, ours.h));
    if (matches!(ours.fmt, 18..=20) || r == "normal") && !composite(&name.to_lowercase()) {
        let nc = normal_check(&ours).unwrap_or_default();
        let get = |k: &str| nc.iter().find(|(key, _)| key == k).and_then(|(_, v)| v.as_f64());
        let gets = |k: &str| {
            nc.iter()
                .find(|(key, _)| key == k)
                .and_then(|(_, v)| v.as_str().map(str::to_owned))
        };
        row.set("normal", Value::Dict(nc.clone()));
        if matches!(ours.fmt, 19 | 20) {
            if get("outside_disc").unwrap_or(0.0) > 0.05 {
                flags.add(
                    "normal: x / y outside the unit disc",
                    "warn",
                    cause(same_pic),
                    format!(
                        "{} of texels - z = sqrt(1 - x^2 - y^2) clamps there",
                        pct(get("outside_disc").unwrap_or(0.0), 1)
                    ),
                );
            }
            if get("mean_x").unwrap_or(0.0).abs() > 0.3 || get("mean_y").unwrap_or(0.0).abs() > 0.3 {
                flags.add(
                    "normal: biased x / y",
                    "warn",
                    cause(same_pic),
                    format!(
                        "mean x {}, y {}",
                        fs(get("mean_x").unwrap_or(0.0)),
                        fs(get("mean_y").unwrap_or(0.0))
                    ),
                );
            }
        } else if matches!(ours.fmt, 18 | 0)
            && r == "normal"
            && (gets("z_channel").as_deref() != Some("b") || get("mean_z").unwrap_or(1.0) < 0.3)
        {
            flags.add(
                "normal: z not in blue",
                if ours.fmt == 18 { "error" } else { "warn" },
                cause(same_pic),
                format!(
                    "mean x {}, y {}, z {} (largest: {})",
                    fs(get("mean_x").unwrap_or(0.0)),
                    fs(get("mean_y").unwrap_or(0.0)),
                    fs(get("mean_z").unwrap_or(0.0)),
                    gets("z_channel").unwrap_or_default()
                ),
            );
        }
        if ours.flags & SRGB != 0 {
            flags.add(
                "srgb: normal map with the sRGB bit",
                "error",
                cause(src.as_ref().is_some_and(|s| s.flags & SRGB != 0)),
                format!("flags {} (DICE's normal maps: never)", ours.flags),
            );
        }
    }
    if let Some(rfs) = role_formats(r)
        && !rfs.iter().any(|(f, _)| *f == ours.fmt)
    {
        let list: Vec<String> = rfs
            .iter()
            .map(|(k, v)| format!("{} {}", name_or(*k), pct(*v, 0)))
            .collect();
        flags.add(
            "format: not DICE's for the role",
            if matches!(r, "normal" | "color") {
                "warn"
            } else {
                "info"
            },
            cause(src.as_ref().is_some_and(|s| s.fmt == ours.fmt)),
            format!("{} for a {r} texture; DICE's: {}", name_or(ours.fmt), list.join(", ")),
        );
    }
    if ours.fmt == 18 && !matches!(r, "normal" | "other" | "mask") {
        flags.add(
            "format: normal-map DXT1 for a non-normal",
            "warn",
            cause(src.as_ref().is_some_and(|s| s.fmt == 18)),
            format!("format 18 on a {r} texture"),
        );
    }
    if matches!(r, "color" | "ui") && ours.flags & SRGB == 0 && !matches!(ours.fmt, 10 | 13) {
        flags.add(
            "srgb: colour map without the sRGB bit",
            "info",
            cause(src.as_ref().is_some_and(|s| s.flags & SRGB == 0)),
            format!(
                "flags {} (DICE's {r} textures: {} with it)",
                ours.flags,
                pct(role_srgb(r), 0)
            ),
        );
    }
    if matches!(ours.fmt, 2 | 1) && och.contains('a') && a0.px.chunks_exact(4).map(|p| p[3]).min().unwrap_or(0) == 255 {
        flags.add(
            "alpha: format with alpha, none used",
            "info",
            "unknown",
            "alpha 255 everywhere".to_owned(),
        );
    }
    if let Some(s) = &src {
        vs_source(&mut row, &mut flags, &ours, s, orig.as_ref())?;
    }
    if flags.0.iter().any(|f| f[2] == "intentional") {
        for f in &mut flags.0 {
            if f[2] == "converter" {
                "repaint".clone_into(&mut f[2]);
            }
        }
    }
    let flag_values: Vec<Value> = flags
        .0
        .iter()
        .map(|f| {
            Value::dict([
                ("code", Value::Str(f[0].clone())),
                ("severity", Value::Str(f[1].clone())),
                ("cause", Value::Str(f[2].clone())),
                ("detail", Value::Str(f[3].clone())),
            ])
        })
        .collect();
    row.set("flags", Value::List(flag_values));
    let score = score_of(
        &flags,
        row.0
            .iter()
            .find(|(k, _)| k == "psnr_source")
            .and_then(|(_, v)| v.as_f64()),
    );
    row.set("score", score);
    Ok(Value::Dict(row.0))
}

/// `score(row)`: an int 0 when nothing counts, a float otherwise (NumPy's rounding once a NumPy PSNR is added).
fn score_of(flags: &Flags, psnr_source: Option<f64>) -> Value {
    let counted: Vec<f64> = flags
        .0
        .iter()
        .filter(|f| f[2] != "intentional" && f[2] != "dice")
        .map(|f| match f[1].as_str() {
            "error" => 100.0,
            "warn" => 10.0,
            _ => 0.0,
        })
        .collect();
    let mut s: Option<f64> = if counted.is_empty() {
        None
    } else {
        let mut t = 0.0;
        for c in counted {
            t += c;
        }
        Some(t)
    };
    let mut numpy = false;
    if let Some(p) = psnr_source
        && p != f64::INFINITY
    {
        let d = 40.0 - p;
        if d > 0.0 {
            numpy = true;
            s = Some(s.unwrap_or(0.0) + d);
        } else {
            s = Some(s.unwrap_or(0.0) + 0.0);
        }
    }
    match s {
        None => Value::Int(0),
        Some(v) if numpy => Value::Float(ms::np_round(v, 2)),
        Some(v) => Value::Float(ms::py_round(v, 2)),
    }
}

#[allow(clippy::too_many_lines)]
fn vs_source(row: &mut Row, flags: &mut Flags, ours: &Tex, src: &Tex, orig: Option<&Tex>) -> DResult<()> {
    let sch = src.channels();
    let mut sv = Row::default();
    sv.set("format", Value::Int(i128::from(src.fmt)));
    sv.set("flags", Value::Int(i128::from(src.flags)));
    sv.set(
        "size",
        Value::List(vec![Value::Int(i128::from(src.w)), Value::Int(i128::from(src.h))]),
    );
    sv.set("mips", Value::Int(i128::from(src.mips)));
    sv.set("channels", Value::Str(sch.clone()));
    let (Some(s0), Some(a0)) = (src.level(0, 0), ours.level(0, 0)) else {
        return Err(DErr::Py); // the Python's assert
    };
    let mut ch: String = ours.channels().chars().filter(|c| sch.contains(*c)).collect();
    if ch.is_empty() {
        "r".clone_into(&mut ch);
    }
    sv.set("stats", stats(s0, &ch));
    row.set("source", Value::Dict(sv.0));
    if u64::from(ours.w) * u64::from(src.h) != u64::from(ours.h) * u64::from(src.w) {
        flags.add(
            "aspect",
            "error",
            "converter",
            format!("{} x {}, the source {} x {}", ours.w, ours.h, src.w, src.h),
        );
    } else if (ours.w, ours.h) != (src.w, src.h) {
        flags.add(
            "size",
            "info",
            "converter",
            format!("{} x {}, the source {} x {}", ours.w, ours.h, src.w, src.h),
        );
    }
    if ours.typ != src.typ {
        flags.add(
            "type",
            "error",
            "converter",
            format!("type {}, the source {}", ours.typ, src.typ),
        );
    }
    if ours.fmt != src.fmt && !matches!((src.fmt, ours.fmt), (3, 10) | (5, 9)) {
        flags.add(
            "format",
            "info",
            "converter",
            format!("{}, the source {}", name_or(ours.fmt), name_or(src.fmt)),
        );
    }
    if (ours.flags ^ src.flags) & SRGB != 0 {
        flags.add(
            "srgb: differs from the source",
            "warn",
            "converter",
            format!("flags {}, the source {}", ours.flags, src.flags),
        );
    }
    if (ours.flags ^ src.flags) & !(SRGB | HALVED) != 0 {
        flags.add(
            "flags",
            "info",
            "converter",
            format!("flags {}, the source {}", ours.flags, src.flags),
        );
    }
    if ours.mips != src.mips && (ours.w, ours.h) == (src.w, src.h) {
        flags.add(
            "mips: count",
            "warn",
            "converter",
            format!("{} levels, the source {}", ours.mips, src.mips),
        );
    }
    if alpha_used(src) && !ours.channels().contains('a') {
        flags.add(
            "alpha: lost",
            "error",
            "converter",
            format!(
                "the source's alpha is used ({}), ours {} has none",
                name_none(src.fmt),
                name_none(ours.fmt)
            ),
        );
    }
    let lv = compare_levels(ours, src, &ch)?;
    row.set("levels", Value::List(lv.iter().map(|l| l.value.clone()).collect()));
    let p0 = lv.first().map(|l| l.psnr);
    row.set("psnr_source", opt_f(p0));
    let ssim_src = lv.first().and_then(|l| l.ssim);
    row.set("ssim_source", opt_f(ssim_src));
    row.set(
        "identical_source",
        Value::Bool(!lv.is_empty() && lv.iter().all(|l| l.psnr == f64::INFINITY)),
    );
    let bm = best_match(a0, s0, &ch)?;
    let (bm_psnr, better) = match &bm {
        Some((p, b)) => (Some(*p), b.clone()),
        None => (None, None),
    };
    if let Some((how, bp)) = &better {
        let bmp = bm_psnr.map_or_else(|| "None".to_owned(), fs);
        flags.add(
            if how.starts_with("channels") {
                "swizzle"
            } else {
                "orientation"
            },
            "error",
            "converter",
            format!("{how} matches the source at {} dB, as it is {bmp} dB", fs(*bp)),
        );
    }
    let ssim_txt = ssim_src.map_or_else(|| "None".to_owned(), fs);
    if let Some(p0) = p0.filter(|p| *p < PSNR_SAME)
        && better.is_none()
    {
        if let Some(orig) = orig {
            let o = compare_levels(orig, src, &ch)?;
            let po = o.first().map(|l| l.psnr);
            row.set("psnr_orig_source", opt_f(po));
            if let Some(po) = po.filter(|p| *p >= PSNR_SAME) {
                flags.add(
                    "repaint (grime.py)",
                    "info",
                    "intentional",
                    format!(
                        "{} dB from the source; the tool's own output (.orig) {} dB",
                        fs(p0),
                        fs(po)
                    ),
                );
                return Ok(());
            }
        }
        let worst = lv.iter().map(|l| l.psnr).fold(f64::INFINITY, f64::min);
        flags.add(
            "differs from the source",
            if p0 < PSNR_OTHER { "error" } else { "warn" },
            "converter",
            format!("level 0 {} dB (SSIM {ssim_txt}), worst level {} dB", fs(p0), fs(worst)),
        );
    } else if !lv.is_empty()
        && let Some(b) = lv.iter().find(|l| l.psnr < PSNR_SAME && l.size[0].min(l.size[1]) >= 4)
    {
        flags.add(
            "differs from the source (a level)",
            "warn",
            "converter",
            format!("level {} ({} x {}): {} dB", b.level, b.size[0], b.size[1], fs(b.psnr)),
        );
    }
    Ok(())
}
