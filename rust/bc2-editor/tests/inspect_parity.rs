//! The editor's property tree and link view against the Python Map Inspector (round GR1, before it was retired):
//! for a sample of `SP_01`, `MP_005`, `MP_009GR` and `ONS_MP_004`'s objects (of every type the first, middle and
//! last, every 50th static group member, the most referencing and most referenced), the editor's
//! [`Inspection`](bc2_editor::inspect::Inspection) gives the same property tree as the inspector's Selection panel
//! walked it (names, nesting, child counts, references and the objects they lead to, `dbx.value` texts), the same
//! references (`Entity.refs`) and the same "referenced by" list (`Inspector.referrers`). The inspector's answers are
//! kept as digests (`tests/data/inspector_props_*.json.gz`, recorded by GR1's `record_inspector_props.py`, removed
//! with the inspector): each list's length and the SHA-1 of its compact JSON (`json.dumps`, separators `,` and `:`,
//! `ensure_ascii=False`), which the test makes the same way from the editor's answers.
//!
//! Skipped (with a note) when the game copy is not there or a level is not prepared.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    missing_docs
)]

use std::collections::BTreeSet;
use std::io::Read;
use std::time::Instant;

use bc2_editor::inspect::{Inspection, Links, PropNode};
use bc2_editor::levels;
use bc2_scene::scene;
use serde_json::{Value, json};

const LEVELS: [&str; 4] = ["sp_01", "mp_005", "mp_009gr", "ons_mp_004"];

fn recorded(lv: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join(format!("inspector_props_{lv}.json.gz"));
    let mut s = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap())
        .read_to_string(&mut s)
        .unwrap();
    serde_json::from_str(&s).unwrap()
}

/// The inspector's rows of a property tree: `[name, "c", count, children]`, `[name, "r", ref, target]`,
/// `[name, "v", value]`.
fn rows(nodes: &[PropNode], links: &Links) -> Value {
    Value::Array(
        nodes
            .iter()
            .map(|n| match (&n.children, &n.reference) {
                (Some(c), _) => json!([n.name, "c", c.len(), rows(c, links)]),
                (None, Some(r)) => json!([n.name, "r", r, links.resolve(r)]),
                (None, None) => json!([n.name, "v", n.value]),
            })
            .collect(),
    )
}

fn digest(v: &Value, count: usize) -> Value {
    let text = serde_json::to_string(v).unwrap();
    json!([count, sha1_smol::Sha1::from(text.as_bytes()).digest().to_string()])
}

fn fields(nodes: &[PropNode]) -> usize {
    nodes.iter().map(|n| 1 + n.count()).sum()
}

#[test]
fn property_tree_and_links_as_the_map_inspector_showed_them() {
    let _gate = crate::gate();
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let mut checked = 0;
    for lv in LEVELS {
        let found = match levels::load_options(&root, lv) {
            Ok(f) => f,
            Err(e) => {
                println!("skipped {lv}: {e}");
                continue;
            }
        };
        let s = scene::load(&found.options).unwrap();
        let want = recorded(lv);
        assert_eq!(want["level"].as_str().unwrap(), s.name, "{lv}");
        assert_eq!(want["entities"].as_u64().unwrap(), s.entities.len() as u64, "{lv}");
        let t0 = Instant::now();
        let ins = Inspection::new(&s, None, &BTreeSet::new());
        let read = t0.elapsed().as_secs_f64();
        let (mut n_fields, mut n_refs, mut n_referrers) = (0, 0, 0);
        let sample = want["sample"].as_array().unwrap();
        for row in sample {
            let k = usize::try_from(row["index"].as_u64().unwrap()).unwrap();
            let e = &s.entities[k];
            assert_eq!(e.guid, row["guid"].as_str().unwrap(), "{lv}: guid of {k}");
            assert_eq!(e.type_name, row["type"].as_str().unwrap(), "{lv}: type of {k}");
            let tree = ins.tree(&s, k).unwrap();
            let got = rows(&tree, &ins.links);
            assert_eq!(
                digest(&got, fields(&tree)),
                row["tree"],
                "{lv}: property tree of {} ({}): {got}",
                e.guid,
                e.short_type
            );
            let refs: Vec<Value> = ins.links.refs(k).iter().map(|r| json!([r.path, r.reference])).collect();
            assert_eq!(
                digest(&Value::Array(refs.clone()), refs.len()),
                row["refs"],
                "{lv}: references of {}",
                e.guid
            );
            let by: Vec<Value> = ins.links.referrers(k).iter().map(|(f, t)| json!([f, t])).collect();
            assert_eq!(
                digest(&Value::Array(by.clone()), by.len()),
                row["referrers"],
                "{lv}: referrers of {}",
                e.guid
            );
            n_fields += fields(&tree);
            n_refs += refs.len();
            n_referrers += by.len();
        }
        println!(
            "{lv}: {} objects' property trees ({n_fields} fields), references ({n_refs}) and referrers ({n_referrers}) \
             as the Map Inspector showed them; {} event connections; links read in {read:.3} s",
            sample.len(),
            ins.links.events.len()
        );
        checked += 1;
    }
    println!("{checked} of {} levels compared with the Map Inspector", LEVELS.len());
}
