//! Project OPFOR's test run, its logic (round OR5): the port of `src\tools\testrun\testrun.py`'s `opfor.ini` editing
//! and its reading of `opfor.log` while the game runs (`bc2_native.testrun_ini_*`, `testrun_watch`). The run itself -
//! starting OPFOR, polling for the process, closing it, the crash reader - stays OPFOR's thin Python: it drives the
//! game, which the suite never starts (rust/BENCHMARKS.md "OR5").
//!
//! `opfor.ini` is edited as the DLL reads it: sections and keys matched without case; an existing key keeps its
//! spelling, the file its line endings (`\r\n` when it has one). The patterns are Python's (`\s` Unicode whitespace,
//! `re.I`); `strip()` is Python's.

#![allow(
    clippy::indexing_slicing,
    reason = "each index lies inside the line list the section search bounded, as the Python's"
)]

use std::sync::LazyLock;

use regex::Regex;

#[allow(clippy::expect_used, reason = "constant patterns, each compiled by the tests")]
fn re(p: &str) -> Regex {
    Regex::new(p).expect("a constant pattern")
}

/// `str.strip()`.
fn strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

/// `str.isspace()` of one character (Python's whitespace: Unicode `White_Space` plus the separators 0x1C-0x1F).
fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

static SECTION: LazyLock<Regex> = LazyLock::new(|| re(r"^[\s\x1C-\x1F]*\[([^\]]+)\]"));

/// `_section(lines, section)`: (the header line's index, the end index) of `[section]`, or (None, the line count).
fn section(lines: &[String], name: &str) -> (Option<usize>, usize) {
    let name = strip(name).to_lowercase();
    let (mut current, mut start, mut end) = (None::<String>, None, lines.len());
    for (i, l) in lines.iter().enumerate() {
        if let Some(m) = SECTION.captures(l) {
            if current.as_deref() == Some(name.as_str()) && start.is_some() {
                end = i;
                break;
            }
            let c = strip(&m[1]).to_lowercase();
            if c == name {
                start = Some(i);
            }
            current = Some(c);
        }
    }
    (start, end)
}

/// `_key(line, key)`: the key's own spelling when the line is `key=...` (without case), else None.
fn key_of(line: &str, key: &str) -> Option<String> {
    let r = Regex::new(&format!(r"(?i)^[\s\x1C-\x1F]*({})[\s\x1C-\x1F]*=", regex::escape(key))).ok()?;
    r.captures(line).map(|c| c[1].to_owned())
}

fn split_lines(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_owned).collect()
}

/// `ini_set(text, section, key, value)`: `key=value` in `[section]` (added when missing).
#[must_use]
pub fn ini_set(text: &str, sec: &str, key: &str, value: &str) -> String {
    let cr = if text.contains("\r\n") { "\r" } else { "" };
    let mut lines = split_lines(text);
    let (start, end) = section(&lines, sec);
    let Some(start) = start else {
        return format!(
            "{}{cr}\n{cr}\n[{sec}]{cr}\n{key}={value}{cr}\n",
            text.trim_end_matches(['\r', '\n'])
        );
    };
    for i in start + 1..end {
        if let Some(name) = key_of(&lines[i], key) {
            lines[i] = format!("{name}={value}{cr}");
            return lines.join("\n");
        }
    }
    lines.insert(start + 1, format!("{key}={value}{cr}"));
    lines.join("\n")
}

/// `ini_get(text, section, key)`: the key's value in `[section]`, or None.
#[must_use]
pub fn ini_get(text: &str, sec: &str, key: &str) -> Option<String> {
    let lines = split_lines(text);
    let (start, end) = section(&lines, sec);
    let start = start?;
    for l in &lines[start + 1..end] {
        if key_of(l, key).is_some() {
            let v = l.split_once('=').map_or("", |(_, v)| v);
            return Some(strip(v.trim_end_matches('\r')).to_owned());
        }
    }
    None
}

/// `ini_unset(text, section, key, drop_empty_section)`: the key's line removed; the section header too (and the
/// blank line `ini_set` put before it) when asked and nothing is left in it.
#[must_use]
pub fn ini_unset(text: &str, sec: &str, key: &str, drop_empty_section: bool) -> String {
    let lines = split_lines(text);
    let (start, end) = section(&lines, sec);
    let Some(start) = start else {
        return text.to_owned();
    };
    let mut lines: Vec<String> = lines
        .into_iter()
        .enumerate()
        .filter(|(i, l)| !(start < *i && *i < end && key_of(l, key).is_some()))
        .map(|(_, l)| l)
        .collect();
    let (start, end) = section(&lines, sec);
    let Some(mut start) = start else {
        return lines.join("\n");
    };
    if drop_empty_section && lines[start + 1..end].iter().all(|l| strip(l).is_empty()) {
        while start > 0 && strip(&lines[start - 1]).is_empty() {
            start -= 1;
        }
        let follows = end < lines.len();
        lines.drain(start..end);
        if follows && start > 0 {
            lines.insert(start, if text.contains("\r\n") { "\r" } else { "" }.to_owned());
        }
        if lines.last().is_some_and(|l| !l.is_empty()) {
            lines.push(String::new());
        }
    }
    lines.join("\n")
}

/// `ini_restore(text, original, changed)`: only the keys a run changed put back as `original` has them (removed
/// where it has none; their section too when `original` has no such section).
#[must_use]
pub fn ini_restore(text: &str, original: &str, changed: &[(String, String)]) -> String {
    let mut text = text.to_owned();
    for (sec, key) in changed {
        match ini_get(original, sec, key) {
            None => {
                let had = section(&split_lines(original), sec).0.is_some();
                text = ini_unset(&text, sec, key, !had);
            }
            Some(was) => text = ini_set(&text, sec, key, &was),
        }
    }
    text
}

// ------------------------------------------------------------------------------------------------ the log

/// The AI's milestones in `opfor.log`: (pattern, what the timeline says), each said once.
pub const EVENTS: [(&str, &str); 6] = [
    (r"AI: server level load", "server level load"),
    (r"AI: init", "AI init"),
    (r"AI: set active", "AI active on the level"),
    (r"AI: path data world", "AI path data loaded"),
    (r"AI: Kynapse world", "AI navigation world built"),
    (r"AI: update \(per frame\)", "AI running"),
];

/// What one log line says, in the order `watch` says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// The session state changed (a repeat is no note).
    State(i64),
    /// An AI milestone, the first time.
    Event(&'static str),
    /// A level fix's line (after `levelfix:`, stripped, at most 150 characters).
    LevelFix(String),
    /// A message box, fault or exception (the line from its 16th character, at most 160).
    Fault(String),
    /// The game is writing a crash report: the outcome is known.
    Crash,
}

static STATE: LazyLock<Regex> = LazyLock::new(|| re(r"Session::updateState.*arg1=([0-9A-F]{8})"));
static FAULT: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)message box:|FAULT|exception"));
static EVENT_RES: LazyLock<Vec<Regex>> = LazyLock::new(|| EVENTS.iter().map(|(p, _)| re(p)).collect());

fn chars(s: &str, from: usize, n: usize) -> String {
    s.chars().skip(from).take(n).collect()
}

/// The log's reading state between polls: the session state and the milestones said.
#[derive(Debug, Clone, Default)]
pub struct Watch {
    /// The last session state.
    pub state: Option<i64>,
    /// The milestones said.
    pub seen: Vec<&'static str>,
}

impl Watch {
    /// The notes of `lines` (whole lines of the log, without their `\n`), stopping after a crash note.
    pub fn lines<'a>(&mut self, lines: impl IntoIterator<Item = &'a str>) -> Vec<Note> {
        let mut out = Vec::new();
        for l in lines {
            if let Some(m) = STATE.captures(l) {
                if let Ok(s) = i64::from_str_radix(&m[1], 16)
                    && self.state != Some(s)
                {
                    self.state = Some(s);
                    out.push(Note::State(s));
                }
                continue;
            }
            for (rx, (_, label)) in EVENT_RES.iter().zip(EVENTS) {
                if !self.seen.contains(&label) && rx.is_match(l) {
                    self.seen.push(label);
                    out.push(Note::Event(label));
                }
            }
            if let Some((_, rest)) = l.split_once("levelfix:")
                && !l.contains("stack code addresses")
            {
                out.push(Note::LevelFix(chars(strip(rest), 0, 150)));
            }
            if FAULT.is_match(l) {
                out.push(Note::Fault(chars(l, 15, 160)));
            }
            if l.contains("crashreport.xml") {
                out.push(Note::Crash);
                return out;
            }
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn ini_edits() {
        let t = "[DEV]\r\nlevel=a\r\n\r\n[x]\r\ny=1\r\n";
        assert_eq!(ini_get(t, "dev", "Level").as_deref(), Some("a"));
        assert_eq!(
            ini_set(t, "dev", "LEVEL", "b"),
            "[DEV]\r\nlevel=b\r\n\r\n[x]\r\ny=1\r\n"
        );
        assert_eq!(ini_set("", "dev", "k", "v"), "\n\n[dev]\nk=v\n");
        let added = ini_set(t, "new", "k", "v");
        assert_eq!(added, "[DEV]\r\nlevel=a\r\n\r\n[x]\r\ny=1\r\n\r\n[new]\r\nk=v\r\n");
        assert_eq!(ini_unset(&added, "new", "k", true), t);
        let changed = [
            ("new".to_owned(), "k".to_owned()),
            ("dev".to_owned(), "level".to_owned()),
        ];
        assert_eq!(ini_restore(&ini_set(&added, "dev", "level", "z"), t, &changed), t);
    }

    #[test]
    fn notes() {
        let mut w = Watch::default();
        let n = w.lines([
            "x Session::updateState arg1=00000002 arg1=00000003",
            "AI: init",
            "AI: init",
            "12:00:00.000 [l] levelfix:  moved 3 ",
            "12:00:00.000 [l] Exception caught",
            "crashreport.xml written",
            "AI: set active",
        ]);
        assert_eq!(
            n,
            vec![
                Note::State(3),
                Note::Event("AI init"),
                Note::LevelFix("moved 3".into()),
                Note::Fault("] Exception caught".into()),
                Note::Crash
            ]
        );
    }
}
