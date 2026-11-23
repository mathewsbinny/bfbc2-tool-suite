//! The Rush stage wiring's facts on every stock Rush / Squad Rush level (round E4-rest, spec GAME-RUSH-11): base
//! counters per stage, the enable chain, what each base counter switches (by target type and stage), how each
//! stage's own objects come into play, the defenders' `BaseCount`.
//!
//! `cargo run --release -p bc2-levelwrite --example rush_facts -- <game>`
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::iter_over_hash_type,
    clippy::uninlined_format_args,
    clippy::cast_possible_wrap,
    clippy::similar_names,
    clippy::many_single_char_names
)]

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::gameplay;
use bc2_levelwrite::modelayer;
use bc2_levelwrite::write::LevelWriter;

fn named(r: &[u8]) -> String {
    String::from_utf8_lossy(r.rsplit(|&c| c == b'/').next().unwrap_or(r)).to_ascii_uppercase()
}

type Conn = (String, String, String, String);

fn conns(w: &LevelWriter) -> Vec<Conn> {
    let mut out = Vec::new();
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
                out.push((s, ev(b"SourceEvent"), t, ev(b"TargetEvent")));
            }
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let game = Game::open(&PathBuf::from(&args[1])).unwrap();
    let modes = bc2_levelwrite::profile::modes(&game).unwrap();
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut bump = |k: String| *tally.entry(k).or_default() += 1;
    for (set, by_mode) in &modes {
        for (mode, lvs) in by_mode {
            if !["rush", "goldrush", "squadrush"].contains(&mode.as_str()) {
                continue;
            }
            for lv in lvs {
                let w = LevelWriter::parse(&export::level_partition(&game, lv).unwrap()).unwrap();
                let cs = conns(&w);
                let ty = |g: &str| -> String {
                    w.find(g).map_or_else(
                        || "?".into(),
                        |e| {
                            let t = w.type_of(e);
                            t.rsplit('.')
                                .next()
                                .unwrap_or(&t)
                                .trim_end_matches("EntityData")
                                .to_owned()
                        },
                    )
                };
                let enabled = |g: &str| w.find(g).and_then(|e| modelayer::bool_field(&w, e, b"Enabled"));
                let (play, unc) = modelayer::rush_play(&w);
                let lifes: Vec<String> =
                    w.d.of_type(b".LifeCounterEntityData")
                        .into_iter()
                        .map(|e| named(w.d.attr(e, b"guid").unwrap()))
                        .collect();
                // base counters: the stage counters whose OnTrigger reaches DestructBase
                let mut bases: Vec<Vec<String>> = Vec::new();
                for s in &play {
                    let b: Vec<String> = gameplay::stage_counters(&w, &s.mcoms)
                        .into_iter()
                        .map(|x| x.0)
                        .filter(|c| {
                            cs.iter().any(|(s, se, t, te)| {
                                s == c && se == "OnTrigger" && te == "DestructBase" && lifes.contains(t)
                            })
                        })
                        .collect();
                    bases.push(b);
                }
                let n = play.len();
                let per: Vec<usize> = bases.iter().map(Vec::len).collect();
                println!("{set:?} {lv} {mode}: {n} stages (uncertain {unc}), base counters per stage {per:?}");
                bump(format!("levels {mode}"));
                if per.iter().all(|&c| c == 1) {
                    bump("one base counter per stage".into());
                } else {
                    // print the destructbase sources
                    for (s, se, t, te) in &cs {
                        if te == "DestructBase" {
                            println!("    DestructBase from {} {} ({se}) -> {}", ty(s), &s[..8], &t[..8]);
                        }
                    }
                }
                let b: Vec<String> = bases.iter().map(|v| v.first().cloned().unwrap_or_default()).collect();
                // chain
                let mut chain_ok = true;
                for k in 0..n {
                    let en = enabled(&b[k]);
                    if (k == 0) != (en != Some(false)) {
                        chain_ok = false;
                        println!("    stage {} base counter Enabled {en:?}", k + 1);
                    }
                    let enables: Vec<String> = cs
                        .iter()
                        .filter(|(s, _, t, te)| *s == b[k] && te == "Enable" && ty(t) == "MultipleTrigger")
                        .map(|x| x.2.clone())
                        .collect();
                    let want: Vec<String> = if k + 1 < n { vec![b[k + 1].clone()] } else { vec![] };
                    if enables != want {
                        chain_ok = false;
                        println!("    stage {} base enables counters {enables:?} want {want:?}", k + 1);
                    }
                    // who enables b[k]
                    let by: Vec<String> = cs
                        .iter()
                        .filter(|(_, _, t, te)| *t == b[k] && te == "Enable")
                        .map(|x| x.0.clone())
                        .collect();
                    let want_by: Vec<String> = if k > 0 { vec![b[k - 1].clone()] } else { vec![] };
                    if by != want_by {
                        chain_ok = false;
                        println!("    stage {} base enabled by {by:?}", k + 1);
                    }
                }
                if chain_ok {
                    bump("enable chain exact".into());
                }
                // other stage counters: enabled from the start?
                for (k, s) in play.iter().enumerate() {
                    for (c, _) in gameplay::stage_counters(&w, &s.mcoms) {
                        if c != b[k] {
                            bump(format!("other counter enabled {:?} stage1 {}", enabled(&c), k == 0));
                        }
                    }
                }
                // what each base counter targets: by (sev, tev, target type, where)
                let on = |k: usize, g: &str| play[k].enabled.get(g).copied().unwrap_or(true);
                let own = |k: usize| -> HashSet<String> {
                    play[k]
                        .enabled
                        .keys()
                        .filter(|g| on(k, g) && (0..n).all(|j| j == k || !on(j, g)))
                        .cloned()
                        .collect()
                };
                let owns: Vec<HashSet<String>> = (0..n).map(own).collect();
                for k in 0..n {
                    let last = k + 1 == n;
                    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
                    for (s, se, t, te) in &cs {
                        if *s != b[k] {
                            continue;
                        }
                        let wh = (0..n)
                            .find(|&j| owns[j].contains(t))
                            .map_or_else(|| "shared".to_owned(), |j| format!("own{:+}", j as i64 - k as i64));
                        *kinds.entry(format!("{se}->{}.{te} [{wh}]", ty(t))).or_default() += 1;
                    }
                    for key in kinds.keys() {
                        bump(format!("{} base: {key}", if last { "last" } else { "mid" }));
                    }
                }
                // entries: how each root of own(k) (k>=1) is switched on
                for k in 1..n {
                    for g in &owns[k] {
                        let t = ty(g);
                        let ins: Vec<&Conn> = cs.iter().filter(|c| c.2 == *g && c.3 == "Enable").collect();
                        let from_prev = ins.iter().any(|c| c.0 == b[k - 1]);
                        let from_own_base = ins.iter().any(|c| c.0 == b[k]);
                        let other = ins
                            .iter()
                            .filter(|c| c.0 != b[k - 1] && c.0 != b[k])
                            .map(|c| ty(&c.0))
                            .collect::<Vec<_>>();
                        bump(format!(
                            "own {t}: prev {from_prev} ownbase {from_own_base} other {}",
                            if other.is_empty() { "-".into() } else { other.join(",") }
                        ));
                    }
                    let mut types: BTreeMap<String, usize> = BTreeMap::new();
                    for g in &owns[k] {
                        *types.entry(ty(g)).or_default() += 1;
                    }
                    println!("    own({}) {:?}", k + 1, types);
                }
                let mut types: BTreeMap<String, usize> = BTreeMap::new();
                for g in &owns[0] {
                    *types.entry(ty(g)).or_default() += 1;
                }
                println!("    own(1) {:?}", types);
                // the defenders' BaseCount
                for e in w.d.of_type(b".LifeCounterEntityData") {
                    if modelayer::bool_field(&w, e, b"Attacker") == Some(false) {
                        let bc = w.d.child(e, b"BaseCount").map(|x| w.d.raw(x).to_vec());
                        println!("    BaseCount raw {bc:02x?}");
                    }
                }
            }
        }
    }
    for (k, v) in &tally {
        println!("{v:5} {k}");
    }
}
