//! The cross-set export canary of `tests/export_xf1.rs` written as files, for `mapexport build` timed and compared
//! between builds (rust/BENCHMARKS.md "PR1b-1-rest", audit AU5 ALG-5): Operation Aurora (`SP_01`) as a WW2 Conquest
//! level with Guadalcanal's radar bunker placed from a template of another blueprint.
//!
//! `cargo run --release -p bc2-levelwrite --example export_canary -- <game> <out folder>` writes
//! `<out>\canary.mapedit.json` and `<out>\canary.export.json`; then
//! `mapexport build <out>\canary.mapedit.json <out>\canary.export.json --game <game> --out <stage>`.
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::path::PathBuf;

use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::ops::Placed;
use bc2_levelwrite::{MapProject, Op, Profile};

const BUNKER: &str = "objects/architecture/beach/radarbunker_t01/radarbunker_t01";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(g), Some(out)) = (args.first(), args.get(1)) else {
        println!("export_canary <game> <out folder>");
        return;
    };
    let game = Game::open(&PathBuf::from(g)).expect("a game copy");
    let base = export::level_partition(&game, "sp_01").unwrap();
    let mut p = MapProject::new("XF1 cross-set", "sp_01", "levels/SP_01", &base, Profile::Bfbc2);
    let c = export::choose_mode(&game, &p, Profile::Ww2, "conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let gc = game.level_bundles("guadal_canal", "level");
    let bunker = export::blueprint_ref(&game, &gc, BUNKER).unwrap();
    let (cur, _) = export::current(&game, &p).unwrap();
    let template = cur.template_for(&bunker).unwrap();
    let n = p.next_n();
    let guid = bc2_levelwrite::new_guid(&p.id, n, &bunker);
    p.push(
        Op::Place {
            items: vec![Placed {
                guid,
                template,
                blueprint: bunker,
                ref_field: None,
                owned: Vec::new(),
                transform: bc2_levelwrite::rows_at([-640.0, 6.0, 330.0]),
                fields: Vec::new(),
                placement: None,
            }],
        },
        "XF1: Guadalcanal's radar bunker",
    );
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_XF1_AURORA_BUNKER".into(),
        folder: String::new(),
        title: "XF1 Aurora bunker".into(),
        mode: "Conquest".into(),
        layer: LayerSpec {
            donor: p.mode().unwrap().donor.clone(),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: Profile::Ww2,
        identity: None,
        path_data: false,
    };
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    p.save(&out.join("canary.mapedit.json")).unwrap();
    let text = serde_json::to_string_pretty(&spec).unwrap();
    bc2_gamefs::atomic::write(&out.join("canary.export.json"), &[text.as_bytes()]).unwrap();
    println!("wrote {}", out.display());
}
