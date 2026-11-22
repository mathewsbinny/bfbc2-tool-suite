//! The export's path-data build on an unedited project of a level (round NK6's proof): `pathdata::build_bytes` - the
//! code the export's path-data step runs - on a project of `level` with no operations, its lines printed, the path
//! data written to `out`, so a script can compare it with `bfbc2.exe navkit build`'s.
//!
//! ```text
//! cargo run --release -p bc2-levelwrite --example pathdata_build -- <opfor> <game> <level> <out>
//! ```
//!
//! `opfor` is the OPFOR folder whose `work` the build reads (a stand-in with junctions to the real one is fine);
//! `game` the game copy the level's partition is read from.

#![allow(clippy::print_stdout, clippy::print_stderr, reason = "a proof's command line prints its lines")]

use std::path::PathBuf;

use bc2_levelwrite::pathdata;
use bc2_levelwrite::project::MapProject;
use bc2_levelwrite::write::LevelWriter;

struct Print;

impl bc2_navbuild::inputs::Out for Print {
    fn line(&mut self, text: &str) -> bc2_navbuild::ntpath::Res<()> {
        println!("{text}");
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [opfor, game, level, out] = args.as_slice() else {
        eprintln!("usage: pathdata_build <opfor> <game> <level> <out>");
        std::process::exit(2);
    };
    let opfor = PathBuf::from(opfor);
    let game = match bc2_levelwrite::export::Game::open(&PathBuf::from(game)) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let bytes = match bc2_levelwrite::export::level_partition(&game, level) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let base = LevelWriter::parse(&bytes).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let project = MapProject::new(
        "pathdata proof",
        level,
        &format!("Levels/{}", level.to_ascii_uppercase()),
        &bytes,
        bc2_levelwrite::project::Profile::Bfbc2,
    );
    let plan = pathdata::ready(&opfor, &project, &base, &base);
    let samples = match plan {
        Ok(pathdata::Plan::Build(s)) => s,
        Ok(pathdata::Plan::Sample) => {
            eprintln!("the project changes the collision: the stage must be sampled");
            std::process::exit(1);
        }
        Ok(pathdata::Plan::Copy(src)) => {
            eprintln!("no samples to build from (the step would copy {})", src.infantry.display());
            std::process::exit(1);
        }
        Err(why) => {
            eprintln!("{}", why.join("; "));
            std::process::exit(1);
        }
    };
    println!("samples: {} ({})", samples.kind, samples.dir.display());
    let clock = bc2_navbuild::inputs::local_clock();
    match pathdata::build_bytes(&opfor, &project, &base, &base, &samples, level, clock, &mut Print) {
        Ok((data, _)) => {
            if let Err(e) = std::fs::write(out, &data) {
                eprintln!("{out}: {e}");
                std::process::exit(1);
            }
            println!("wrote {out}: {} bytes", data.len());
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
