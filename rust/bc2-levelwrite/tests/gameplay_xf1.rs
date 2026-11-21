//! Round XF1's typed gameplay operations (`bc2_levelwrite::gameplay`) on a real Conquest level (`MP_001`, Valparaiso):
//! a team set, a spawner unlinked and linked again, the tickets, an outline point inserted and removed; each as a
//! project operation applied from the log, each refusal in words, the project file at version 4 (a version-3 file
//! loads as it is, a newer one is refused).
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); skipped without it.
#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    missing_docs
)]

use std::path::PathBuf;

use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::gameplay::{self, Team, TeamSet, Tickets};
use bc2_levelwrite::project::{ProjectError, VERSION};
use bc2_levelwrite::write::LevelWriter;
use bc2_levelwrite::{MapProject, Op, Profile, Schema};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

fn guid_of(w: &LevelWriter, ty: &[u8], k: usize) -> String {
    let e = w.d.of_type(ty)[k];
    String::from_utf8_lossy(w.d.attr(e, b"guid").unwrap()).to_ascii_uppercase()
}

/// Every point of the outline closes: walking `Next` comes back to the start, and each point's `Previous` is the
/// one before it.
fn closed(w: &LevelWriter, start: &str) -> usize {
    let ring = gameplay::outline(w, start);
    for (i, g) in ring.iter().enumerate() {
        let prev = &ring[(i + ring.len() - 1) % ring.len()];
        let e = w.find(g).unwrap();
        let p = w.d.child(e, b"Previous").and_then(|x| w.d.attr(x, b"ref")).unwrap();
        assert_eq!(String::from_utf8_lossy(p).to_ascii_uppercase(), *prev, "{g}'s Previous");
    }
    let last = w.find(ring.last().unwrap()).unwrap();
    let n = w.d.child(last, b"Next").and_then(|x| w.d.attr(x, b"ref")).unwrap();
    assert_eq!(
        String::from_utf8_lossy(n).to_ascii_uppercase(),
        ring[0],
        "the outline closes"
    );
    ring.len()
}

#[test]
fn typed_gameplay_operations_on_valparaiso() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let base = export::level_partition(&game, "mp_001").unwrap();
    let w0 = LevelWriter::parse(&base).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    // the level's capture points, a spawner of the first, a ticket counter, the combat area's first point
    let point = (0..w0.d.of_type(b".CapturePointEntityData").len())
        .map(|k| guid_of(&w0, b".CapturePointEntityData", k))
        .find(|p| gameplay::spawners(&w0, p).len() >= 2)
        .unwrap();
    let spawners = gameplay::spawners(&w0, &point);
    let counter = guid_of(&w0, b".TicketCounterEntityData", 0);
    let area = w0.d.of_type(b".CombatAreaTriggerEntityData")[0];
    let geo = w0.d.child(area, b"Geometry").unwrap();
    let first_ref = w0.d.attr(w0.d.children(geo).unwrap()[0], b"ref").unwrap();
    let first = String::from_utf8_lossy(first_ref).to_ascii_uppercase();
    let ring = closed(&w0, &first);
    println!(
        "{point}: {} spawners; counter {counter}; the combat area's outline: {ring} points",
        spawners.len()
    );

    // each operation straight on the partition, and its refusals
    let mut w = LevelWriter::parse(&base).unwrap();
    let was = gameplay::team_of(&w, &point).unwrap();
    gameplay::set_team(&mut w, &schema, &point, Team::Team2).unwrap();
    assert_eq!(gameplay::team_of(&w, &point), Some(Team::Team2));
    let e = gameplay::set_team(&mut w, &schema, &first, Team::Team1).unwrap_err();
    assert!(e.to_string().contains("has no team"), "{e}");
    gameplay::link_spawner(&mut w, &point, &spawners[0], false).unwrap();
    assert_eq!(gameplay::spawners(&w, &point), spawners[1..].to_vec());
    let e = gameplay::link_spawner(&mut w, &point, &spawners[0], false).unwrap_err();
    assert!(e.to_string().contains("is not one of"), "{e}");
    gameplay::link_spawner(&mut w, &point, &spawners[0], true).unwrap();
    let e = gameplay::link_spawner(&mut w, &point, &spawners[0], true).unwrap_err();
    assert!(e.to_string().contains("already"), "{e}");
    let e = gameplay::link_spawner(&mut w, &point, &counter, true).unwrap_err();
    assert!(e.to_string().contains("not a soldier spawner"), "{e}");
    let e = gameplay::link_spawner(&mut w, &counter, &spawners[0], true).unwrap_err();
    assert!(e.to_string().contains("not a capture point"), "{e}");
    let t0 = gameplay::tickets_of(&w, &schema, &counter).unwrap();
    assert_eq!(t0.initial, 200, "GAME-TKT-1: DICE's Conquest start");
    gameplay::set_tickets(
        &mut w,
        &schema,
        &counter,
        Tickets {
            initial: 300,
            loss_per_min: 6.0,
        },
    )
    .unwrap();
    assert_eq!(
        gameplay::tickets_of(&w, &schema, &counter),
        Some(Tickets {
            initial: 300,
            loss_per_min: 6.0
        })
    );
    let e = gameplay::set_tickets(
        &mut w,
        &schema,
        &counter,
        Tickets {
            initial: -1,
            loss_per_min: 1.0,
        },
    )
    .unwrap_err();
    assert!(e.to_string().contains("0 or more"), "{e}");
    // an outline point inserted after the area's first and removed again: the partition as it was
    let mut v = LevelWriter::parse(&base).unwrap();
    let mut at = v.transform(&first).unwrap();
    at[3][0] += 5.0;
    let new = bc2_levelwrite::new_guid("xf1-test", 1, &first);
    gameplay::insert_point(&mut v, &first, &new, &at).unwrap();
    assert_eq!(closed(&v, &first), ring + 1);
    assert_eq!(
        gameplay::outline(&v, &first)[1],
        new,
        "the new point follows the one it was inserted after"
    );
    gameplay::remove_point(&mut v, &new).unwrap();
    // the same instances and values as before (the DBX writer keeps the new guid's string in its table)
    let xml = |b: &[u8]| bc2_formats::dbx::to_xml(b).unwrap();
    assert!(
        xml(&v.write().unwrap()) == xml(&base),
        "insert then remove gives the partition's content back"
    );
    // removing the point the area names: the area names its next, the outline one shorter and closed
    let second = gameplay::outline(&v, &first)[1].clone();
    gameplay::remove_point(&mut v, &first).unwrap();
    let area = v.d.of_type(b".CombatAreaTriggerEntityData")[0];
    let geo = v.d.child(area, b"Geometry").unwrap();
    let named = v.d.attr(v.d.children(geo).unwrap()[0], b"ref").unwrap();
    assert_eq!(String::from_utf8_lossy(named).to_ascii_uppercase(), second);
    assert_eq!(closed(&v, &second), ring - 1);
    let e = gameplay::remove_point(&mut v, &point).unwrap_err();
    assert!(e.to_string().contains("not an outline point"), "{e}");

    // the same as project operations: applied from the log, named in the History, kept by the file
    let mut p = MapProject::new("XF1 gameplay", "mp_001", "levels/MP_001", &base, Profile::Bfbc2);
    p.push(
        Op::SetTeam {
            items: vec![TeamSet {
                guid: point.clone(),
                from: Some(was),
                to: Team::Team2,
            }],
        },
        "",
    );
    p.push(
        Op::LinkSpawner {
            point: point.clone(),
            spawner: spawners[0].clone(),
            linked: false,
        },
        "",
    );
    p.push(
        Op::SetTickets {
            counter: counter.clone(),
            from: Some(t0),
            to: Tickets {
                initial: 300,
                loss_per_min: 6.0,
            },
        },
        "",
    );
    let n = p.next_n();
    let new = p.new_guid(&first);
    let _ = n;
    p.push(
        Op::InsertPoint {
            after: first.clone(),
            guid: new.clone(),
            transform: at,
        },
        "",
    );
    p.push(Op::RemovePoint { guid: second.clone() }, "");
    let names: Vec<String> = p.operations.iter().map(bc2_levelwrite::Operation::name).collect();
    println!("{names:#?}");
    assert!(names[0].ends_with("to team 2"), "{names:?}");
    assert!(names[1].starts_with("Unlink spawner"), "{names:?}");
    assert_eq!(names[2], "Set tickets to 300, losing 6 a minute");
    let applied = bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap();
    let w = LevelWriter::parse(&applied.bytes).unwrap();
    assert_eq!(gameplay::team_of(&w, &point), Some(Team::Team2));
    assert_eq!(gameplay::spawners(&w, &point), spawners[1..].to_vec());
    assert_eq!(gameplay::tickets_of(&w, &schema, &counter).unwrap().initial, 300);
    assert_eq!(closed(&w, &first), ring, "one point in, one out");
    assert!(w.find(&second).is_none());
    // the file: version 4; a version-3 file loads as it is; a newer one is refused in words
    assert_eq!(VERSION, 4);
    let text = p.to_json().unwrap();
    let back = MapProject::from_json(&text).unwrap();
    assert_eq!(back.operations, p.operations, "every typed operation kept by the file");
    let raw: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut v3 = raw.clone();
    v3["version"] = serde_json::Value::from(3);
    v3["operations"] = serde_json::Value::Array(Vec::new());
    let old = MapProject::from_json(&serde_json::to_string(&v3).unwrap()).unwrap();
    assert_eq!(old.version, VERSION);
    let mut newer = raw;
    newer["version"] = serde_json::Value::from(VERSION + 1);
    let e = MapProject::from_json(&serde_json::to_string(&newer).unwrap()).unwrap_err();
    assert!(matches!(e, ProjectError::Version(_)), "{e}");
    assert!(e.to_string().contains("made by a newer editor"), "{e}");
}
