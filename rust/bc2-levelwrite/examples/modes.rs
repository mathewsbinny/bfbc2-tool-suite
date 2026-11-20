//! Survey of the game copy's multiplayer modes (round E3d): every level with a multiplayer description, its set and
//! mode, and (with `--types`) the instance types of its level partition, to see what each mode's layer is made of.
//!
//! `cargo run -p bc2-levelwrite --example modes -- <game folder> [--types [level ...]]`
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use bc2_levelwrite::LevelWriter;
use bc2_levelwrite::export::{Game, read_found};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = Game::open(&PathBuf::from(&a[1])).expect("game");
    let modes = bc2_levelwrite::profile::modes(&game).unwrap();
    for (p, m) in &modes {
        println!("== {}", p.word());
        for (mode, lvs) in m {
            println!("   {mode}: {}", lvs.join(", "));
        }
    }
    if a.get(2).map(String::as_str) == Some("--types") {
        let only: Vec<String> = a.iter().skip(3).map(|s| s.to_ascii_lowercase()).collect();
        for m in modes.values() {
            for lvs in m.values() {
                for lv in lvs {
                    if !only.is_empty() && !only.contains(lv) {
                        continue;
                    }
                    let live = game.live(&game.loaded(lv, "level")).unwrap();
                    let Some(f) = live.get(&format!("levels/{lv}.dbx")) else {
                        continue;
                    };
                    let data = read_found(&[f]).unwrap().pop().unwrap();
                    let w = LevelWriter::parse(&data).unwrap();
                    let mut by: BTreeMap<String, usize> = BTreeMap::new();
                    for e in w.d.instances() {
                        let t = w.type_of(e);
                        *by.entry(t.rsplit('.').next().unwrap_or(&t).to_owned()).or_default() += 1;
                    }
                    println!("-- {lv}: {by:?}");
                }
            }
        }
    }
}
