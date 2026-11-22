//! The calibration of the vehicle rules (XVAL-PLACE-3 and 6 to 8) on DICE's own vehicle spawners: for each stock level
//! of the game copy the editor can open with its collision, every vehicle spawner as DICE placed it - its class, team,
//! the capture point that owns it - with what stands around it (`bc2_levelwrite::vehicles::around`: its room, how far
//! a land vehicle drives, a boat sails, the open sky, the longest take-off run), and the rules' findings (they must
//! find nothing on DICE's data). `--json` writes every spawner's numbers.
//!
//! ```text
//! cargo run --release -p bc2-editor --example vehicle_sites -- [<level>...] [--opfor <folder>] [--json out]
//! ```
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]

use std::collections::{BTreeMap, HashSet};

use bc2_editor::levels;
use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::vehicles;
use bc2_levelwrite::write::LevelWriter;

/// The Package levels A1 calibrated on besides the prepared Dist levels.
const PACKAGE: &[&str] = &[
    "mp_005gr",
    "nam_mp_002cq",
    "nam_mp_005r",
    "wake_island_s",
    "guadal_canal",
    "iwo_jima_s",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let root = levels::find_opfor(value("--opfor").as_deref().map(std::path::Path::new)).unwrap();
    let game = Game::open(&root.join("game")).unwrap();
    let flagged: Vec<String> = ["--opfor", "--json"].iter().filter_map(|k| value(k)).collect();
    let mut wanted: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--") && !flagged.contains(a))
        .cloned()
        .collect();
    if wanted.is_empty() {
        wanted = levels::list_levels(&root)
            .into_iter()
            .filter(|l| l.prepared && l.lv.starts_with("mp_"))
            .map(|l| l.lv)
            .collect();
        for p in PACKAGE {
            if !wanted.iter().any(|w| w == p) {
                wanted.push((*p).to_owned());
            }
        }
    }
    let mut rows = Vec::new();
    // per class: the tightest of each measure
    let mut least: BTreeMap<&str, [f64; 4]> = BTreeMap::new();
    let mut findings: BTreeMap<String, usize> = BTreeMap::new();
    for lv in wanted {
        let Ok(bytes) = export::level_partition(&game, &lv) else {
            println!("{lv}: no partition");
            continue;
        };
        let w = LevelWriter::parse(&bytes).unwrap();
        let Ok(found) = levels::load_options(&root, &lv) else {
            println!("{lv}: not prepared");
            continue;
        };
        let t0 = std::time::Instant::now();
        let scene = bc2_scene::scene::load(&found.options).unwrap();
        let skip: HashSet<String> = bc2_levelwrite::modelayer::layer(&w);
        let site = if scene.heightfield.is_some() {
            bc2_levelwrite::site::from_scene(&scene, &skip)
        } else {
            let (hf, water) = export::ground_of(&game, &lv, &w).unwrap();
            bc2_levelwrite::site::with_ground(&scene, &skip, hf, water.as_ref())
        };
        let sp = vehicles::spawners(&w);
        let mut n = 0;
        for s in &sp {
            let Some((class, how)) = s.moving() else { continue };
            n += 1;
            let p = s.rows[3].map(f64::from);
            let a = vehicles::around(&site, p);
            let wet = site
                .water(p[0], p[2])
                .map(|x| x - site.support(p).or(site.ground(p[0], p[2])).unwrap_or(x));
            let near = sp
                .iter()
                .filter(|o| o.guid != s.guid && o.moving().is_some())
                .map(|o| (f64::from(o.rows[3][0]) - p[0]).hypot(f64::from(o.rows[3][2]) - p[2]))
                .fold(f64::INFINITY, f64::min);
            let ground = site.ground(p[0], p[2]);
            let judged = vehicles::water_judged(&site, p);
            let why = vehicles::cannot_leave(&site, class, p);
            if let Some((code, words)) = &why {
                *findings.entry((*code).to_owned()).or_default() += 1;
                println!("  FINDING {lv} {} {class}: {code} {words}", s.guid);
            }
            let l = least.entry(class).or_insert([f64::INFINITY; 4]);
            match how {
                vehicles::Leave::Drive => l[0] = l[0].min(a.way_out),
                vehicles::Leave::Sail if judged => l[1] = l[1].min(a.water_reach),
                vehicles::Leave::Lift => l[2] = l[2].min(a.sky),
                vehicles::Leave::TakeOff => l[3] = l[3].min(a.run.0),
                vehicles::Leave::Sail => {}
            }
            let owner = s.owner.as_ref().map(|o| {
                let d = w
                    .transform(&o.0)
                    .map(|r| (f64::from(r[3][0]) - p[0]).hypot(f64::from(r[3][2]) - p[2]));
                serde_json::json!({"guid": o.0, "base": o.1, "team": o.2, "distance": d})
            });
            let skies = [0.0, 3.5, 7.0].map(|r| site.open_sky(p, r, 60.0));
            let stand = site.stand(p);
            println!(
                "  {lv} {class:6} team {:11} owner {:24} clear {:4.1} drive {:4.1} sail {:5.1} sky {:4.1} run {:5.1} wet {:?} judged {judged} nn {:5.1} y-g {:?}",
                s.team,
                s.owner.as_ref().map_or_else(
                    || "-".to_owned(),
                    |o| format!("{}{}", if o.1 { "base " } else { "point " }, o.2)
                ),
                a.clearance,
                a.way_out,
                a.water_reach,
                a.sky,
                a.run.0,
                wet.map(|x| (x * 10.0).round() / 10.0),
                near,
                ground.map(|g| ((p[1] - g) * 10.0).round() / 10.0),
            );
            rows.push(serde_json::json!({
                "level": lv, "guid": s.guid, "class": class, "classes": s.classes, "team": s.team, "owner": owner,
                "at": p, "forward": s.rows[2], "clearance": a.clearance, "way_out": a.way_out,
                "water_reach": a.water_reach, "sky": a.sky, "run": a.run.0, "run_bearing": a.run.1,
                "water_depth": wet, "water_judged": judged,
                "auto_spawn": s.auto_spawn, "enabled": s.enabled,
                "nearest_vehicle": near.is_finite().then_some(near), "skies": skies, "stand": p[1] - stand,
                "above_ground": ground.map(|g| p[1] - g),
                "finding": why.map(|x| format!("{} {}", x.0, x.1)),
            }));
        }
        println!(
            "{lv}: {} vehicle spawners, {n} of moving vehicles ({:.1} s)",
            sp.len(),
            t0.elapsed().as_secs_f64()
        );
    }
    println!("the tightest of DICE's spawners per class (drive, sail, sky, run):");
    for (c, l) in &least {
        println!("  {c:6} {l:?}");
    }
    println!("findings of XVAL-PLACE-6 to 8 on DICE's spawners: {findings:?}");
    if let Some(out) = value("--json") {
        let text = serde_json::to_string_pretty(&rows).unwrap();
        bc2_formats::atomic::write(std::path::Path::new(&out), &[text.as_bytes()]).unwrap();
    }
}
