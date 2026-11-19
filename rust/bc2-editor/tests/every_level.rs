//! levelwrite's proof over every prepared stock level (round E2b): NavKit's prepared levels in OPFOR's
//! `work\levels` and every level G3 lists from `game\Package` (Vietnam, Pacific War, the Bad Company 1 maps and
//! DICE's other Package-only layouts; their partitions read from their `level-00.fbrb` in memory, the bytes G3's
//! prepare writes). OPFOR's files are only read; nothing is written anywhere.
//!
//! On each level a scripted log - a move, a rotate, a duplicate, a deep duplicate (a flag with its capture area,
//! else the first object that owns something), two placements (the most and the least placed blueprint), a delete,
//! a decimal field set, a hide and a lock - is applied, and the edited partition:
//!   (a) differs from the base exactly where the log says (field-level diff against the expected set: the copies and
//!       placements added, the deleted removed, the moved, turned and set fields, `Objects`, the event connections
//!       that name the copied or deleted objects, every field that named the deleted object);
//!   (b) parses again and writes back byte for byte (`EditDbx`), and reads as a tree;
//!   (c) is the same bytes when the log is replayed again (and after a JSON round trip of the log);
//!   (d) has the deep copies' references among themselves remapped (none to an original it copied).
//!
//! Skipped (with a note) without Project OPFOR BFBC2.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::panic,
    clippy::many_single_char_names,
    clippy::type_complexity,
    clippy::map_unwrap_or,
    clippy::assert_is_empty,
    missing_docs
)]

use std::collections::BTreeSet;
use std::path::Path;

use bc2_editor::factions::partition_from_bundle;
use bc2_editor::{levels, package};
use bc2_formats::dbx::CONTAINER;
use bc2_formats::dbx::edit::EditDbx;
use bc2_levelwrite::diff::{Change, diff};
use bc2_levelwrite::ops::{Duplicated, FieldSet, Moved, Placed, SubCopy};
use bc2_levelwrite::write::refers_to;
use bc2_levelwrite::{FieldType, FieldValue, LevelWriter, Op, Operation, Schema, apply};

fn up(b: &[u8]) -> String {
    String::from_utf8_lossy(b).to_ascii_uppercase()
}

/// The level partition NavKit prepared (a Package overlay first).
fn prepared(root: &Path, lv: &str) -> Option<Vec<u8>> {
    let work = root.join("work").join("levels").join(lv);
    for b in ["pkg", "dist"] {
        for e in std::fs::read_dir(work.join(b).join("levels")).ok()?.flatten() {
            let p = e.path();
            if p.file_stem()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(lv))
                && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dbx"))
            {
                return std::fs::read(p).ok();
            }
        }
    }
    None
}

/// The instances (and their top fields) that name a guid anywhere below them.
fn referrers(d: &EditDbx, guid: &str) -> BTreeSet<Change> {
    let mut out = BTreeSet::new();
    for inst in d.instances() {
        let g = up(d.attr(inst, b"guid").unwrap_or_default());
        for &c in d.children(inst).unwrap_or_default() {
            if d.walk(c)
                .iter()
                .any(|&x| d.attr(x, b"ref").is_some_and(|r| refers_to(r, guid.as_bytes())))
            {
                let name = String::from_utf8_lossy(d.attr(c, b"name").unwrap_or_default()).into_owned();
                out.insert(Change::Field(g.clone(), name));
            }
        }
    }
    out
}

/// The `EventConnections` arrays with a connection naming any of these guids.
fn connections_naming(d: &EditDbx, guids: &[String]) -> BTreeSet<Change> {
    let mut out = BTreeSet::new();
    for inst in d.instances() {
        let Some(a) = d.child(inst, b"EventConnections").filter(|&a| d.kind(a) == CONTAINER) else {
            continue;
        };
        let hit = d.children(a).unwrap_or_default().iter().any(|&c| {
            [&b"Source"[..], b"Target"].iter().any(|f| {
                d.child(c, f)
                    .and_then(|x| d.attr(x, b"ref"))
                    .is_some_and(|r| guids.iter().any(|g| refers_to(r, g.as_bytes())))
            })
        });
        if hit {
            out.insert(Change::Field(
                up(d.attr(inst, b"guid").unwrap_or_default()),
                "EventConnections".into(),
            ));
        }
    }
    out
}

fn op(n: u64, op: Op) -> Operation {
    Operation {
        n,
        by: "proof".into(),
        when: "2026-11-19T00:00:00Z".into(),
        label: String::new(),
        op,
    }
}

fn moved(r: &bc2_levelwrite::Rows, dx: f32) -> bc2_levelwrite::Rows {
    let mut t = *r;
    t[3][0] += dx;
    t
}

/// The scripted log for a level, and the changes it must make. `None` when the level has too few placed objects.
fn script(base: &[u8]) -> Option<(Vec<Operation>, BTreeSet<Change>, Schema, Vec<(String, String)>)> {
    let w = LevelWriter::parse(base).ok()?;
    let d = &w.d;
    let s = Schema::scan([d]);
    let project = "every-level-proof";
    let ld = w.level_data()?;
    let ld_guid = up(d.attr(ld, b"guid").unwrap_or_default());
    let in_objects: BTreeSet<String> = d
        .child(ld, b"Objects")
        .and_then(|o| d.children(o))
        .unwrap_or_default()
        .iter()
        .filter_map(|&i| d.attr(i, b"ref"))
        .map(up)
        .collect();
    // placed objects with a transform, in Objects
    let placed: Vec<String> = d
        .of_type(b".ReferenceObjectData")
        .into_iter()
        .map(|e| up(d.attr(e, b"guid").unwrap_or_default()))
        .filter(|g| in_objects.contains(g) && w.transform(g).is_some())
        .collect();
    if placed.len() < 5 {
        return None;
    }
    let (mv, rot, dup, del) = (
        placed[0].clone(),
        placed[1].clone(),
        placed[2].clone(),
        placed[3].clone(),
    );
    let mut ops = Vec::new();
    let mut expected = BTreeSet::new();
    let mut n = 0;
    let mut next = || {
        n += 1;
        n
    };
    // move and rotate
    let t = w.transform(&mv)?;
    ops.push(op(
        next(),
        Op::Move {
            items: vec![Moved {
                guid: mv.clone(),
                from: t,
                to: moved(&t, 3.0),
                placement: None,
            }],
        },
    ));
    expected.insert(Change::Field(mv.clone(), "Transform".into()));
    let t = w.transform(&rot)?;
    let turned = [[0.0, 0.0, -1.0], t[1], [1.0, 0.0, 0.0], t[3]];
    ops.push(op(
        next(),
        Op::Rotate {
            items: vec![Moved {
                guid: rot.clone(),
                from: t,
                to: turned,
                placement: None,
            }],
        },
    ));
    expected.insert(Change::Field(rot.clone(), "Transform".into()));
    // a plain duplicate
    let k = next();
    let copy = bc2_levelwrite::new_guid(project, k, &dup);
    ops.push(op(
        k,
        Op::Duplicate {
            items: vec![Duplicated {
                source: dup.clone(),
                guid: copy.clone(),
                transform: moved(&w.transform(&dup)?, 4.0),
                owned: Vec::new(),
            }],
        },
    ));
    expected.insert(Change::Added(copy.to_ascii_uppercase()));
    let mut connected = vec![dup.clone()];
    // a deep duplicate: a flag, else the first object (of the first 400) that owns something
    let candidates: Vec<String> = d
        .of_type(b".CapturePointEntityData")
        .into_iter()
        .chain(d.instances().into_iter().take(400))
        .map(|e| up(d.attr(e, b"guid").unwrap_or_default()))
        .filter(|g| *g != ld_guid && ![&mv, &rot, &dup, &del].contains(&g))
        .collect();
    let mut deep_pairs = Vec::new();
    let graph = w.ownership();
    if let Some((owner, owned)) = candidates.iter().find_map(|g| {
        let o = graph.owned(g);
        (!o.is_empty() && !o.iter().any(|x| [&mv, &rot, &dup, &del].contains(&x))).then(|| (g.clone(), o))
    }) {
        let k = next();
        let guid = bc2_levelwrite::new_guid(project, k, &owner);
        let subs: Vec<SubCopy> = owned
            .iter()
            .map(|s| SubCopy {
                source: s.clone(),
                guid: bc2_levelwrite::new_guid(project, k, s),
                transform: w.transform(s).map(|t| moved(&t, 6.0)),
            })
            .collect();
        expected.insert(Change::Added(guid.to_ascii_uppercase()));
        deep_pairs.push((owner.clone(), guid.clone()));
        for sc in &subs {
            expected.insert(Change::Added(sc.guid.to_ascii_uppercase()));
            deep_pairs.push((sc.source.clone(), sc.guid.clone()));
        }
        connected.push(owner.clone());
        connected.extend(owned.iter().cloned());
        let transform = w
            .transform(&owner)
            .map_or_else(|| bc2_levelwrite::rows_at([0.0; 3]), |t| moved(&t, 6.0));
        if w.transform(&owner).is_some() {
            // and the same owner placed as a gameplay tile (the asset browser's Spawners / Objectives): a copy with
            // what it owns, the team field kept, only the connections among them copied
            let kp = k + 100;
            let pguid = bc2_levelwrite::new_guid(project, kp, &owner);
            let psubs: Vec<SubCopy> = owned
                .iter()
                .map(|s| SubCopy {
                    source: s.clone(),
                    guid: bc2_levelwrite::new_guid(project, kp, s),
                    transform: w.transform(s).map(|t| moved(&t, 9.0)),
                })
                .collect();
            expected.insert(Change::Added(pguid.to_ascii_uppercase()));
            deep_pairs.push((owner.clone(), pguid.clone()));
            for sc in &psubs {
                expected.insert(Change::Added(sc.guid.to_ascii_uppercase()));
                deep_pairs.push((sc.source.clone(), sc.guid.clone()));
            }
            let ptransform = moved(&w.transform(&owner)?, 9.0);
            ops.push(op(
                k,
                Op::Duplicate {
                    items: vec![Duplicated {
                        source: owner.clone(),
                        guid,
                        transform,
                        owned: subs,
                    }],
                },
            ));
            ops.push(op(
                kp,
                Op::Place {
                    items: vec![Placed {
                        guid: pguid,
                        template: owner,
                        blueprint: String::new(),
                        ref_field: None,
                        owned: psubs,
                        transform: ptransform,
                        fields: Vec::new(),
                        placement: None,
                    }],
                },
            ));
        } else {
            // an owner without a transform is not copied by the editor (no place to put it)
            for sc in &subs {
                expected.remove(&Change::Added(sc.guid.to_ascii_uppercase()));
            }
            expected.remove(&Change::Added(deep_pairs.first()?.1.to_ascii_uppercase()));
            deep_pairs.clear();
            connected.truncate(1);
        }
    }
    // two placements: the most and the least placed blueprints
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for e in d.of_type(b".ReferenceObjectData") {
        if let Some(r) = d.child(e, b"ReferencedObject").and_then(|c| d.attr(c, b"ref"))
            && r.contains(&b'/')
        {
            *counts.entry(String::from_utf8_lossy(r).to_lowercase()).or_default() += 1;
        }
    }
    let most = counts.iter().max_by_key(|(_, c)| **c)?.0.clone();
    let least = counts.iter().min_by_key(|(_, c)| **c)?.0.clone();
    let mut items = Vec::new();
    let k = next();
    for bp in [&most, &least] {
        let template = w.template_for(bp)?;
        let fields = w.place_defaults(&s, &template, bp);
        let guid = bc2_levelwrite::new_guid(project, k, bp);
        if items.iter().any(|p: &Placed| p.guid == guid) {
            continue;
        }
        expected.insert(Change::Added(guid.to_ascii_uppercase()));
        items.push(Placed {
            guid,
            template,
            blueprint: bp.clone(),
            transform: bc2_levelwrite::rows_at([1.0, 2.0, 3.0]),
            fields,
            ref_field: None,
            owned: Vec::new(),
            placement: None,
        });
    }
    ops.push(op(k, Op::Place { items }));
    // a delete
    ops.push(op(
        next(),
        Op::Delete {
            guids: vec![del.clone()],
        },
    ));
    expected.insert(Change::Removed(del.clone()));
    for r in referrers(d, &del) {
        if let Change::Field(g, _) = &r
            && *g != del
        {
            expected.insert(r);
        }
    }
    // a decimal field set on another object
    let used: Vec<&String> = vec![&mv, &rot, &dup, &del];
    let deep_sources: Vec<String> = deep_pairs.iter().map(|(s, _)| s.clone()).collect();
    let set = d.instances().into_iter().find_map(|e| {
        let g = up(d.attr(e, b"guid").unwrap_or_default());
        if used.contains(&&g) || deep_sources.contains(&g) || g == ld_guid {
            return None;
        }
        w.fields(&s, &g).into_iter().find_map(|(path, ty, v)| match (ty, v) {
            (FieldType::Float, Some(FieldValue::Float(f))) if !path.contains('.') => Some((g.clone(), path, f)),
            _ => None,
        })
    });
    if let Some((g, path, f)) = set {
        ops.push(op(
            next(),
            Op::SetField {
                items: vec![FieldSet {
                    guid: g.clone(),
                    field: path.clone(),
                    from: Some(FieldValue::Float(f)),
                    to: FieldValue::Float(f + 1.0),
                }],
            },
        ));
        expected.insert(Change::Field(g, path));
    }
    // the organisation's: no partition change
    ops.push(op(
        next(),
        Op::Hide {
            guids: vec![mv.clone()],
            hidden: true,
        },
    ));
    ops.push(op(
        next(),
        Op::Lock {
            guids: vec![rot],
            locked: true,
        },
    ));
    // Objects grows; the connections naming the copied or deleted objects change
    expected.insert(Change::Field(ld_guid, "Objects".into()));
    connected.push(del);
    expected.extend(connections_naming(d, &connected));
    Some((ops, expected, s, deep_pairs))
}

/// One level's proof; its line for the table.
fn prove(lv: &str, base: &[u8]) -> Result<String, String> {
    let t = std::time::Instant::now();
    let Some((ops, expected, schema, deep)) = script(base) else {
        return Ok(format!("{lv}: fewer than five placed objects - nothing to script"));
    };
    let a = apply(base, &ops, &schema).map_err(|e| format!("{lv}: {e}"))?;
    // (c) replayed, also after the log's JSON round trip
    let json = serde_json::to_string(&ops).unwrap();
    let back: Vec<Operation> = serde_json::from_str(&json).unwrap();
    let again = apply(base, &back, &schema).map_err(|e| format!("{lv}: {e}"))?;
    if again.bytes != a.bytes {
        return Err(format!("{lv}: a replay gives other bytes"));
    }
    // (b) parses, writes back byte for byte, reads as a tree
    let e = EditDbx::parse(&a.bytes).map_err(|e| format!("{lv}: the result does not parse: {}", e.0))?;
    if e.write().map_err(|e| e.0)? != a.bytes {
        return Err(format!("{lv}: the result does not write back byte for byte"));
    }
    bc2_formats::dbx::parse_tree(&a.bytes).map_err(|e| format!("{lv}: not a tree: {e}"))?;
    // (a) exactly the expected changes
    let b = EditDbx::parse(base).map_err(|e| e.0)?;
    let got: BTreeSet<Change> = diff(&b, &e).into_iter().collect();
    if got != expected {
        let extra: Vec<_> = got.difference(&expected).collect();
        let missing: Vec<_> = expected.difference(&got).collect();
        return Err(format!("{lv}: unexpected changes {extra:?}, missing {missing:?}"));
    }
    // (d) the deep copies name their own copies, never the originals they copied
    let w = LevelWriter::parse(&a.bytes).map_err(|e| e.to_string())?;
    let originals: Vec<&String> = deep.iter().map(|(s, _)| s).collect();
    for (_, copy) in &deep {
        let inst = w.find(copy).ok_or_else(|| format!("{lv}: {copy} missing"))?;
        for x in w.d.walk(inst) {
            if let Some(r) = w.d.attr(x, b"ref")
                && !r.contains(&b'/')
                && originals.iter().any(|o| r.eq_ignore_ascii_case(o.as_bytes()))
            {
                return Err(format!("{lv}: the copy {copy} still names an original ({})", up(r)));
            }
        }
    }
    Ok(format!(
        "{lv}: {} ops, {} changes as expected ({} instances copied with their owners, {} placed), {:.2} s",
        a.operations,
        got.len(),
        deep.len(),
        a.placed,
        t.elapsed().as_secs_f64()
    ))
}

#[test]
fn levelwrite_on_every_prepared_level() {
    let root = match levels::find_opfor(None) {
        Ok(r) => r,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let mut done = 0;
    let mut failures = Vec::new();
    let mut names: Vec<String> = std::fs::read_dir(root.join("work").join("levels"))
        .map(|r| {
            r.flatten()
                .map(|e| e.file_name().to_string_lossy().to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    for lv in names {
        let Some(base) = prepared(&root, &lv) else { continue };
        match prove(&lv, &base) {
            Ok(line) => println!("work\\levels  {line}"),
            Err(e) => failures.push(e),
        }
        done += 1;
    }
    for p in package::package_levels(&root) {
        let Some(bundle) = p.level.first() else { continue };
        let base = match partition_from_bundle(&bundle.path, &p.lv) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("{}: {e}", p.lv));
                continue;
            }
        };
        match prove(&p.lv, &base) {
            Ok(line) => println!("Package      {line}"),
            Err(e) => failures.push(e),
        }
        done += 1;
    }
    println!("{done} levels, {} failed", failures.len());
    assert!(failures.is_empty(), "{failures:#?}");
}
