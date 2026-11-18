//! The opfor.log readers: the harness's sessions (`testharness.ingame.sessions`, `sessions_reference`'s rules) and
//! the crash reader's lines before a crash (`crashreader.log_before_crash`).
//!
//! The text is split into lines as Python's `str.splitlines` splits it (`\n`, `\r`, `\r\n`, VT, FF, the file / group
//! / record separators, U+0085, U+2028, U+2029). Only the lines holding one of the literals a session line needs
//! ([`LITERALS`]) are read, on all cores; the others are counted. The patterns are Python's with `[0-9]` for `\d`:
//! a line read here that holds a digit outside ASCII is refused ([`Refused`]) and the caller runs the Python, whose
//! `\d` and `int()` take Unicode digits.

use aho_corasick::AhoCorasick;
use rayon::prelude::*;
use regex::Regex;
use std::sync::LazyLock;

/// The Python reference decides: a line or an input the port does not read itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused(pub &'static str);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not read here: {}", self.0)
    }
}

impl std::error::Error for Refused {}

/// One session of an opfor.log (`ingame.Session`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    /// The process id, as written.
    pub pid: String,
    /// The time of day of its start line (`hh:mm:ss`), or empty.
    pub start: String,
    /// The level it hosted.
    pub level: Option<String>,
    /// Session states, a repeat left out.
    pub states: Vec<i64>,
    /// The AI events seen, first time each.
    pub events: Vec<&'static str>,
    /// `SectorOffsets` loads (X, Z).
    pub offset_loads: Vec<(i64, i64)>,
    /// The `[dev]` override (X, Z).
    pub override_: Option<(i64, i64)>,
    /// Unit-counter pathfinder counts.
    pub pathfinders: Vec<i64>,
    /// A crash report was written.
    pub crashed: bool,
    /// `exit(0)` was called.
    pub exited: bool,
    /// The lines after its start line.
    pub lines: i64,
}

/// The literals every line a session reads holds (`ingame.LITERALS`).
pub const LITERALS: [&str; 13] = [
    "opfor.dll loaded into PID ",
    "level='",
    "Session::updateState",
    "sector offsets",
    "unit counters (",
    "AI: server level load",
    "AI: init",
    "AI: set active",
    "AI: path data world",
    "AI: Kynapse world",
    "AI: update (per frame)",
    "crashreport.xml",
    "exit(0) called",
];

/// `ingame.EVENTS`: (literal, label).
const EVENTS: [(&str, &str); 6] = [
    ("AI: server level load", "server level load"),
    ("AI: init", "AI init"),
    ("AI: set active", "AI active"),
    ("AI: path data world", "path data loaded"),
    ("AI: Kynapse world", "navigation world built"),
    ("AI: update (per frame)", "AI running"),
];

#[allow(clippy::expect_used)]
fn re(p: &str) -> Regex {
    Regex::new(p).expect("a fixed pattern")
}

static START: LazyLock<Regex> = LazyLock::new(|| re(r"---- opfor\.dll loaded into PID ([0-9]+) ----"));
static STAMP: LazyLock<Regex> = LazyLock::new(|| re(r"^\[([0-9][0-9]:[0-9][0-9]:[0-9][0-9])\.[0-9]+\]"));
static LEVEL: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:queued hostMultiplayerLocal request|dev trigger): level='([^']+)'"));
static STATE: LazyLock<Regex> = LazyLock::new(|| re(r"Session::updateState.*arg1=([0-9A-F]{8})"));
static OFFSET_LOAD: LazyLock<Regex> =
    LazyLock::new(|| re(r"sector offsets load handle .*X\(\+1Ch\)=(-?[0-9]+) Z\(\+18h\)=(-?[0-9]+)"));
static OVERRIDE: LazyLock<Regex> = LazyLock::new(|| re(r"sector offsets: on - X=(-?[0-9]+) Z=(-?[0-9]+)"));
static COUNTERS: LazyLock<Regex> = LazyLock::new(|| re(r"unit counters \((interval|exit)\): ([0-9]+) pathfinders"));
#[allow(clippy::expect_used)]
static HITS: LazyLock<AhoCorasick> = LazyLock::new(|| AhoCorasick::new(LITERALS).expect("fixed literals"));

/// What one line says (a session start, or facts about the current session).
#[derive(Debug, Clone, Default)]
struct LineFacts {
    start: Option<(String, String)>,
    level: Option<String>,
    state: Option<i64>,
    events: Vec<&'static str>,
    offset_load: Option<(i64, i64)>,
    override_: Option<(i64, i64)>,
    pathfinders: Option<i64>,
    crashed: bool,
    exited: bool,
}

/// A Python `int()` of an ASCII decimal with an optional minus (the patterns only match those).
fn int(s: &str) -> Result<i64, Refused> {
    s.parse::<i64>().map_err(|_| Refused("a number past 64 bits"))
}

fn group<'a>(m: &regex::Captures<'a>, k: usize) -> &'a str {
    m.get(k).map_or("", |g| g.as_str())
}

fn facts(line: &str) -> Result<LineFacts, Refused> {
    if !line.is_ascii() && line.chars().any(|c| !c.is_ascii() && c.is_numeric()) {
        return Err(Refused("a digit outside ASCII on a session line"));
    }
    let mut f = LineFacts::default();
    if let Some(m) = START.captures(line) {
        let stamp = STAMP.captures(line).map_or(String::new(), |t| group(&t, 1).to_string());
        f.start = Some((group(&m, 1).to_string(), stamp));
        return Ok(f);
    }
    if let Some(m) = LEVEL.captures(line) {
        f.level = Some(group(&m, 1).to_string());
    }
    if let Some(m) = STATE.captures(line) {
        f.state = Some(i64::from_str_radix(group(&m, 1), 16).map_err(|_| Refused("a state"))?);
    }
    for (lit, label) in EVENTS {
        if line.contains(lit) {
            f.events.push(label);
        }
    }
    if let Some(m) = OFFSET_LOAD.captures(line) {
        f.offset_load = Some((int(group(&m, 1))?, int(group(&m, 2))?));
    }
    if let Some(m) = OVERRIDE.captures(line) {
        f.override_ = Some((int(group(&m, 1))?, int(group(&m, 2))?));
    }
    if let Some(m) = COUNTERS.captures(line) {
        f.pathfinders = Some(int(group(&m, 2))?);
    }
    f.crashed = line.contains("crashreport.xml");
    f.exited = line.contains("exit(0) called");
    Ok(f)
}

/// The length of the line break at `b[k..]` (0: none), as `str.splitlines` breaks.
#[inline]
fn break_len(b: &[u8], k: usize) -> usize {
    match b.get(k) {
        Some(b'\n' | 0x0B | 0x0C | 0x1C | 0x1D | 0x1E) => 1,
        Some(b'\r') => {
            if b.get(k + 1) == Some(&b'\n') {
                2
            } else {
                1
            }
        }
        Some(0xC2) => usize::from(b.get(k + 1) == Some(&0x85)) * 2,
        Some(0xE2) if b.get(k + 1) == Some(&0x80) && matches!(b.get(k + 2), Some(0xA8 | 0xA9)) => 3,
        _ => 0,
    }
}

/// Bytes that may start a line break.
static BREAK_START: LazyLock<[bool; 256]> = LazyLock::new(|| {
    let mut t = [false; 256];
    for b in [b'\n', b'\r', 0x0B, 0x0C, 0x1C, 0x1D, 0x1E, 0xC2, 0xE2] {
        if let Some(x) = t.get_mut(usize::from(b)) {
            *x = true;
        }
    }
    t
});

/// The lines of a text as `str.splitlines()` gives them, each `(start, end of text, end with its break)`.
fn for_lines(text: &str, mut each: impl FnMut(usize, usize, usize)) {
    let b = text.as_bytes();
    let table = &*BREAK_START;
    let mut start = 0usize;
    let mut k = 0usize;
    while k < b.len() {
        let c = b.get(k).copied().unwrap_or(0);
        if table.get(usize::from(c)).copied().unwrap_or(false) {
            let n = break_len(b, k);
            if n > 0 {
                each(start, k, k + n);
                k += n;
                start = k;
                continue;
            }
        }
        k += 1;
    }
    if start < b.len() {
        each(start, b.len(), b.len());
    }
}

type ChunkFacts = (i64, Vec<(i64, LineFacts)>);

/// One chunk's lines: their count and the facts of the lines that hold a literal (line index within the chunk).
fn chunk_facts(text: &str) -> Result<ChunkFacts, Refused> {
    let hits: Vec<usize> = HITS.find_iter(text).map(|m| m.start()).collect();
    let mut out = Vec::new();
    let mut n: i64 = 0;
    let mut h = 0usize;
    let mut err = None;
    for_lines(text, |s, e, _| {
        while hits.get(h).is_some_and(|&p| p < s) {
            h += 1;
        }
        if hits.get(h).is_some_and(|&p| p < e) {
            match facts(text.get(s..e).unwrap_or_default()) {
                Ok(f) => out.push((n, f)),
                Err(r) => err = Some(r),
            }
        }
        n += 1;
    });
    if let Some(r) = err {
        return Err(r);
    }
    Ok((n, out))
}

/// The text split after newlines into about `parts` pieces (a newline always ends a line, and is never inside a
/// `\r\n` or a multi-byte character).
fn split_chunks(text: &str, parts: usize) -> Vec<&str> {
    let b = text.as_bytes();
    let step = (b.len() / parts.max(1)).max(1 << 20);
    let mut out = Vec::new();
    let mut s = 0usize;
    while s < b.len() {
        let target = s + step;
        if target >= b.len() {
            out.push(text.get(s..).unwrap_or_default());
            break;
        }
        let e = memchr::memchr(b'\n', b.get(target..).unwrap_or_default()).map_or(b.len(), |k| target + k + 1);
        out.push(text.get(s..e).unwrap_or_default());
        s = e;
    }
    out
}

/// `ingame.sessions(text)`.
///
/// # Errors
/// [`Refused`] for a session line the port does not read (a digit outside ASCII, a number past 64 bits).
pub fn sessions(text: &str) -> Result<Vec<Session>, Refused> {
    let chunks = split_chunks(text, rayon::current_num_threads() * 4);
    let parts: Vec<Result<ChunkFacts, Refused>> = chunks.par_iter().map(|c| chunk_facts(c)).collect();
    let mut out: Vec<Session> = Vec::new();
    let mut cur: Option<usize> = None;
    let mut prev: i64 = -1;
    let mut offset: i64 = 0;
    for part in parts {
        let (n, lines) = part?;
        for (k, f) in lines {
            let line = offset + k;
            if let Some(c) = cur.and_then(|c| out.get_mut(c)) {
                c.lines += line - prev - 1;
            }
            prev = line;
            if let Some((pid, start)) = f.start {
                out.push(Session {
                    pid,
                    start,
                    ..Session::default()
                });
                cur = Some(out.len() - 1);
                continue;
            }
            let Some(c) = cur.and_then(|c| out.get_mut(c)) else {
                continue;
            };
            c.lines += 1;
            if f.level.is_some() {
                c.level = f.level;
            }
            if let Some(s) = f.state
                && c.states.last() != Some(&s)
            {
                c.states.push(s);
            }
            for e in f.events {
                if !c.events.contains(&e) {
                    c.events.push(e);
                }
            }
            if let Some(o) = f.offset_load {
                c.offset_loads.push(o);
            }
            if f.override_.is_some() {
                c.override_ = f.override_;
            }
            if let Some(p) = f.pathfinders {
                c.pathfinders.push(p);
            }
            c.crashed |= f.crashed;
            c.exited |= f.exited;
        }
        offset += n;
    }
    if let Some(c) = cur.and_then(|c| out.get_mut(c)) {
        c.lines += offset - prev - 1;
    }
    Ok(out)
}

/// Python's `bytes.decode('utf-8', errors='replace')` (a replacement character for each maximal invalid
/// subsequence, as `String::from_utf8_lossy`).
#[must_use]
pub fn decode_replace(data: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(data)
}

/// `ingame.sessions(open(path, encoding='utf-8', errors='replace').read())`: the file's sessions. Text mode's
/// newline translation changes no line: `str.splitlines` breaks at `\r` and `\r\n` as at `\n`.
///
/// # Errors
/// The file's read error; inside, [`Refused`] as [`sessions`].
pub fn sessions_of_file(path: &std::path::Path) -> std::io::Result<Result<Vec<Session>, Refused>> {
    let data = std::fs::read(path)?;
    Ok(sessions(&decode_replace(&data)))
}

/// The mark of a crash line.
pub const MARK: &[u8] = b"crashreport.xml";
/// `crashreader.BEFORE`: the lines looked at before the crash line.
pub const BEFORE: usize = 400;

/// Python's `seq[-n:]` start index for a list of `len` items.
fn tail_start(len: usize, n: i64) -> usize {
    match n.cmp(&0) {
        std::cmp::Ordering::Greater => len.saturating_sub(usize::try_from(n).unwrap_or(usize::MAX)),
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Less => usize::try_from(n.unsigned_abs()).unwrap_or(usize::MAX).min(len),
    }
}

/// `crashreader._near(when)`'s test of a crash line: within 2 minutes of `want` (seconds of the day), or no time on
/// the line; `want` None: every line passes.
fn near(line: &str, want: Option<i64>) -> Result<bool, Refused> {
    let Some(want) = want else { return Ok(true) };
    let c: Vec<char> = line.chars().take(9).collect();
    if c.first() != Some(&'[') {
        return Ok(true);
    }
    // \[(\d\d):(\d\d):(\d\d) with ASCII digits; a Unicode digit there is the reference's to read
    for k in [1usize, 2, 4, 5, 7, 8] {
        if let Some(&ch) = c.get(k)
            && !ch.is_ascii()
            && ch.is_numeric()
        {
            return Err(Refused("a digit outside ASCII in a crash line's time"));
        }
    }
    let digit = |k: usize| c.get(k).filter(|ch| ch.is_ascii_digit()).and_then(|ch| ch.to_digit(10));
    let colon = |k: usize| c.get(k) == Some(&':');
    let (Some(h1), Some(h2), Some(m1), Some(m2), Some(s1), Some(s2)) =
        (digit(1), digit(2), digit(4), digit(5), digit(7), digit(8))
    else {
        return Ok(true);
    };
    if !colon(3) || !colon(6) {
        return Ok(true);
    }
    let t = i64::from(h1 * 10 + h2) * 3600 + i64::from(m1 * 10 + m2) * 60 + i64::from(s1 * 10 + s2);
    let d = (t - want).abs();
    Ok(d.min(86400 - d) <= 120)
}

/// `crashreader.log_before_crash(path, lines, when)` on the log's bytes, `want` the report's time of day in seconds
/// (`when` given) or `None`: the lines before the latest crash line that matches, spam left out, and the crash line
/// itself; empty when none matches. Only the bytes around the crash lines are decoded, as the reference does.
///
/// # Errors
/// [`Refused`] where the reference reads a Unicode digit in a crash line's time.
pub fn log_before_crash(data: &[u8], lines: i64, want: Option<i64>) -> Result<Vec<String>, Refused> {
    let finder = memchr::memmem::FinderRev::new(MARK);
    let mut q = finder.rfind(data);
    while let Some(qq) = q {
        // back past BEFORE + 1 whole lines (and the mark line's own start)
        let mut s: Option<usize> = Some(qq);
        for _ in 0..BEFORE + 2 {
            s = s.and_then(|x| memchr::memrchr(b'\n', data.get(..x).unwrap_or_default()));
            if s.is_none() {
                break;
            }
        }
        let s = s.map_or(0, |x| x + 1);
        let e = memchr::memchr(b'\n', data.get(qq..).unwrap_or_default()).map_or(data.len(), |k| qq + k + 1);
        let window = decode_replace(data.get(s..e).unwrap_or_default());
        let at = decode_replace(data.get(s..qq).unwrap_or_default()).len();
        let mut text: Vec<&str> = Vec::new();
        let mut end: Option<usize> = None;
        for_lines(&window, |a, b, with_break| {
            if end.is_none() && with_break > at {
                end = Some(text.len());
            }
            text.push(window.get(a..b).unwrap_or_default());
        });
        let Some(end) = end else {
            return Err(Refused("no crash line in the window"));
        };
        let Some(&crash) = text.get(end) else {
            return Err(Refused("no crash line in the window"));
        };
        if near(crash, want)? {
            let from = end.saturating_sub(BEFORE);
            let picked: Vec<&str> = text
                .get(from..end)
                .unwrap_or_default()
                .iter()
                .copied()
                .filter(|l| !l.contains("Kynapse error") && !l.contains("per frame"))
                .collect();
            let start = tail_start(picked.len(), lines);
            let mut out: Vec<String> = picked
                .get(start..)
                .unwrap_or_default()
                .iter()
                .map(|s| (*s).to_string())
                .collect();
            out.push(crash.to_string());
            return Ok(out);
        }
        q = finder.rfind(data.get(..qq).unwrap_or_default());
    }
    Ok(Vec::new())
}
