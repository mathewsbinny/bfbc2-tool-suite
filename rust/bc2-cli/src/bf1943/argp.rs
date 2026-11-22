//! The parity tools' `argparse` parsers (Python 3.13's `_parse_known_args`, `allow_abbrev=True`) in full: `-h` /
//! `--help`, `-o O`, `--predict` (dbxparity) and the positional `names` (`nargs='*'`). Every form argparse takes is
//! answered here (round PR1b-1d: nothing is handed to the Python any more): abbreviations and their ambiguity error,
//! `--x=v`, attached short values (`-oX`) and short flags run together (`-ho`), `--` and what follows it, negative
//! numbers and values with a space taken as positionals, and positionals on both sides of an option (the first run
//! is `names`, a later one is unrecognised) - each step in the order argparse takes it, so a help that comes before
//! an error is printed and an error that comes before a help is the answer.

#![allow(clippy::similar_names, reason = "argparse's names: argv, args")]

use std::sync::LazyLock;

use regex::Regex;

use crate::pyfmt::str_repr;

/// An action of the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Help,
    O,
    Predict,
}

impl Act {
    /// `_get_action_name`: the option strings joined by `/`.
    fn name(self) -> &'static str {
        match self {
            Self::Help => "-h/--help",
            Self::O => "-o",
            Self::Predict => "--predict",
        }
    }

    /// How many arguments the action takes (`nargs` 0 or `None`).
    fn takes(self) -> usize {
        usize::from(self == Self::O)
    }
}

/// The parsed values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub names: Vec<String>,
    pub o: String,
    pub predict: bool,
}

/// What the parser answers: the values, the help (exit 0) or a usage error (its message; exit 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Run(Parsed),
    Help,
    Error(String),
}

/// An option tuple (`action`, `option_string`, `sep`, `explicit_arg`); `action` `None`: an unknown option.
type Tuple = (Option<Act>, String, Option<String>, Option<String>);

/// `_negative_number_matcher` (Python's `\d`: Unicode decimal digits, as the regex crate's).
static NEGATIVE: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^-\d+$|^-\d*\.\d+$").ok());

/// The option strings in `_option_string_actions`' order.
fn strings(predict: bool) -> &'static [(&'static str, Act)] {
    if predict {
        &[
            ("-h", Act::Help),
            ("--help", Act::Help),
            ("-o", Act::O),
            ("--predict", Act::Predict),
        ]
    } else {
        &[("-h", Act::Help), ("--help", Act::Help), ("-o", Act::O)]
    }
}

fn find(table: &[(&'static str, Act)], s: &str) -> Option<Act> {
    table.iter().find(|(k, _)| *k == s).map(|(_, a)| *a)
}

/// `str.partition('=')`: (head, sep, tail) with `None` for both when there is no `=`.
fn partition(s: &str) -> (&str, Option<String>, Option<String>) {
    match s.split_once('=') {
        Some((h, t)) => (h, Some("=".to_owned()), Some(t.to_owned())),
        None => (s, None, None),
    }
}

/// `_parse_optional(arg)`: `None` for a positional, else the option tuples.
fn parse_optional(table: &[(&'static str, Act)], arg: &str) -> Option<Vec<Tuple>> {
    if !arg.starts_with('-') {
        return None;
    }
    if let Some(a) = find(table, arg) {
        return Some(vec![(Some(a), arg.to_owned(), None, None)]);
    }
    let mut cs = arg.chars();
    cs.next();
    let second = cs.next()?;
    let (head, sep, explicit) = partition(arg);
    if sep.is_some()
        && let Some(a) = find(table, head)
    {
        return Some(vec![(Some(a), head.to_owned(), sep, explicit)]);
    }
    // _get_option_tuples
    let mut out = Vec::new();
    if second == '-' {
        for (s, a) in table {
            if s.starts_with(head) {
                out.push((Some(*a), (*s).to_owned(), sep.clone(), explicit.clone()));
            }
        }
    } else {
        let short: String = arg.chars().take(2).collect();
        let short_explicit: String = arg.chars().skip(2).collect();
        for (s, a) in table {
            if *s == short {
                out.push((
                    Some(*a),
                    (*s).to_owned(),
                    Some(String::new()),
                    Some(short_explicit.clone()),
                ));
            } else if s.starts_with(head) {
                out.push((Some(*a), (*s).to_owned(), sep.clone(), explicit.clone()));
            }
        }
    }
    if !out.is_empty() {
        return Some(out);
    }
    if NEGATIVE.as_ref().is_some_and(|r| r.is_match(arg)) || arg.contains(' ') {
        return None;
    }
    Some(vec![(None, arg.to_owned(), None, None)])
}

/// `parser.parse_args(argv)` of a parity tool (`predict`: dbxparity's parser, with `--predict`); `default_o` the
/// `-o` default.
#[must_use]
#[allow(clippy::too_many_lines, reason = "argparse's _parse_known_args, step for step")]
pub fn parse(argv: &[String], predict: bool, default_o: &str) -> Outcome {
    let table = strings(predict);
    // the pattern: 'O' an option, 'A' a value, '-' the first '--' (everything after it a value)
    let mut pattern: Vec<char> = Vec::with_capacity(argv.len());
    let mut tuples: Vec<Option<Vec<Tuple>>> = Vec::with_capacity(argv.len());
    let mut dashed = false;
    for a in argv {
        if dashed {
            pattern.push('A');
            tuples.push(None);
        } else if a == "--" {
            dashed = true;
            pattern.push('-');
            tuples.push(None);
        } else {
            let t = parse_optional(table, a);
            pattern.push(if t.is_some() { 'O' } else { 'A' });
            tuples.push(t);
        }
    }
    let mut out = Parsed {
        names: Vec::new(),
        o: default_o.to_owned(),
        predict: false,
    };
    let mut names_left = true;
    let mut extras: Vec<String> = Vec::new();

    // consume_positionals: `names` ('(-*[A-]*)') once, unless it would match nothing right before an option
    let consume_positionals = |start: usize, names_left: &mut bool, out: &mut Parsed| -> usize {
        if !*names_left {
            return start;
        }
        let count = pattern
            .get(start..)
            .unwrap_or_default()
            .iter()
            .take_while(|c| **c == 'A' || **c == '-')
            .count();
        if count == 0 && pattern.get(start) == Some(&'O') {
            return start;
        }
        let mut args: Vec<String> = argv.get(start..start + count).unwrap_or_default().to_vec();
        if pattern.get(start..start + count).unwrap_or_default().contains(&'-')
            && let Some(i) = args.iter().position(|a| a == "--")
        {
            args.remove(i);
        }
        out.names = args;
        *names_left = false;
        start + count
    };

    let max_option = pattern.iter().rposition(|c| *c == 'O');
    let mut start = 0usize;
    if let Some(max_option) = max_option {
        while start <= max_option {
            let mut next = start;
            while next <= max_option && pattern.get(next) != Some(&'O') {
                next += 1;
            }
            if start != next {
                let end = consume_positionals(start, &mut names_left, &mut out);
                if end > start {
                    start = end;
                    continue;
                }
                start = end;
            }
            if pattern.get(start) != Some(&'O') {
                extras.extend(argv.get(start..next).unwrap_or_default().iter().cloned());
                start = next;
            }
            // consume_optional(start)
            let Some(Some(ts)) = tuples.get(start) else {
                break;
            };
            if ts.len() > 1 {
                let matches: Vec<&str> = ts.iter().map(|t| t.1.as_str()).collect();
                return Outcome::Error(format!(
                    "ambiguous option: {} could match {}",
                    argv.get(start).map_or("", String::as_str),
                    matches.join(", ")
                ));
            }
            let Some((first_act, first_s, first_sep, first_explicit)) = ts.first().cloned() else {
                break;
            };
            let (mut action, mut s, mut sep, mut explicit) = (first_act, first_s, first_sep, first_explicit);
            let mut taken: Vec<(Act, Vec<String>)> = Vec::new();
            let stop;
            loop {
                let Some(act) = action else {
                    extras.push(argv.get(start).cloned().unwrap_or_default());
                    stop = start + 1;
                    break;
                };
                if let Some(e) = explicit.clone() {
                    let count = act.takes();
                    let single = s.chars().nth(1).is_some_and(|c| c != '-');
                    if count == 0 && single && !e.is_empty() {
                        if sep.as_deref().is_some_and(|x| !x.is_empty()) || e.starts_with('-') {
                            return Outcome::Error(format!(
                                "argument {}: ignored explicit argument {}",
                                act.name(),
                                str_repr(&e)
                            ));
                        }
                        taken.push((act, Vec::new()));
                        let mut ec = e.chars();
                        let c0 = ec.next().unwrap_or_default();
                        s = format!("-{c0}");
                        if let Some(a) = find(table, &s) {
                            action = Some(a);
                            let rest: String = ec.collect();
                            if rest.is_empty() {
                                sep = None;
                                explicit = None;
                            } else if let Some(r) = rest.strip_prefix('=') {
                                sep = Some("=".to_owned());
                                explicit = Some(r.to_owned());
                            } else {
                                sep = Some(String::new());
                                explicit = Some(rest);
                            }
                        } else {
                            extras.push(format!("-{e}"));
                            stop = start + 1;
                            break;
                        }
                    } else if count == 1 {
                        taken.push((act, vec![e]));
                        stop = start + 1;
                        break;
                    } else {
                        return Outcome::Error(format!(
                            "argument {}: ignored explicit argument {}",
                            act.name(),
                            str_repr(&e)
                        ));
                    }
                } else {
                    let from = start + 1;
                    let count = act.takes();
                    if count == 1 && pattern.get(from) != Some(&'A') {
                        return Outcome::Error(format!("argument {}: expected one argument", act.name()));
                    }
                    taken.push((act, argv.get(from..from + count).unwrap_or_default().to_vec()));
                    stop = from + count;
                    break;
                }
            }
            for (act, args) in taken {
                match act {
                    Act::Help => return Outcome::Help,
                    Act::O => out.o = args.into_iter().next().unwrap_or_default(),
                    Act::Predict => out.predict = true,
                }
            }
            start = stop;
        }
    }
    let stop = consume_positionals(start, &mut names_left, &mut out);
    extras.extend(argv.get(stop..).unwrap_or_default().iter().cloned());
    if !extras.is_empty() {
        return Outcome::Error(format!("unrecognized arguments: {}", extras.join(" ")));
    }
    Outcome::Run(out)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "the test's own recorded cases"
)]
mod tests {
    use bc2_pyrt::json::{self, Value};

    use super::*;

    /// 1,200 command lines (random runs of option and value forms, both parsers) and what Python 3.13's argparse
    /// answered each (`ap.parse_args` with `exit_on_error=False`; recorded in `argp_cases.json`).
    #[test]
    fn every_recorded_answer() {
        let rows = json::loads(include_str!("argp_cases.json")).expect("the recorded cases");
        let rows = rows.as_list().expect("a list");
        assert_eq!(rows.len(), 1200);
        for row in rows {
            let r = row.as_list().expect("a row");
            let args: Vec<String> = r[0]
                .as_list()
                .expect("args")
                .iter()
                .map(|a| a.as_str().expect("an arg").to_owned())
                .collect();
            let predict = r[1].truthy();
            let want = r[2].as_list().expect("an answer");
            let expected = match want[0].as_str() {
                Some("run") => Outcome::Run(Parsed {
                    names: want[1]
                        .as_list()
                        .expect("names")
                        .iter()
                        .map(|a| a.as_str().unwrap_or("").to_owned())
                        .collect(),
                    o: want[2].as_str().unwrap_or("").to_owned(),
                    predict: want[3].truthy(),
                }),
                Some("help") => Outcome::Help,
                _ => Outcome::Error(want[1].as_str().unwrap_or("").to_owned()),
            };
            assert_eq!(
                parse(&args, predict, "x.txt"),
                expected,
                "{args:?} (predict parser: {predict})"
            );
            let _: &Value = row;
        }
    }
}
