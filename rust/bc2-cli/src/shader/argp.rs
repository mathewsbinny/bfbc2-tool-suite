//! The `argparse` parsers of the shader tools (`shaderdb.py`'s main parser and its subparsers, `renderqa.py`'s,
//! `xdiff.py`'s), for the command lines they take: options in any order, one run of positional values, `--x=v`,
//! types (`int`, `float`), choices, `append`, `nargs='*'` options, the help and the usage errors at the pipe width of
//! 80 (the texts are the Python's, recorded: `argparse` builds them from the parser).
//!
//! Handed to the Python ([`Fail::Python`]): the help and error texts when `COLUMNS` is set or stdout is a console
//! (`argparse` wraps them to that width), option abbreviations, `--`, attached short values (`-oX`), single-dash
//! options it does not know, a second run of positional values (`argparse`'s own matching rules across options),
//! `-h` after other arguments, and values whose `int()` / `float()` only Python decides (anything but plain ASCII
//! digits).

use std::collections::HashMap;

use crate::pyfmt::str_repr;
use crate::pyproc::{Fail, Py, PyResult};

/// What an option takes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `store_true`
    Flag,
    /// one value, the last one kept
    Store,
    /// one value each time, all kept (`action='append'`)
    Append,
    /// `nargs='*'`: every value after it
    Star,
}

/// A value's type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Str,
    Int,
    Float,
}

/// One option: its strings (`-o`, `--out`), the name argparse reports it by, what it takes, its type and choices.
pub struct Opt {
    pub strings: &'static [&'static str],
    pub kind: Kind,
    pub ty: Ty,
    pub choices: &'static [&'static str],
    pub required: bool,
}

/// How many values a positional takes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nargs {
    One,
    Optional,
    Star,
    Plus,
}

/// One positional: its name, how many values, choices.
pub struct Pos {
    pub name: &'static str,
    pub nargs: Nargs,
    pub choices: &'static [&'static str],
}

/// A parser: `prog` (for its error line), the usage and help texts, its actions in the order they were added
/// (`required` errors list them in that order: positionals and options interleaved as `order` says).
pub struct Parser {
    pub prog: &'static str,
    pub usage: &'static str,
    pub help: &'static str,
    pub opts: &'static [Opt],
    pub pos: &'static [Pos],
    /// the actions in the order they were added: `Ok(i)` positional i, `Err(i)` option i
    pub order: &'static [Result<usize, usize>],
}

/// The parsed values.
#[derive(Default, Debug)]
pub struct Args {
    pub values: HashMap<&'static str, Vec<String>>,
    pub flags: HashMap<&'static str, bool>,
    /// the arguments it did not recognize (`parse_args` reports them with the parser's usage)
    pub extras: Vec<String>,
}

impl Args {
    /// A positional's values, or a store option's (by its first string), or an append option's.
    pub fn all(&self, name: &str) -> &[String] {
        self.values.get(name).map_or(&[], Vec::as_slice)
    }

    /// The last value given (a store option keeps the last; a one-value positional has one).
    pub fn one(&self, name: &str) -> Option<&str> {
        self.values.get(name).and_then(|v| v.last()).map(String::as_str)
    }

    pub fn flag(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }
}

/// The outcome of a parse.
pub enum Parsed {
    /// the arguments, and the arguments left after the subcommand (a parser with `sub_at`)
    Args(Args),
    /// the help was printed (exit 0) or a usage error (exit 2): the exit code
    Exit(i32),
}

/// `argparse` wraps help and usage to the terminal's width: only the pipe's (80) is ported.
pub fn width_ported() -> bool {
    std::env::var_os("COLUMNS").is_none() && !crate::pystdio::is_console(&std::io::stdout())
}

/// An option string to `argparse` ('O') rather than a value ('A'): starts with '-', is not '-' alone, and is not a
/// negative number (the parsers have no option that looks like one).
pub fn is_option(a: &str) -> bool {
    let Some(rest) = a.strip_prefix('-') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    let neg = match rest.split_once('.') {
        None => digits(rest),
        Some((i, f)) => digits(i) && !f.is_empty() && digits(f),
    };
    !neg
}

/// `parser.error(message)`: the usage and the message on stderr, exit code 2.
pub fn error(py: &mut Py, p: &Parser, message: &str) -> Parsed {
    py.eprint(&format!("{}{}: error: {message}", p.usage, p.prog));
    Parsed::Exit(2)
}

/// `int(text)` where only plain ASCII digits (with a sign) are answered here: Some(Ok(value)), Some(Err) when Python
/// surely refuses (ASCII without a digit), None when Python decides.
fn py_int(v: &str) -> Option<Result<i64, ()>> {
    let body = v.strip_prefix(['-', '+']).unwrap_or(v);
    if !body.is_empty() && body.bytes().all(|c| c.is_ascii_digit()) {
        return v.parse::<i64>().ok().map(Ok);
    }
    if v.is_ascii() && !v.bytes().any(|c| c.is_ascii_digit()) && !v.bytes().any(|c| c.is_ascii_whitespace()) {
        return Some(Err(()));
    }
    None
}

/// `float(text)`: plain decimals only (Python's `float()` takes much more; anything else is its).
fn py_float(v: &str) -> Option<Result<f64, ()>> {
    let body = v.strip_prefix(['-', '+']).unwrap_or(v);
    let (i, f) = body.split_once('.').unwrap_or((body, ""));
    let plain = !body.is_empty()
        && body != "."
        && i.bytes().all(|c| c.is_ascii_digit())
        && f.bytes().all(|c| c.is_ascii_digit());
    if plain {
        return v.parse::<f64>().ok().map(Ok);
    }
    let low = v.to_ascii_lowercase();
    let special = ["inf", "infinity", "nan"]
        .iter()
        .any(|s| low.trim_start_matches(['-', '+']) == *s);
    if v.is_ascii()
        && !special
        && !v
            .bytes()
            .any(|c| c.is_ascii_digit() || c.is_ascii_whitespace() || c == b'.')
    {
        return Some(Err(()));
    }
    None
}

/// A value converted by the option's type: the error message argparse gives, or None (Python's to decide).
fn convert(ty: Ty, name: &str, v: &str) -> Result<Result<(), String>, Fail> {
    let bad = |t: &str| Ok(Err(format!("argument {name}: invalid {t} value: {}", str_repr(v))));
    match ty {
        Ty::Str => Ok(Ok(())),
        Ty::Int => match py_int(v) {
            Some(Ok(_)) => Ok(Ok(())),
            Some(Err(())) => bad("int"),
            None => Err(Fail::Python),
        },
        Ty::Float => match py_float(v) {
            Some(Ok(_)) => Ok(Ok(())),
            Some(Err(())) => bad("float"),
            None => Err(Fail::Python),
        },
    }
}

/// `argument X: invalid choice: 'v' (choose from 'a', 'b')`; int choices too (the reprs of their `str()`).
fn check_choice(name: &str, v: &str, choices: &[&str], ty: Ty) -> Option<String> {
    if choices.is_empty() {
        return None;
    }
    let (shown, ok): (String, bool) = if ty == Ty::Int {
        let n = v.parse::<i64>().ok()?;
        (
            str_repr(&n.to_string()),
            choices.iter().any(|c| c.parse::<i64>().ok() == Some(n)),
        )
    } else {
        (str_repr(v), choices.contains(&v))
    };
    if ok {
        return None;
    }
    // Python 3.13's argparse: the value and every choice as the repr of their str()
    let list: Vec<String> = choices.iter().map(|c| str_repr(c)).collect();
    Some(format!(
        "argument {name}: invalid choice: {shown} (choose from {})",
        list.join(", ")
    ))
}

/// The name argparse reports an option by in "required" lists (`-o/--out`) and in value errors (`--out`? no: the
/// strings joined with '/').
fn opt_name(o: &Opt) -> String {
    o.strings.join("/")
}

/// The counts a run of `n` values gives each positional of `pos` (argparse's greedy regex over the run; a prefix of
/// the positionals: the longest one that matches), and how many values are left over.
fn match_run(pos: &[Pos], n: usize) -> (Vec<usize>, usize) {
    for take in (1..=pos.len()).rev() {
        let ps = &pos[..take];
        let min: Vec<usize> = ps
            .iter()
            .map(|p| usize::from(matches!(p.nargs, Nargs::One | Nargs::Plus)))
            .collect();
        if min.iter().sum::<usize>() > n {
            continue;
        }
        let mut left = n;
        let mut counts = Vec::new();
        for (k, p) in ps.iter().enumerate() {
            let rest_min: usize = min[k + 1..].iter().sum();
            let room = left - rest_min;
            let c = match p.nargs {
                Nargs::One => 1,
                Nargs::Optional => usize::from(room >= 1),
                Nargs::Star => room,
                Nargs::Plus => room.max(1),
            };
            counts.push(c);
            left -= c;
        }
        return (counts, left);
    }
    (Vec::new(), n)
}

/// `parser.parse_args(argv)` (or, with `sub`, the main parser up to its subcommand: `Args` holds the options before
/// it, the subcommand under the positional's name, and `rest` the arguments after it).
#[allow(
    clippy::too_many_lines,
    reason = "argparse's loop over options and the run of values, kept in one piece"
)]
pub fn parse(py: &mut Py, p: &Parser, argv: &[String], sub: bool, rest: &mut Vec<String>) -> PyResult<Parsed> {
    if !width_ported() {
        return Err(Fail::Python);
    }
    let mut out = Args::default();
    let mut extras: Vec<String> = Vec::new();
    let mut seen_opts: Vec<usize> = Vec::new();
    let mut pos_done = 0usize; // positionals consumed
    let mut had_run = false;
    let mut after_option = false;
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        if a == "--" {
            return Err(Fail::Python);
        }
        if !is_option(a) {
            if had_run {
                return Err(Fail::Python); // a second run of values: argparse's rules across options
            }
            had_run = true;
            if after_option
                && p.pos
                    .get(pos_done)
                    .is_some_and(|q| matches!(q.nargs, Nargs::Star | Nargs::Optional))
            {
                return Err(Fail::Python); // an empty match at the option before it may have taken the positional
            }
            let mut n = 0;
            while i + n < argv.len() && !is_option(&argv[i + n]) && argv[i + n] != "--" {
                n += 1;
            }
            let run = &argv[i..i + n];
            if sub {
                // the main parser: its one positional is the subcommand (choices); everything after it is the
                // subparser's
                let Some(cmd) = p.pos.first() else {
                    return Err(Fail::Python);
                };
                let v = &run[0];
                if !cmd.choices.contains(&v.as_str()) {
                    let list: Vec<String> = cmd.choices.iter().map(|c| str_repr(c)).collect();
                    let msg = format!(
                        "argument {}: invalid choice: {} (choose from {})",
                        cmd.name,
                        str_repr(v),
                        list.join(", ")
                    );
                    return Ok(error(py, p, &msg));
                }
                out.values.insert(cmd.name, vec![v.clone()]);
                rest.extend(argv[i + 1..].iter().cloned());
                pos_done = 1;
                break;
            }
            let (counts, left) = match_run(&p.pos[pos_done..], n);
            let mut at = 0;
            for (k, c) in counts.iter().enumerate() {
                let q = &p.pos[pos_done + k];
                let vals = &run[at..at + c];
                for v in vals {
                    if !q.choices.is_empty() && !q.choices.contains(&v.as_str()) {
                        let list: Vec<String> = q.choices.iter().map(|c| str_repr(c)).collect();
                        let msg = format!(
                            "argument {}: invalid choice: {} (choose from {})",
                            q.name,
                            str_repr(v),
                            list.join(", ")
                        );
                        return Ok(error(py, p, &msg));
                    }
                }
                out.values.insert(q.name, vals.to_vec());
                at += c;
            }
            pos_done += counts.len();
            extras.extend(run[at..at + left].iter().cloned());
            i += n;
            continue;
        }
        after_option = true;
        if a == "-h" || a == "--help" {
            if i != 0 {
                return Err(Fail::Python); // -h after other arguments: their errors may come first
            }
            py.write(p.help)?;
            py.flush_out();
            return Ok(Parsed::Exit(0));
        }
        let (name, inline) = match a.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v)),
            _ => (a, None),
        };
        let Some(k) = p.opts.iter().position(|o| o.strings.contains(&name)) else {
            if !a.starts_with("--") || inline.is_some() {
                return Err(Fail::Python); // single-dash forms and --x=v: argparse's own
            }
            if p.opts
                .iter()
                .flat_map(|o| o.strings.iter())
                .chain(["--help"].iter())
                .any(|s| s.starts_with(name))
            {
                return Err(Fail::Python); // an abbreviation
            }
            extras.push(a.to_owned());
            i += 1;
            continue;
        };
        let o = &p.opts[k];
        let oname = opt_name(o);
        let key = o.strings[o.strings.len() - 1];
        seen_opts.push(k);
        match o.kind {
            Kind::Flag => {
                if inline.is_some() {
                    return Err(Fail::Python);
                }
                out.flags.insert(key, true);
                i += 1;
            }
            Kind::Store | Kind::Append => {
                let v = if let Some(v) = inline {
                    i += 1;
                    v.to_owned()
                } else {
                    match argv.get(i + 1) {
                        Some(v) if !is_option(v) && v != "--" => {
                            i += 2;
                            v.clone()
                        }
                        _ => return Ok(error(py, p, &format!("argument {oname}: expected one argument"))),
                    }
                };
                match convert(o.ty, &oname, &v)? {
                    Ok(()) => {}
                    Err(m) => return Ok(error(py, p, &m)),
                }
                if let Some(m) = check_choice(&oname, &v, o.choices, o.ty) {
                    return Ok(error(py, p, &m));
                }
                let slot = out.values.entry(key).or_default();
                if o.kind == Kind::Store {
                    slot.clear();
                }
                slot.push(v);
            }
            Kind::Star => {
                if inline.is_some() {
                    return Err(Fail::Python);
                }
                let mut vals = Vec::new();
                let mut j = i + 1;
                while j < argv.len() && !is_option(&argv[j]) && argv[j] != "--" {
                    match convert(o.ty, &oname, &argv[j])? {
                        Ok(()) => {}
                        Err(m) => return Ok(error(py, p, &m)),
                    }
                    vals.push(argv[j].clone());
                    j += 1;
                }
                if j < argv.len() && argv[j] == "--" {
                    return Err(Fail::Python);
                }
                out.values.insert(key, vals);
                i = j;
            }
        }
    }
    // the positionals not given: '*' / '?' take nothing; the required ones are reported with the required options
    let mut missing: Vec<String> = Vec::new();
    if sub {
        if pos_done == 0 {
            missing.push(p.pos[0].name.to_owned());
        }
    } else {
        for q in &p.pos[pos_done..] {
            if matches!(q.nargs, Nargs::Star) {
                out.values.entry(q.name).or_default();
            }
        }
    }
    for act in p.order {
        match *act {
            Ok(pi) => {
                if !sub && pi >= pos_done && matches!(p.pos[pi].nargs, Nargs::One | Nargs::Plus) {
                    missing.push(p.pos[pi].name.to_owned());
                }
            }
            Err(oi) => {
                if p.opts[oi].required && !seen_opts.contains(&oi) {
                    missing.push(opt_name(&p.opts[oi]));
                }
            }
        }
    }
    if !missing.is_empty() {
        return Ok(error(
            py,
            p,
            &format!("the following arguments are required: {}", missing.join(", ")),
        ));
    }
    if sub && !extras.is_empty() {
        return Err(Fail::Python); // the main parser's and the subparser's extras together: argparse's order
    }
    out.extras = extras;
    Ok(Parsed::Args(out))
}

/// `parse_args`' end: the unrecognized arguments reported with `p`'s usage (exit 2), else the arguments.
pub fn finish(py: &mut Py, p: &Parser, a: Args) -> Parsed {
    if a.extras.is_empty() {
        Parsed::Args(a)
    } else {
        let msg = format!("unrecognized arguments: {}", a.extras.join(" "));
        error(py, p, &msg)
    }
}
