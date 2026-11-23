//! A Rush level's stage wiring (round E4-rest, spec GAME-RUSH-11): per stage in play order, every event reached from its M-COMs'
//! `onDestroyed` (through any instance), with the targets' types and names.
//!
//! `cargo run --release -p bc2-levelwrite --example rush_wiring -- <game> <level> [depth]`
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::iter_over_hash_type,
    clippy::similar_names
)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::modelayer;
use bc2_levelwrite::write::LevelWriter;

fn named(r: &[u8]) -> String {
    String::from_utf8_lossy(r.rsplit(|&c| c == b'/').next().unwrap_or(r)).to_ascii_uppercase()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let game = Game::open(&PathBuf::from(&args[1])).unwrap();
    let lv = &args[2];
    let depth: usize = args.get(3).map_or(6, |d| d.parse().unwrap());
    let w = LevelWriter::parse(&export::level_partition(&game, lv).unwrap()).unwrap();
    let mut out: HashMap<String, Vec<(String, String, String)>> = HashMap::new();
    let mut inn: HashMap<String, Vec<(String, String, String)>> = HashMap::new();
    for a in w.connection_arrays() {
        for &c in w.d.children(a).unwrap_or_default() {
            let end = |f: &[u8]| w.d.child(c, f).and_then(|x| w.d.attr(x, b"ref")).map(named);
            let ev = |f: &[u8]| {
                w.d.child(c, f)
                    .and_then(|x| w.d.child(x, b"Name"))
                    .and_then(|x| w.d.text(x))
                    .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned())
            };
            if let (Some(s), Some(t)) = (end(b"Source"), end(b"Target")) {
                out.entry(s.clone())
                    .or_default()
                    .push((ev(b"SourceEvent"), t.clone(), ev(b"TargetEvent")));
                inn.entry(t)
                    .or_default()
                    .push((ev(b"SourceEvent"), s, ev(b"TargetEvent")));
            }
        }
    }
    let label = |g: &str| -> String {
        let Some(e) = w.find(g) else { return format!("{g}(ext)") };
        let ty = w.type_of(e);
        let ty = ty
            .rsplit('.')
            .next()
            .unwrap_or(&ty)
            .trim_end_matches("EntityData")
            .to_owned();
        let name = ["Name", "Sid", "StringId"]
            .iter()
            .find_map(|f| w.d.child(e, f.as_bytes()).and_then(|x| w.d.text(x)))
            .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned());
        let en = modelayer::bool_field(&w, e, b"Enabled").map_or("", |b| if b { "" } else { " [off]" });
        let bp =
            w.d.child(e, b"ReferencedObject")
                .and_then(|x| w.d.attr(x, b"ref"))
                .map_or_else(String::new, |r| format!(" <{}>", String::from_utf8_lossy(r)));
        format!(
            "{ty}:{}{}{}{}",
            &g[..8.min(g.len())],
            if name.is_empty() {
                String::new()
            } else {
                format!(" '{name}'")
            },
            en,
            bp
        )
    };
    let (play, unc) = modelayer::rush_play(&w);
    println!("{lv}: {} stages, uncertain {unc}", play.len());
    for (k, s) in play.iter().enumerate() {
        println!(
            "== stage {} M-COMs {:?}",
            k + 1,
            s.mcoms.iter().map(|m| label(m)).collect::<Vec<_>>()
        );
        let mut seen: HashSet<String> = HashSet::new();
        let mut q: VecDeque<(String, usize)> = VecDeque::new();
        for m in &s.mcoms {
            for (se, t, te) in out.get(m).into_iter().flatten() {
                if se == "onDestroyed" {
                    println!("  M {} -{se}-> {} .{te}", &m[..8], label(t));
                    q.push_back((t.clone(), 1));
                }
            }
        }
        while let Some((x, d)) = q.pop_front() {
            if !seen.insert(x.clone()) || d > depth {
                continue;
            }
            for (se, t, te) in out.get(&x).into_iter().flatten() {
                println!("  {}{} -{se}-> {} .{te}", "  ".repeat(d), label(&x), label(t));
                q.push_back((t.clone(), d + 1));
            }
        }
    }
    // who enables / disables each counter, base and combat area
    println!("== Enable/Disable targets by type");
    for t in [
        "MultipleTrigger",
        "CapturePoint",
        "CombatAreaTrigger",
        "LifeCounter",
        "SoldierSpawn",
    ] {
        for e in w.d.of_type(format!(".{t}EntityData").as_bytes()) {
            let g = named(w.d.attr(e, b"guid").unwrap());
            let ins: Vec<String> = inn
                .get(&g)
                .into_iter()
                .flatten()
                .map(|(se, s, te)| format!("{} -{se}-> .{te}", label(s)))
                .collect();
            let outs: Vec<String> = out
                .get(&g)
                .into_iter()
                .flatten()
                .map(|(se, t, te)| format!("-{se}-> {} .{te}", label(t)))
                .collect();
            println!("{}\n   in: {ins:?}\n   out: {outs:?}", label(&g));
        }
    }
}
