//! Round V1's proof run: every rule of the registry against every stock multiplayer level of the game copy (as the
//! level is: its own mode and era) and against staged exports (`mapexport check`'s lines with their codes).
//!
//! ```text
//! cargo run --release -p bc2-levelwrite --example v1_rules -- <game> [--measures <out.json>] [--notes]
//!     [--no-stock] [--stage <folder>]...
//! ```
//!
//! Prints, per stock level, its findings by severity (errors and warnings in full, notes with `--notes`), the totals
//! per rule, and what DICE's unedited levels hold for the rules that report only what edits bring; with
//! `--measures`, L1's measurements of every level as JSON (the parity with `l1_measure.py`); per stage, every check
//! line with its code. Nothing is written to the game copy.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use bc2_levelwrite::Profile;
use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::placement::{self, Mode};
use bc2_levelwrite::rules::Severity;
use bc2_levelwrite::write::LevelWriter;

/// The stock multiplayer levels with their era and mode, as L1's `l1_levels.py` lists them: `maps.ini`'s mode keys
/// (Onslaught and Air Superiority left out: L1 gives them no rules), the era by the name.
fn stock_levels(game: &str) -> Vec<(String, Profile, Mode)> {
    let text = std::fs::read_to_string(PathBuf::from(game).join("maps.ini")).unwrap();
    let mut out: Vec<(String, Profile, Mode)> = Vec::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let mode = match k.trim().to_ascii_lowercase().as_str() {
            "conquest" => Mode::Conquest,
            "rush" => Mode::Rush,
            "squad rush" => Mode::SquadRush,
            "squad deathmatch" => Mode::SquadDeathmatch,
            _ => continue,
        };
        let lv = v.trim().rsplit('/').next().unwrap_or("").to_ascii_lowercase();
        if lv.is_empty() || out.iter().any(|x| x.0 == lv) {
            continue;
        }
        let profile = if lv.starts_with("bc1_") {
            Profile::Bfbc1
        } else if lv.starts_with("nam_") {
            Profile::Vietnam
        } else if ["wake_island_s", "guadal_canal", "iwo_jima_s", "coral_sea"].contains(&lv.as_str()) {
            Profile::Ww2
        } else {
            Profile::Bfbc2
        };
        out.push((lv, profile, mode));
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let game = Game::open(&PathBuf::from(&args[0])).unwrap();
    let value = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let stages: Vec<String> = args
        .windows(2)
        .filter(|w| w[0] == "--stage")
        .map(|w| w[1].clone())
        .collect();
    let mut measures = Vec::new();
    let mut totals: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    let mut dice: BTreeMap<String, usize> = BTreeMap::new();
    let mut levels = 0;
    let t = std::time::Instant::now();
    if !args.iter().any(|a| a == "--no-stock") {
        for (lv, profile, mode) in stock_levels(&args[0]) {
            let t0 = std::time::Instant::now();
            let (_, found) = match bc2_levelwrite::validate::stock(&game, &lv, profile, Some(mode)) {
                Ok(x) => x,
                Err(e) => {
                    println!("{lv}: {e}");
                    continue;
                }
            };
            levels += 1;
            let n = |s: Severity| found.iter().filter(|f| f.severity == s).count();
            println!(
                "{lv:<22} {:<8} {:<4} errors {} warnings {} notes {} ({:.0} ms)",
                placement::group(profile),
                mode.word(),
                n(Severity::Error),
                n(Severity::Warning),
                n(Severity::Note),
                t0.elapsed().as_secs_f64() * 1000.0
            );
            for f in &found {
                totals.entry(f.code.clone()).or_default()[f.severity as usize] += 1;
                if f.severity != Severity::Note || args.iter().any(|a| a == "--notes") {
                    println!("    {}", f.line());
                }
            }
            let bytes = export::level_partition(&game, &lv).unwrap();
            let w = LevelWriter::parse(&bytes).unwrap();
            if args.iter().any(|a| a == "--time") {
                let t1 = std::time::Instant::now();
                let _ = placement::measure(&w, mode);
                let t2 = std::time::Instant::now();
                let _ = bc2_levelwrite::validate::dbx_check(&w);
                println!(
                    "    time: measure {:.0} ms, DBX check {:.0} ms",
                    (t2 - t1).as_secs_f64() * 1000.0,
                    t2.elapsed().as_secs_f64() * 1000.0
                );
            }
            // what DICE's own level holds for the rules that report only what edits bring
            for f in bc2_levelwrite::validate::dbx_check(&w)
                .into_iter()
                .chain(bc2_levelwrite::validate::out_of_range(&w, None))
                .chain(bc2_levelwrite::validate::stacked_spawners(&w, None))
            {
                *dice.entry(f.code.clone()).or_default() += 1;
                if args.iter().any(|a| a == "--dice") {
                    println!("    DICE {}", f.line());
                }
            }
            if value("--measures").is_some() {
                for m in placement::measure(&w, mode) {
                    measures.push(serde_json::json!({
                        "level": lv, "mode": mode.word(), "group": placement::group(profile),
                        "metric": m.metric, "value": m.value, "dy": m.dy, "stage": m.stage,
                        "subject": m.subject, "tag": m.tag,
                    }));
                }
            }
        }
        println!(
            "\n{levels} stock levels in {:.1} s; findings per rule (notes, warnings, errors):",
            t.elapsed().as_secs_f64()
        );
        for (code, [n, w, e]) in &totals {
            println!("  {code:<12} {n:>4} {w:>4} {e:>4}");
        }
        println!("DICE's own levels, the rules that report only what edits bring (findings on the unedited levels):");
        for (code, n) in &dice {
            println!("  {code:<12} {n:>4}");
        }
    }
    if let Some(p) = value("--measures") {
        std::fs::write(&p, serde_json::to_string_pretty(&measures).unwrap()).unwrap();
        println!("wrote {} measurements to {p}", measures.len());
    }
    for s in &stages {
        let t0 = std::time::Instant::now();
        let r = export::check(&game, &PathBuf::from(s)).unwrap();
        println!("\n=== {s} ({:.1} s)", t0.elapsed().as_secs_f64());
        let mut codes: BTreeMap<&str, usize> = BTreeMap::new();
        for (code, ok, what) in r.lines() {
            *codes.entry(code).or_default() += 1;
            println!(
                "{}",
                export::CheckReport::line(code, ok, &what.chars().take(160).collect::<String>())
            );
        }
        println!(
            "check: {} - {} lines, {} rules: {:?}",
            if r.ok() { "ok" } else { "FAILED" },
            r.checks.len(),
            codes.len(),
            codes
        );
    }
}
