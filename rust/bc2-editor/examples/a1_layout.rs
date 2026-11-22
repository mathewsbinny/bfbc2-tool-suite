//! Round A1's proof run: the auto layout on stock levels' terrain without their gameplay, validated, measured against
//! DICE's statistics and compared with DICE's own layout of each level.
//!
//! ```text
//! cargo run --release -p bc2-editor --example a1_layout -- [<level>:<mode>[:<donor>]]... [--seed N] [--json out]
//!     [--opfor <folder>] [--routes <pathdata.hpd>]
//! ```
//!
//! For each level (default: the proof set below): the stock level's partition is a project; the export's target is
//! chosen with another stock level of the mode as the donor (each donor of the era and mode in turn, never the level's
//! own map, unless one is named), which takes the level's own gameplay out; the site is the level's ground, water,
//! collision and art (its own gameplay layer left out); the auto layout proposes; the proposal is pushed as one move
//! operation and `validate::project` plus the site rules validate it. The best donor's proposal is reported: its
//! findings, its statistics inside DICE's ranges, and how close it lands to DICE's own layout (anchor distances,
//! combat area overlap). With `--routes`, the path data of the (one) level given: XVAL-PLACE-5 on the proposal, and on
//! DICE's own spawns, flags and M-COMs for comparison. Nothing is written to the game copy.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::single_match_else
)]

use std::collections::HashSet;
use std::path::PathBuf;

use bc2_editor::levels;
use bc2_levelwrite::autolayout::{self, Ask};
use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::placement::Mode;
use bc2_levelwrite::rules::Severity;
use bc2_levelwrite::write::LevelWriter;
use bc2_levelwrite::{MapProject, Op, Profile};

/// The proof set: prepared Dist levels of each mode.
const LEVELS: &[&str] = &[
    "mp_001:conquest",
    "mp_003:conquest",
    "mp_005:conquest",
    "mp_006cq:conquest",
    "mp_009cq:conquest",
    "nam_mp_002cq:conquest",
    "mp_005gr:goldrush",
    "mp_009gr:goldrush",
    "mp_012gr:goldrush",
    "nam_mp_005r:goldrush",
    "mp_002sr:squadrush",
    "mp_005sr:squadrush",
    "mp_012sr:squadrush",
    "mp_004sdm:teamdeathmatch",
    "mp_007sdm:teamdeathmatch",
    "mp_009sdm:teamdeathmatch",
];

/// A level's map: its name without the mode suffix (`mp_009cq` -> `mp_009`, `ons_mp_004` -> `mp_004`).
fn family(lv: &str) -> String {
    let low = lv.to_ascii_lowercase();
    // an Onslaught level shares its map's terrain (`ons_mp_004` is `mp_004`'s)
    let l = low.strip_prefix("ons_").unwrap_or(&low).to_owned();
    for s in ["cq", "gr", "sr", "sdm", "r"] {
        if let Some(h) = l.strip_suffix(s)
            && h.chars().last().is_some_and(|c| c.is_ascii_digit())
        {
            return h.to_owned();
        }
    }
    l
}

/// The mean art of the site over a layout's combat areas (samples every 8 m).
fn art_in(site: &bc2_levelwrite::site::Site, w: &LevelWriter) -> f64 {
    let polys = bc2_levelwrite::validate::combat_outlines(w);
    let inside = |p: [f64; 2], q: &[[f64; 2]]| {
        let mut r = false;
        for k in 0..q.len() {
            let (a, b) = (q[k], q[(k + 1) % q.len()]);
            if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
                r = !r;
            }
        }
        r
    };
    let (mut sum, mut n) = (0.0, 0.0);
    let mut z = -2048.0;
    while z < 2048.0 {
        let mut x = -2048.0;
        while x < 2048.0 {
            if polys.iter().any(|q| inside([x, z], q)) {
                sum += site.art(x, z);
                n += 1.0;
            }
            x += 8.0;
        }
        z += 8.0;
    }
    if n > 0.0 { sum / n } else { 0.0 }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let root = levels::find_opfor(value("--opfor").as_deref().map(std::path::Path::new)).unwrap();
    let game = Game::open(&root.join("game")).unwrap();
    let seed: u64 = value("--seed").and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut wanted: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| a.contains(':') && !(*i > 0 && args[i - 1].starts_with("--")))
        .map(|x| x.1.clone())
        .collect();
    if wanted.is_empty() {
        wanted = LEVELS.iter().map(|s| (*s).to_owned()).collect();
    }
    let mut report = Vec::new();
    for item in wanted {
        let parts: Vec<&str> = item.split(':').collect();
        let (lv, mode_key) = (parts[0].to_ascii_lowercase(), parts[1]);
        let mode = Mode::of_key(mode_key).unwrap();
        let profile = if lv.starts_with("nam_") {
            Profile::Vietnam
        } else if lv.starts_with("bc1_") {
            Profile::Bfbc1
        } else {
            Profile::Bfbc2
        };
        let t0 = std::time::Instant::now();
        let base = export::level_partition(&game, &lv).unwrap();
        let dice = LevelWriter::parse(&base).unwrap();
        // the site: the level's own ground, without its gameplay layer
        let found = levels::load_options(&root, &lv).unwrap();
        let scene = bc2_scene::scene::load(&found.options).unwrap();
        let skip: HashSet<String> = bc2_levelwrite::modelayer::layer(&dice);
        let site = if scene.heightfield.is_some() {
            bc2_levelwrite::site::from_scene(&scene, &skip)
        } else {
            // the terrain not unpacked for the editor: from the game copy's bundles
            let (hf, water) = export::ground_of(&game, &lv, &dice).unwrap();
            bc2_levelwrite::site::with_ground(&scene, &skip, hf, water.as_ref())
        };
        let t_site = t0.elapsed().as_secs_f64();
        // the level's path data, when given: its route graph, nodes on the ground
        let routes = value("--routes").map(|f| {
            let nav = bc2_editor::navdata::load(std::path::Path::new(&f), scene.heightfield.as_ref()).unwrap();
            let nodes = nav
                .node_pos
                .iter()
                .map(|p| [p[0], p[1] - bc2_editor::navdata::Y_OFFSET, p[2]])
                .collect();
            bc2_levelwrite::site::Routes::new(nodes, nav.node_island.clone(), &f)
        });
        if let Some(r) = &routes {
            let own = bc2_levelwrite::validate::path_rules(&dice, None, r);
            println!(
                "  path data: {} route nodes; DICE's own spawns, flags and M-COMs off the main network: {}",
                r.len(),
                own.len()
            );
        }
        println!("  DICE's combat area: mean art {:.2}", art_in(&site, &dice));
        let donors: Vec<String> = match parts.get(2) {
            Some(d) => vec![(*d).to_owned()],
            None => {
                let ps = bc2_levelwrite::profile::scan(&game, mode_key).unwrap();
                ps.get(profile)
                    .unwrap()
                    .donors
                    .iter()
                    .map(|d| d.level.clone())
                    .filter(|d| family(d) != family(&lv))
                    .collect()
            }
        };
        println!(
            "{lv} ({}): site {:.1} s, {} collision triangles; donors {}",
            mode.word(),
            t_site,
            site.triangles(),
            donors.join(" ")
        );
        let mut best: Option<(serde_json::Value, (usize, usize, usize, f64))> = None;
        for donor in donors {
            let t1 = std::time::Instant::now();
            let mut p = MapProject::new(
                "A1 auto layout",
                &lv,
                &format!("levels/{}", lv.to_ascii_uppercase()),
                &base,
                profile,
            );
            let c = match export::choose_mode(&game, &p, profile, mode_key, Some(&donor)) {
                Ok(c) => c,
                Err(e) => {
                    println!("  {donor}: not chosen: {e}");
                    continue;
                }
            };
            p.push(Op::SetMode { to: Box::new(c) }, "target");
            let (cur, _) = export::current(&game, &p).unwrap();
            let prop = match autolayout::propose_with(
                &cur,
                &site,
                routes.as_ref(),
                Ask::of_choice(p.mode().unwrap(), seed).unwrap(),
            ) {
                Ok(x) => x,
                Err(e) => {
                    println!("  {donor}: no proposal: {e}");
                    continue;
                }
            };
            let t_prop = t1.elapsed().as_secs_f64();
            p.push(prop.op(), &prop.describe());
            let v = &prop.vehicles;
            println!(
                "    vehicles: {} placed ({} of the level's own moved, {} copies), {} links; {}",
                v.vehicles.len(),
                v.moves.len(),
                v.places.len(),
                v.links.len(),
                v.vehicles
                    .iter()
                    .map(|x| format!("{} {:.0} m", x.class, x.distance))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for note in &v.notes {
                println!("      note: {note}");
            }
            let found = bc2_levelwrite::validate::project(&game, &p).unwrap();
            let (ours, _) = export::current(&game, &p).unwrap();
            // the site rules with the collision too (validate::project reads the ground and water only): each finding once
            let site_found: Vec<bc2_levelwrite::rules::Finding> =
                bc2_levelwrite::validate::site_rules(&ours, Some(&dice), &site)
                    .into_iter()
                    .filter(|f| !found.contains(f))
                    .collect();
            let n = |s: Severity| found.iter().chain(&site_found).filter(|f| f.severity == s).count();
            let (e, w, no) = (n(Severity::Error), n(Severity::Warning), n(Severity::Note));
            let off = routes
                .as_ref()
                .map(|r| bc2_levelwrite::validate::path_rules(&ours, Some(&dice), r));
            if let Some(off) = &off {
                println!(
                    "    path data: {} of the proposal's objects off the main network",
                    off.len()
                );
                for f in off.iter().take(5) {
                    println!("      {}", f.line());
                }
            }
            let (ins, all) = prop.inside();
            let cmp = autolayout::compare(&dice, &ours, mode);
            let art = art_in(&site, &ours);
            println!(
                "  {donor}: {:.1} s, {} candidates, turn {:.0}, scale {:.2}, {} moved ({} clusters, {} points fitted, {} bad); \
                 validate: {e} errors, {w} warnings, {no} notes; stats {ins}/{all} inside DICE's; vs DICE: mean {:.0} m, IoU {:.2}, inside DICE's area {:.0} %, art {:.2}",
                t_prop,
                prop.candidates,
                prop.turn,
                prop.scale,
                prop.moves.len(),
                prop.clusters_fitted,
                prop.points_fitted,
                prop.bad_spots,
                cmp.mean_distance,
                cmp.iou,
                cmp.inside_dice * 100.0,
                art
            );
            for f in found
                .iter()
                .chain(&site_found)
                .filter(|f| f.severity >= Severity::Warning)
            {
                println!("      {}", f.line());
            }
            // the donor is chosen without DICE's own layout: findings, statistics, then the ground score
            let key = (e + w, all - ins, no, prop.score);
            let v = serde_json::json!({
                "level": lv, "mode": mode.word(), "donor": donor, "seed": seed,
                "errors": e, "warnings": w, "notes": no,
                "stats_inside": ins, "stats": all,
                "turn": prop.turn, "scale": prop.scale, "moved": prop.moves.len(),
                "clusters_fitted": prop.clusters_fitted, "points_fitted": prop.points_fitted, "bad": prop.bad_spots,
                "seconds": t_prop, "candidates": prop.candidates, "vehicles": prop.vehicles.vehicles,
                "vehicle_notes": prop.vehicles.notes,
                "compare": cmp, "stat_rows": prop.stats, "score": prop.score, "art": art,
                "off_network": off.as_ref().map(Vec::len),
                "dice_stats": autolayout::stats(&bc2_levelwrite::placement::measure(&dice, mode), mode, profile),
                "findings": found.iter().chain(&site_found).map(bc2_levelwrite::rules::Finding::line).collect::<Vec<_>>(),
            });
            if best.as_ref().is_none_or(|b| {
                (key.0, key.1, key.2) < (b.1.0, b.1.1, b.1.2)
                    || ((key.0, key.1, key.2) == (b.1.0, b.1.1, b.1.2) && key.3 < b.1.3)
            }) {
                best = Some((v, key));
            }
        }
        if let Some((v, _)) = best {
            println!(
                "  best: {} ({} errors, {} warnings, {} notes, {}/{} inside)",
                v["donor"], v["errors"], v["warnings"], v["notes"], v["stats_inside"], v["stats"]
            );
            report.push(v);
        }
    }
    if let Some(out) = value("--json") {
        let text = serde_json::to_string_pretty(&report).unwrap();
        bc2_formats::atomic::write(&PathBuf::from(out), &[text.as_bytes()]).unwrap();
    }
    let _ = Mode::Conquest;
}
