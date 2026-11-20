//! The AI workspace against the Navigation Studio (round GR2, recorded before the Studio was retired): on `SP_01`,
//! `ONS_MP_004`, `ONS_MP_002` and `SP_009`, for NavKit's build and DICE's file each, the editor reads the same
//! navigation data as `navdata.load` (summary, areas, cells and their heights, route-graph islands), colours the
//! ground into the same classes in each of the Studio's colourings (island, sector, pitch, difference, anomaly), gives
//! the same difference figures, soldier spawns' islands and islands' distances from the main network, the same anomaly
//! overlay (`overlay.anomalies`) and the same path tests (`navpath.route`); and the compare overlay
//! (`overlay.reference_report`) reads the same AI layers and lookups for `ONS_MP_004` and `MP_002` against
//! `level:ons_mp_002`, `SP_009` against `level:sp_01`, and the WW2 level `GUADAL_CANAL`'s own layer from its
//! partition.
//!
//! The Studio's answers are digests (`tests/data/navstudio.json.gz`, `tests/record_navstudio.py`): each list's length
//! and the SHA-1 of its compact JSON, floats as fixed-point text. Skipped (with a note) without the game copy.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    missing_docs
)]

use std::io::Read;
use std::time::Instant;

use bc2_editor::levels;
use bc2_editor::nav;
use bc2_editor::navdata::{self, Anomaly, NavData};
use bc2_editor::navview::{self, ColourMode};
use bc2_scene::scene;
use serde_json::Value;

/// A JSON value with its keys in order (Python's dicts), for the digests.
#[derive(Debug, Clone)]
enum J {
    Null,
    B(bool),
    I(i64),
    S(String),
    A(Vec<J>),
    O(Vec<(&'static str, J)>),
}

impl J {
    fn text(&self, out: &mut String) {
        match self {
            J::Null => out.push_str("null"),
            J::B(b) => out.push_str(if *b { "true" } else { "false" }),
            J::I(i) => out.push_str(&i.to_string()),
            J::S(s) => out.push_str(&serde_json::to_string(s).unwrap()),
            J::A(v) => {
                out.push('[');
                for (k, x) in v.iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    x.text(out);
                }
                out.push(']');
            }
            J::O(v) => {
                out.push('{');
                for (k, (n, x)) in v.iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(n).unwrap());
                    out.push(':');
                    x.text(out);
                }
                out.push('}');
            }
        }
    }
}

fn digest_of(items: &[J]) -> Value {
    let mut s = String::new();
    J::A(items.to_vec()).text(&mut s);
    serde_json::json!([items.len(), sha1_smol::Sha1::from(s.as_bytes()).digest().to_string()])
}

fn f3(v: f64) -> J {
    J::S(format!("{v:.3}"))
}

fn f2(v: f64) -> J {
    J::S(format!("{v:.2}"))
}

fn i(v: usize) -> J {
    J::I(v as i64)
}

fn opt<T>(v: Option<T>, f: impl Fn(T) -> J) -> J {
    v.map_or(J::Null, f)
}

fn s(v: &str) -> J {
    J::S(v.to_owned())
}

fn pos(p: [f64; 3]) -> J {
    J::A(p.iter().map(|v| f3(*v)).collect())
}

fn recorded() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("navstudio.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn anomaly(a: &Anomaly) -> J {
    match a {
        Anomaly::SmallIsland {
            island,
            pos: p,
            nodes,
            cells,
        } => J::O(vec![
            ("island", i(*island)),
            ("pos", pos(*p)),
            ("nodes", i(*nodes)),
            ("cells", i(*cells)),
        ]),
        Anomaly::SpawnerOnSmallIsland {
            pos: p,
            name,
            player_type,
            enabled,
            island_size,
            node_m,
        } => J::O(vec![
            ("pos", pos(*p)),
            ("name", s(name)),
            ("player_type", opt(player_type.as_deref(), s)),
            ("enabled", J::B(*enabled)),
            ("island_size", i(*island_size)),
            ("node_m", f3(*node_m)),
        ]),
        Anomaly::OrderWithoutNode { kind, name, pos: p } => {
            J::O(vec![("kind", s(kind)), ("name", s(name)), ("pos", pos(*p))])
        }
        Anomaly::SliverArea {
            area,
            sector,
            cells,
            pos: p,
        } => J::O(vec![
            ("area", i(*area)),
            ("sector", J::I(i64::from(*sector))),
            ("cells", i(*cells)),
            ("pos", pos(*p)),
        ]),
        Anomaly::LonelyNode { node, pos: p } => J::O(vec![("node", i(*node)), ("pos", pos(*p))]),
    }
}

fn file_digests(name: &str, nav: &NavData, same: Option<&[bool]>, spawns: &[(String, [f64; 3])]) -> Value {
    let sm = navdata::summary(nav);
    let mut out = serde_json::json!({
        "file": nav.path.file_name().unwrap().to_string_lossy(),
        "summary": {
            "areas": sm.areas, "detailed areas": sm.detailed_areas, "cells": sm.cells, "nodes": sm.nodes,
            "edges": sm.edges, "islands": sm.islands, "large islands (25+ nodes)": sm.large_islands,
            "sectors": sm.sectors, "ladder links": sm.ladder_links, "ladder ground cells": sm.ladder_ground_cells,
        },
    });
    let areas: Vec<J> = nav
        .areas
        .iter()
        .map(|a| {
            J::A(vec![
                J::I(i64::from(a.sector)),
                i(a.index),
                J::I(a.tile.0),
                J::I(a.tile.1),
                J::I(a.layer),
                J::S(format!("{:.4}", a.pitch)),
                f3(a.min_y),
                f3(a.max_y),
                i(a.cells),
                J::I(a.island),
                J::I(i64::from(a.ladder)),
                i(a.outline.len()),
                i(a.portals.len()),
                i(a.borders.len()),
            ])
        })
        .collect();
    out["areas"] = digest_of(&areas);
    let cells: Vec<J> = nav
        .cell_ij
        .iter()
        .zip(&nav.cell_area)
        .map(|(c, a)| J::A(vec![J::I(c[0]), J::I(c[1]), J::I(i64::from(*a))]))
        .collect();
    out["cells"] = digest_of(&cells);
    let ys: Vec<J> = nav
        .cell_y
        .iter()
        .map(|y| J::I((y * 100.0).round_ties_even() as i64))
        .collect();
    out["cell_y"] = digest_of(&ys);
    out["node_island"] = digest_of(&nav.node_island.iter().map(|v| J::I(i64::from(*v))).collect::<Vec<_>>());
    let isl: Vec<J> = nav
        .islands
        .iter()
        .map(|x| {
            J::A(vec![
                i(x.nodes),
                i(x.cells),
                f2(x.centre[0]),
                f2(x.centre[1]),
                f2(x.centre[2]),
            ])
        })
        .collect();
    out["islands"] = digest_of(&isl);
    let mut colours = serde_json::Map::new();
    for m in ColourMode::ALL {
        let c = navview::canonical(&navview::classes(nav, m, same, name == "DICE"));
        let key = m.name().to_lowercase();
        colours.insert(
            key,
            digest_of(&c.iter().map(|v| J::I(i64::from(*v))).collect::<Vec<_>>()),
        );
    }
    out["colours"] = Value::Object(colours);
    let mut sp: Vec<(String, J)> = spawns
        .iter()
        .map(|(g, p)| {
            let (isl, d) = navdata::spawn_island(nav, *p);
            (
                g.clone(),
                J::A(vec![s(g), opt(isl, |v| J::I(i64::from(v))), opt(d, f2)]),
            )
        })
        .collect();
    sp.sort_by(|a, b| a.0.cmp(&b.0));
    out["spawns"] = digest_of(&sp.into_iter().map(|(_, j)| j).collect::<Vec<_>>());
    out["gaps"] = digest_of(&navdata::island_gaps(nav, 300).into_iter().map(f2).collect::<Vec<_>>());
    out
}

fn layer_rows(c: &nav::Compare, navkit: Option<&NavData>) -> serde_json::Map<String, Value> {
    let idx = navkit.map(navdata::NavIndex::new);
    let row = |p: [f64; 3]| {
        let r = navdata::node_row(idx.as_ref(), p);
        vec![
            ("navkit_ground", opt(r.navkit_ground, J::B)),
            ("nearest_node_m", opt(r.nearest_node_m, f3)),
            ("node_island_size", opt(r.node_island_size, i)),
            ("node_on_main_island", opt(r.node_on_main_island, J::B)),
        ]
    };
    let l = &c.layer;
    let mut out = serde_json::Map::new();
    let sp: Vec<J> = l
        .spawners
        .iter()
        .map(|x| {
            let mut v = vec![
                ("pos", pos(x.pos)),
                ("name", s(&x.name)),
                ("player_type", opt(x.player_type.as_deref(), s)),
                ("enabled", J::B(x.enabled)),
                ("team", opt(x.team.as_deref(), s)),
                ("squad_id", opt(x.squad_id.as_deref(), s)),
                ("in_squad_entity", J::B(x.in_squad_entity)),
            ];
            v.extend(row(x.pos));
            J::O(v)
        })
        .collect();
    out.insert("spawners".into(), digest_of(&sp));
    let ao: Vec<J> = l
        .area_orders
        .iter()
        .map(|x| {
            let mut v = vec![
                ("pos", pos(x.pos)),
                ("name", s(&x.name)),
                ("area_type", opt(x.area_type.as_deref(), s)),
            ];
            v.extend(row(x.pos));
            J::O(v)
        })
        .collect();
    out.insert("area_orders".into(), digest_of(&ao));
    let pp: Vec<J> = l
        .patrols
        .iter()
        .flat_map(|p| p.points.iter().map(move |q| (p, *q)))
        .map(|(p, q)| {
            let mut v = vec![("patrol", s(&p.name)), ("pos", pos(q))];
            v.extend(row(q));
            J::O(v)
        })
        .collect();
    out.insert("patrol_points".into(), digest_of(&pp));
    let ob: Vec<J> = l
        .objectives
        .iter()
        .map(|x| {
            let mut v = vec![("pos", pos(x.pos)), ("type", s(&x.kind))];
            v.extend(row(x.pos));
            J::O(v)
        })
        .collect();
    out.insert("objectives".into(), digest_of(&ob));
    out
}

fn check_compare(want: &Value, c: &nav::Compare, navkit: Option<&NavData>) {
    let what = format!("{} against {}", want["target"], want["source"]);
    assert_eq!(serde_json::json!(c.layer.has_ai), want["has_ai"], "{what}: HasAI");
    assert_eq!(
        serde_json::json!(c.offsets.map(|(x, z)| [x, z])),
        want["sector_offsets"],
        "{what}: SectorOffsets"
    );
    let l = &c.layer;
    let summary = serde_json::json!({
        "spawners": l.spawners.len(),
        "ai_spawners_enabled": l.spawners.iter().filter(|s| s.enabled && s.player_type.as_deref() == Some("SSAiPlayer")).count(),
        "area_orders": l.area_orders.len(), "patrols": l.patrols.len(), "objectives": l.objectives.len(),
    });
    assert_eq!(summary, want["summary"], "{what}: counts");
    let rows = layer_rows(c, navkit);
    for (k, v) in &rows {
        assert_eq!(v, &want["rows"][k], "{what}: {k}");
    }
    let patrols: Vec<J> = l
        .patrols
        .iter()
        .map(|p| {
            J::O(vec![
                ("name", s(&p.name)),
                ("points", J::A(p.points.iter().map(|q| pos(*q)).collect())),
            ])
        })
        .collect();
    assert_eq!(digest_of(&patrols), want["patrols"], "{what}: patrols");
}

#[test]
fn navigation_as_the_studio_showed_it() {
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let rec = recorded();
    let mut checked = 0;
    for (lv, want) in rec["levels"].as_object().unwrap() {
        let found = match levels::load_options(&root, lv) {
            Ok(f) => f,
            Err(e) => {
                println!("skipped {lv}: {e}");
                continue;
            }
        };
        let sc = scene::load(&found.options).unwrap();
        let spawns: Vec<(String, [f64; 3])> = sc
            .entities
            .iter()
            .filter(|e| e.category == "soldier spawn")
            .filter_map(|e| e.matrix.map(|m| (e.guid.clone(), [m[12], m[13], m[14]])))
            .collect();
        assert_eq!(
            spawns.len() as u64,
            want["spawns"].as_u64().unwrap(),
            "{lv}: soldier spawns"
        );
        let (cand, dice) = navdata::default_files(&root, lv);
        let t0 = Instant::now();
        let inputs = nav::Inputs {
            navkit: cand,
            dice,
            heightfield: sc.heightfield.clone(),
            partition: sc.file(0).map(|(_, d)| d.to_vec()),
            spawns: spawns.iter().map(|(_, p)| *p).collect(),
        };
        let loaded = nav::read(&inputs).unwrap();
        let secs = t0.elapsed().as_secs_f64();
        let names: Vec<&str> = loaded.files.iter().map(|f| f.name).collect();
        let want_names: Vec<&str> = want["files"].as_object().unwrap().keys().map(String::as_str).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, want_names, "{lv}: files");
        for f in &loaded.files {
            let got = file_digests(f.name, &f.data, loaded.same_of(f.name), &spawns);
            let w = &want["files"][f.name];
            for key in [
                "file",
                "summary",
                "areas",
                "cells",
                "cell_y",
                "node_island",
                "islands",
                "colours",
                "spawns",
                "gaps",
            ] {
                assert_eq!(got[key], w[key], "{lv} {}: {key}", f.name);
            }
        }
        match (&loaded.diff, &loaded.same) {
            (Some(d), Some((c, r))) => {
                let w = &want["difference"];
                assert_eq!(
                    format!("{:.3}", d.reproduced),
                    w["reproduced"].as_str().unwrap(),
                    "{lv}: reproduced"
                );
                assert_eq!(format!("{:.3}", d.extra), w["extra"].as_str().unwrap(), "{lv}: extra");
                assert_eq!(
                    format!("{:.3}", d.ladder),
                    w["ladder"].as_str().unwrap(),
                    "{lv}: ladder"
                );
                let b = |v: &[bool]| digest_of(&v.iter().map(|x| J::I(i64::from(*x))).collect::<Vec<_>>());
                assert_eq!(b(c), w["same_navkit"], "{lv}: NavKit's cells DICE has");
                assert_eq!(b(r), w["same_dice"], "{lv}: DICE's cells NavKit has");
            }
            _ => assert!(want["difference"].is_null(), "{lv}: difference"),
        }
        if let Some(an) = &loaded.anomalies {
            let w = &want["anomalies"];
            for (key, list) in [
                ("small_islands", &an.small_islands),
                ("spawners_on_small_islands", &an.spawners_on_small_islands),
                ("orders_without_node", &an.orders_without_node),
                ("sliver_areas", &an.sliver_areas),
                ("lonely_nodes", &an.lonely_nodes),
            ] {
                assert_eq!(
                    digest_of(&list.iter().map(anomaly).collect::<Vec<_>>()),
                    w[key],
                    "{lv}: anomalies {key}"
                );
            }
        }
        // the path tests
        for (k, p) in want["paths"].as_array().unwrap().iter().enumerate() {
            let pt = |v: &Value| -> [f64; 3] {
                let a = v.as_array().unwrap();
                [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()]
            };
            let (a, b) = (pt(&p["a"]), pt(&p["b"]));
            let mut routes = Vec::new();
            for f in &loaded.files {
                let r = navdata::route(&f.data, &f.graph, a, b);
                let w = &p["routes"][f.name];
                let got = serde_json::json!({
                    "found": r.found,
                    "nodes": digest_of(&r.nodes.iter().map(|n| i(*n)).collect::<Vec<_>>()),
                    "cost": format!("{:.3}", r.cost), "length": format!("{:.3}", r.length),
                    "straight": format!("{:.3}", r.straight), "start_node": r.start_node, "end_node": r.end_node,
                    "start_join": format!("{:.2}", r.start_join.min(1e9)), "end_join": format!("{:.2}", r.end_join.min(1e9)),
                    "start_island": r.start_island, "end_island": r.end_island, "reason": r.reason,
                    "gap": r.gap.map(|g| serde_json::json!([g.0, g.1, format!("{:.2}", g.2)])),
                });
                assert_eq!(&got, w, "{lv}: path test {k} over {}", f.name);
                routes.push((f.name, r));
            }
            let get = |n: &str| routes.iter().find(|(f, _)| *f == n).map(|(_, r)| r);
            let change = navdata::route_cost_change(get("NavKit"), get("DICE")).map(|c| format!("{c:.2}"));
            assert_eq!(
                serde_json::json!(change),
                p["change"],
                "{lv}: path test {k}: the cost change"
            );
        }
        println!(
            "{lv}: {} as the Navigation Studio showed them ({}); {} path tests; read in {secs:.1} s",
            names.join(" and "),
            loaded
                .files
                .iter()
                .map(|f| format!(
                    "{} {} cells, {} islands",
                    f.name,
                    f.data.cell_ij.len(),
                    f.data.islands.len()
                ))
                .collect::<Vec<_>>()
                .join(", "),
            want["paths"].as_array().unwrap().len()
        );
        checked += 1;
    }
    // the compare overlays
    for w in rec["compares"].as_array().unwrap() {
        let target = w["target"].as_str().unwrap();
        let c = nav::open_compare(w["source"].as_str().unwrap(), &root).unwrap();
        let (cand, _) = navdata::default_files(&root, target);
        assert_eq!(
            serde_json::json!(
                cand.as_ref()
                    .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            ),
            w["candidate"],
            "{target}: NavKit's build"
        );
        let navkit = cand.map(|p| navdata::load(&p, None).unwrap());
        check_compare(w, &c, navkit.as_ref());
        println!("compare {target} with {}: the same AI layer and lookups", w["source"]);
    }
    let ww2 = &rec["ww2"];
    if !ww2.is_null() {
        let part = root
            .join("work")
            .join("editor")
            .join("levels")
            .join("guadal_canal")
            .join("00_guadal_canal")
            .join("levels")
            .join("guadal_canal.dbx");
        let c = nav::open_compare(&part.display().to_string(), &root).unwrap();
        check_compare(ww2, &c, None);
        println!("guadal_canal (WW2): its own AI layer as the Studio's overlay read it");
    }
    println!(
        "{checked} of {} levels compared with the Navigation Studio",
        rec["levels"].as_object().unwrap().len()
    );
}
