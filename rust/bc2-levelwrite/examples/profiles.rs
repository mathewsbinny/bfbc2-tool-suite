//! Prints the content profiles of a game copy for a mode (round E3b): each set's levels, common bundles, donors and
//! level fields, as `bc2_levelwrite::profile::scan` reads them.
//!
//! `cargo run -p bc2-levelwrite --example profiles -- <game folder> [mode]`
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::many_single_char_names
)]

use std::path::PathBuf;

use bc2_levelwrite::export::Game;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = Game::open(&PathBuf::from(&a[1])).expect("game");
    let mode = a.get(2).map_or("conquest", String::as_str);
    let t = std::time::Instant::now();
    let p = bc2_levelwrite::profile::scan(&game, mode).unwrap();
    for s in &p.sets {
        println!("== {} ({} multiplayer levels)", s.profile.word(), s.levels.len());
        for (k, v) in &s.commons {
            println!("   commons {k}: {v:?}");
        }
        println!("   fields {:?}", s.fields);
        for d in &s.donors {
            let (b, f) = d.bases_and_flags();
            println!(
                "   donor {} mode {} factions {:?} kits {} bases {:?} flags {:?} outside {:?}",
                d.level,
                d.game_mode,
                d.factions,
                d.kits.len(),
                b.iter().map(|c| &c.name).collect::<Vec<_>>(),
                f.iter().map(|c| &c.name).collect::<Vec<_>>(),
                d.outside
            );
        }
        println!("   default donor {:?}", s.default_donor().map(|d| &d.level));
    }
    println!("{:.1} s", t.elapsed().as_secs_f64());
}
