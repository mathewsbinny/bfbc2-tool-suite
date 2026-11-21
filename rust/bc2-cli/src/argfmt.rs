//! `argparse`'s help and usage texts at the console's width (round RT2): Python 3.13's `HelpFormatter` (and
//! `RawDescriptionHelpFormatter`'s descriptions) and `textwrap`'s line filling, over the width-independent pieces of
//! each parser that `gen_argspecs.py` takes from the Python parsers ([`crate::argspecs`]).
//!
//! The width is `shutil.get_terminal_size().columns - 2`, as `HelpFormatter` takes it: `COLUMNS` when it holds a
//! positive whole number, else the width of the console window stdout writes to, else 80 ([`width`]). Before this
//! round only the pipe's width (80) was ported, and in a console (or with `COLUMNS` set) the tools' `--help` and usage
//! errors were handed to the Python. Proof: `core/tests/test_argfmt.py` (every parser's help and usage at many widths
//! against the Python's own, in process; the command lines' help and usage errors as processes, with `COLUMNS`).

#![allow(
    clippy::indexing_slicing,
    reason = "character vectors sliced within their lengths (textwrap's long words), a non-empty slice's first and last"
)]

use std::borrow::Cow;

use crate::argspecs::SPECS;
use crate::pyproc::{Fail, PyResult};

/// A description or epilog: as written (`RawDescriptionHelpFormatter`) or filled (its words as `textwrap` splits
/// them, the whitespace already collapsed).
pub enum Text {
    /// Kept as written, each line indented.
    Raw(&'static str),
    /// Filled to the width.
    Fill(&'static [&'static str]),
}

/// An action's help.
pub enum Help {
    /// `help` is `None` or empty: the invocation alone on its line.
    Missing,
    /// `help` holds only whitespace: the invocation, padded, then the line's end.
    #[allow(
        dead_code,
        reason = "argparse's rule for it is kept; no parser of these tools has such a help today"
    )]
    Blank,
    /// The expanded help's words (as `textwrap` splits them), wrapped in the help column.
    Chunks(&'static [&'static str]),
}

/// One action's entry in the help: its invocation (`-o OUT, --out OUT`), its help, its subactions (a subcommand
/// action's choices with help).
pub struct Item {
    /// `_format_action_invocation`'s text.
    pub invocation: &'static str,
    /// The help.
    pub help: Help,
    /// The subactions, one indent further in.
    pub subs: &'static [Item],
}

/// An argument group: its title, description and actions.
pub struct Section {
    /// The heading (`positional arguments`, `options`, a group's title).
    pub heading: Option<&'static str>,
    /// The group's description.
    pub text: Option<Text>,
    /// The group's actions (those whose help is not suppressed).
    pub items: &'static [Item],
}

/// A parser's pieces.
pub struct Spec {
    /// The tool, then the subcommand (`"shaderdb info"`).
    pub key: &'static str,
    /// `parser.prog`.
    pub prog: &'static str,
    /// The usage on one line (prog and every part).
    pub usage: &'static str,
    /// The optionals' usage parts (mutually exclusive groups' brackets and bars included).
    pub opt_parts: &'static [&'static str],
    /// The positionals' usage parts.
    pub pos_parts: &'static [&'static str],
    /// The description.
    pub description: Option<Text>,
    /// The argument groups in order.
    pub sections: &'static [Section],
    /// The epilog.
    pub epilog: Option<Text>,
}

/// The parser `key` (a tool, or `"tool subcommand"`).
pub fn spec(key: &str) -> PyResult<&'static Spec> {
    SPECS.iter().find(|s| s.key == key).ok_or(Fail::Python)
}

fn len(s: &str) -> i64 {
    s.chars().count() as i64
}

fn pad(n: i64) -> String {
    " ".repeat(usize::try_from(n).unwrap_or(0))
}

/// `s[n:]` by characters.
fn from(s: &str, n: i64) -> String {
    s.chars().skip(usize::try_from(n).unwrap_or(0)).collect()
}

/// Python's `int(text)` of an environment variable as `shutil.get_terminal_size` reads it: `Ok(Some(v))`,
/// `Ok(None)` when `int()` raises (not a number: the variable counts as unset), `Err` for text only Python reads
/// (other than ASCII, or past 18 digits).
fn py_int(text: &str) -> Result<Option<i64>, ()> {
    if !text.is_ascii() {
        return Err(());
    }
    let t = text.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1f'));
    let (neg, body) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let b = body.as_bytes();
    if b.is_empty() || b[0] == b'_' || b[b.len() - 1] == b'_' || body.contains("__") {
        return Ok(None);
    }
    if !b.iter().all(|c| c.is_ascii_digit() || *c == b'_') {
        return Ok(None);
    }
    let digits: String = body.chars().filter(char::is_ascii_digit).collect();
    if digits.trim_start_matches('0').len() > 18 {
        return Err(());
    }
    let v: i64 = digits.parse().map_err(|_| ())?;
    Ok(Some(if neg { -v } else { v }))
}

/// The console window's width (columns) that stdout writes to, as `os.get_terminal_size(1)` reads it, or `None`.
#[cfg(windows)]
#[allow(unsafe_code)]
fn console_columns() -> Option<i64> {
    use windows_sys::Win32::System::Console::{
        CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo, GetStdHandle, STD_OUTPUT_HANDLE,
    };
    // SAFETY: GetStdHandle returns the process's standard output handle (or null / invalid, which the next call
    // rejects); GetConsoleScreenBufferInfo writes one CONSOLE_SCREEN_BUFFER_INFO it is given and fails (0) for a
    // handle that is not a console.
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(h, &raw mut info) == 0 {
            return None;
        }
        Some(i64::from(info.srWindow.Right) - i64::from(info.srWindow.Left) + 1)
    }
}

#[cfg(not(windows))]
fn console_columns() -> Option<i64> {
    None
}

/// `HelpFormatter`'s width: `shutil.get_terminal_size().columns - 2` (`COLUMNS` when a positive whole number, else
/// the console's columns, else 80). [`Fail::Python`] when `COLUMNS` holds text only Python's `int()` reads.
pub fn width() -> PyResult<i64> {
    let env = match std::env::var_os("COLUMNS") {
        None => None,
        Some(v) => match v.to_str() {
            Some(s) => py_int(s).map_err(|()| Fail::Python)?,
            None => return Err(Fail::Python),
        },
    };
    let columns = match env {
        Some(c) if c > 0 => c,
        _ => match console_columns() {
            Some(c) if c != 0 => c,
            _ => 80,
        },
    };
    Ok(columns - 2)
}

/// `TextWrapper._wrap_chunks` (break long words and on hyphens, drop whitespace, no line limit) of `chunks` at
/// `width` with the indents.
fn wrap(chunks: &[&str], width: i64, initial: &str, subsequent: &str) -> Vec<String> {
    // str.strip()'s whitespace: Unicode's, and the separators 0x1C - 0x1F
    let blank = |s: &str| s.chars().all(|c| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'));
    let mut rest: Vec<String> = chunks.iter().rev().map(|c| (*c).to_owned()).collect();
    let mut lines: Vec<String> = Vec::new();
    while !rest.is_empty() {
        let mut cur: Vec<String> = Vec::new();
        let mut cur_len = 0i64;
        let indent = if lines.is_empty() { initial } else { subsequent };
        let w = width - len(indent);
        if !lines.is_empty() && rest.last().is_some_and(|c| blank(c)) {
            rest.pop();
        }
        while let Some(c) = rest.last() {
            let l = len(c);
            if cur_len + l <= w {
                cur_len += l;
                cur.extend(rest.pop());
            } else {
                break;
            }
        }
        if rest.last().is_some_and(|c| len(c) > w) {
            // _handle_long_word
            let space_left = if w < 1 { 1 } else { w - cur_len };
            if space_left > 0 {
                if let Some(chunk) = rest.last_mut() {
                    let cs: Vec<char> = chunk.chars().collect();
                    let mut end = space_left;
                    if len(chunk) > space_left {
                        let upto = usize::try_from(space_left).unwrap_or(0).min(cs.len());
                        if let Some(h) = cs[..upto].iter().rposition(|&c| c == '-')
                            && h > 0
                            && cs[..h].iter().any(|&c| c != '-')
                        {
                            end = h as i64 + 1;
                        }
                    }
                    let e = usize::try_from(end).unwrap_or(0).min(cs.len());
                    cur.push(cs[..e].iter().collect());
                    *chunk = cs[e..].iter().collect();
                }
            } else if cur.is_empty() {
                cur.extend(rest.pop());
            }
            cur_len = cur.iter().map(|c| len(c)).sum();
        }
        if cur.last().is_some_and(|c| blank(c))
            && let Some(c) = cur.pop()
        {
            cur_len -= len(&c);
        }
        let _ = cur_len;
        if !cur.is_empty() {
            lines.push(format!("{indent}{}", cur.concat()));
        }
    }
    lines
}

/// The formatter's state for one text: its width and help column.
struct Fmt {
    width: i64,
    max_help_position: i64,
    action_max: i64,
}

impl Fmt {
    fn new(width: i64) -> Self {
        Self {
            width,
            max_help_position: 24.min((width - 20).max(4)),
            action_max: 0,
        }
    }

    /// `_format_usage(None, actions, groups, prefix)` at indent 0, without its trailing blank line.
    fn usage(&self, s: &Spec, prefix: &str) -> String {
        let text_width = self.width;
        if len(prefix) + len(s.usage) <= text_width {
            return format!("{prefix}{}", s.usage);
        }
        let get_lines = |parts: &[&str], indent: &str, prefix: Option<&str>| -> Vec<String> {
            let mut lines: Vec<String> = Vec::new();
            let mut line: Vec<&str> = Vec::new();
            let indent_length = len(indent);
            let mut line_len = prefix.map_or(indent_length - 1, |p| len(p) - 1);
            for &part in parts {
                if line_len + 1 + len(part) > text_width && !line.is_empty() {
                    lines.push(format!("{indent}{}", line.join(" ")));
                    line.clear();
                    line_len = indent_length - 1;
                }
                line.push(part);
                line_len += len(part) + 1;
            }
            if !line.is_empty() {
                lines.push(format!("{indent}{}", line.join(" ")));
            }
            if prefix.is_some()
                && let Some(first) = lines.first_mut()
            {
                *first = from(first, indent_length);
            }
            lines
        };
        let prog = s.prog;
        // len(prefix) + len(prog) <= 0.75 * text_width, exactly (both sides times 4)
        let lines: Vec<String> = if 4 * (len(prefix) + len(prog)) <= 3 * text_width {
            let indent = pad(len(prefix) + len(prog) + 1);
            let with_prog = |parts: &[&'static str]| {
                let mut v = vec![prog];
                v.extend_from_slice(parts);
                v
            };
            if !s.opt_parts.is_empty() {
                let mut l = get_lines(&with_prog(s.opt_parts), &indent, Some(prefix));
                l.extend(get_lines(s.pos_parts, &indent, None));
                l
            } else if !s.pos_parts.is_empty() {
                get_lines(&with_prog(s.pos_parts), &indent, Some(prefix))
            } else {
                vec![prog.to_owned()]
            }
        } else {
            let indent = pad(len(prefix));
            let all: Vec<&str> = s.opt_parts.iter().chain(s.pos_parts).copied().collect();
            let mut l = get_lines(&all, &indent, None);
            if l.len() > 1 {
                l = get_lines(s.opt_parts, &indent, None);
                l.extend(get_lines(s.pos_parts, &indent, None));
            }
            let mut v = vec![prog.to_owned()];
            v.extend(l);
            v
        };
        format!("{prefix}{}", lines.join("\n"))
    }

    /// `_format_text` at `indent`.
    fn text(&self, t: &Text, indent: i64) -> String {
        let text_width = (self.width - indent).max(11);
        let ind = pad(indent);
        let body = match t {
            Text::Raw(s) => {
                let mut out = String::new();
                for line in s.split_inclusive('\n') {
                    out.push_str(&ind);
                    out.push_str(line);
                }
                out
            }
            Text::Fill(chunks) => wrap(chunks, text_width, &ind, &ind).join("\n"),
        };
        format!("{body}\n\n")
    }

    fn measure(&mut self, it: &Item, indent: i64) {
        self.action_max = self.action_max.max(len(it.invocation) + indent);
        for s in it.subs {
            self.measure(s, indent + 2);
        }
    }

    /// `_format_action` at `indent`.
    fn action(&self, it: &Item, indent: i64) -> String {
        let help_position = (self.action_max + 2).min(self.max_help_position);
        let help_width = (self.width - help_position).max(11);
        let action_width = help_position - indent - 2;
        let mut parts: Vec<String> = Vec::new();
        let mut indent_first = 0;
        let header = if matches!(it.help, Help::Missing) {
            format!("{}{}\n", pad(indent), it.invocation)
        } else if len(it.invocation) <= action_width {
            let fill = pad(action_width - len(it.invocation));
            format!("{}{}{fill}  ", pad(indent), it.invocation)
        } else {
            indent_first = help_position;
            format!("{}{}\n", pad(indent), it.invocation)
        };
        let ends = header.ends_with('\n');
        parts.push(header);
        match it.help {
            Help::Chunks(chunks) if !chunks.is_empty() => {
                let lines = wrap(chunks, help_width, "", "");
                for (k, line) in lines.iter().enumerate() {
                    let at = if k == 0 { indent_first } else { help_position };
                    parts.push(format!("{}{line}\n", pad(at)));
                }
            }
            Help::Blank if !ends => parts.push("\n".into()),
            _ => {}
        }
        for s in it.subs {
            parts.push(self.action(s, indent + 2));
        }
        parts.concat()
    }

    /// A section (`_Section.format_help`) at indent 0 (its items at 2).
    fn section(&self, sec: &Section) -> String {
        let mut items = String::new();
        if let Some(t) = &sec.text {
            items.push_str(&self.text(t, 2));
        }
        for it in sec.items {
            items.push_str(&self.action(it, 2));
        }
        if items.is_empty() {
            return String::new();
        }
        let heading = sec.heading.map_or_default(|h| format!("{h}:\n"));
        format!("\n{heading}{items}\n")
    }
}

/// `format_help()`'s last steps: runs of three or more line ends made two, line ends stripped, one added.
fn finish(help: &str) -> String {
    let mut out = String::with_capacity(help.len());
    let mut run = 0usize;
    for c in help.chars() {
        if c == '\n' {
            run += 1;
            continue;
        }
        out.push_str(if run >= 3 { "\n\n" } else { &"\n\n"[..run] });
        run = 0;
        out.push(c);
    }
    out.push_str(if run >= 3 { "\n\n" } else { &"\n\n"[..run] });
    format!("{}\n", out.trim_matches('\n'))
}

/// `parser.format_usage()` of `s` at `width`: `usage: ...` and the line's end.
pub fn usage_at(s: &Spec, width: i64) -> String {
    finish(&format!("{}\n\n", Fmt::new(width).usage(s, "usage: ")))
}

/// `parser.format_help()` of `s` at `width`.
pub fn help_at(s: &Spec, width: i64) -> String {
    let mut f = Fmt::new(width);
    for sec in s.sections {
        for it in sec.items {
            f.measure(it, 2);
        }
    }
    let mut items = format!("{}\n\n", f.usage(s, "usage: "));
    if let Some(t) = &s.description {
        items.push_str(&f.text(t, 0));
    }
    for sec in s.sections {
        items.push_str(&f.section(sec));
    }
    if let Some(t) = &s.epilog {
        items.push_str(&f.text(t, 0));
    }
    finish(&format!("\n{items}\n"))
}

/// A parser's key from its `prog`: the tool (`shaderdb.py` -> `shaderdb`, `python -m bfbc2core.caches` -> `caches`),
/// then its subcommand.
fn key_of(prog: &str) -> String {
    if prog == "python -m bfbc2core.caches" {
        return "caches".into();
    }
    let (tool, sub) = prog.split_once(' ').unwrap_or((prog, ""));
    let tool = tool.strip_suffix(".py").unwrap_or(tool);
    if sub.is_empty() {
        tool.to_owned()
    } else {
        format!("{tool} {sub}")
    }
}

/// The pipe's width (80 columns): the texts the ports recorded from the Python are printed as they are.
const PIPE: i64 = 78;

/// The help of parser `key` at the console's width (a parser whose 80-column text was not recorded).
pub fn help(key: &str) -> PyResult<String> {
    Ok(help_at(spec(key)?, width()?))
}

/// The usage of the parser whose `prog` is given at the console's width (what `parser.error` prints before its
/// message): `at80`, the recorded text, at the pipe's width.
pub fn usage_or<'a>(prog: &str, at80: &'a str) -> PyResult<Cow<'a, str>> {
    let w = width()?;
    Ok(if w == PIPE {
        Cow::Borrowed(at80)
    } else {
        Cow::Owned(usage_at(spec(&key_of(prog))?, w))
    })
}

/// The help of the parser whose `prog` is given at the console's width (`at80` at the pipe's).
pub fn help_or<'a>(prog: &str, at80: &'a str) -> PyResult<Cow<'a, str>> {
    let w = width()?;
    Ok(if w == PIPE {
        Cow::Borrowed(at80)
    } else {
        Cow::Owned(help_at(spec(&key_of(prog))?, w))
    })
}

/// What `argparse` (Python 3.13, `allow_abbrev=True`) makes of an argument that starts with `--` and is not `--`
/// alone (round RT3: option abbreviations answered here instead of in the Python).
#[derive(Debug, PartialEq, Eq)]
pub enum Long {
    /// the option string it names: exact, the part before `=` exact, or the one option string the part before `=`
    /// starts
    Found(&'static str),
    /// two or more option strings start with it: `consume_optional`'s error, `ambiguous option: <arg> could match
    /// <them>` (the whole argument as typed)
    Ambiguous(String),
    /// no option of the parser: an extra argument (`unrecognized arguments`)
    Unknown,
    /// no option, and it holds a space: argparse takes it for a positional value (the ports leave that to it)
    Spaced,
}

/// `_parse_optional` / `_get_option_tuples` of a `--` argument against the parser's option strings in its
/// `_option_string_actions` order (`-h`, `--help` first, then each `add_argument`'s strings in order). The
/// ambiguity is argparse's when the option is consumed (`consume_optional`), so in order with the other errors.
pub fn long_option(arg: &str, strings: &[&'static str]) -> Long {
    if let Some(s) = strings.iter().find(|s| **s == arg) {
        return Long::Found(s);
    }
    let prefix = arg.split_once('=').map_or(arg, |(p, _)| p);
    if let Some(s) = strings.iter().find(|s| **s == prefix) {
        return Long::Found(s);
    }
    let hits: Vec<&'static str> = strings.iter().copied().filter(|s| s.starts_with(prefix)).collect();
    match hits.as_slice() {
        [] if arg.contains(' ') => Long::Spaced,
        [] => Long::Unknown,
        [one] => Long::Found(one),
        many => Long::Ambiguous(format!("ambiguous option: {arg} could match {}", many.join(", "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviations_as_argparse() {
        let s = ["-h", "--help", "--prune", "--prune-targets", "--opfor"];
        assert_eq!(long_option("--prune", &s), Long::Found("--prune"));
        assert_eq!(long_option("--prune-t", &s), Long::Found("--prune-targets"));
        assert_eq!(long_option("--op=x", &s), Long::Found("--opfor"));
        assert_eq!(long_option("--he", &s), Long::Found("--help"));
        assert_eq!(long_option("--zz", &s), Long::Unknown);
        assert_eq!(
            long_option("--pr=1", &s),
            Long::Ambiguous("ambiguous option: --pr=1 could match --prune, --prune-targets".into())
        );
    }

    #[test]
    fn python_int() {
        assert_eq!(py_int(" 120 "), Ok(Some(120)));
        assert_eq!(py_int("1_20"), Ok(Some(120)));
        assert_eq!(py_int("+7"), Ok(Some(7)));
        assert_eq!(py_int("-3"), Ok(Some(-3)));
        assert_eq!(py_int("1__0"), Ok(None));
        assert_eq!(py_int("_1"), Ok(None));
        assert_eq!(py_int("abc"), Ok(None));
        assert_eq!(py_int(""), Ok(None));
        assert_eq!(py_int("\u{0661}"), Err(()));
    }

    #[test]
    fn wraps_as_textwrap() {
        let c = ["hello", " ", "big-", "world", " ", "x"];
        assert_eq!(wrap(&c, 9, "", ""), vec!["hello", "big-world", "x"]);
        assert_eq!(wrap(&c, 6, "", ""), vec!["hello", "big-", "world", "x"]);
        assert_eq!(wrap(&["abcdefgh"], 3, "", ""), vec!["abc", "def", "gh"]);
    }

    #[test]
    fn long_breaks() {
        assert_eq!(finish("\nusage: x\n\n\n\nhelp\n\n\n"), "usage: x\n\nhelp\n");
    }
}
