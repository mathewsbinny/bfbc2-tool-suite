//! Times and sizes the editor core's document on a real level (rust/BENCHMARKS.md, "PR1b-0"): the base scene's
//! clone, checkpoint memory after many operations, undo / redo / History and the first ray after each, member edits
//! (their refusal check and commit), and prints a digest of every result so two builds can be compared (the same
//! digest: the same files, transforms and hits, bit for bit).
//!
//! `cargo run --release -p bc2-editor --example editor_core_bench -- <OPFOR folder> <level> [operations] [members]`
//! The level must be prepared (opened in the editor once). The project is made in a temporary folder and removed;
//! nothing else is written.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::type_complexity,
    missing_docs,
    unsafe_code
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use bc2_editor::edit::Doc;
use bc2_editor::{levels, query};
use bc2_levelwrite::Op;
use bc2_levelwrite::ops::Moved;
use bc2_scene::scene::{self, Scene, Source};

/// The heap in use and its peak, counted at every allocation.
struct Counting;

static NOW: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call is forwarded to the system allocator unchanged; only the sizes are counted.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: as the system allocator's contract.
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let n = NOW.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(n, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: as the system allocator's contract.
        unsafe { System.dealloc(p, l) };
        NOW.fetch_sub(l.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        // SAFETY: as the system allocator's contract.
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            if new >= l.size() {
                let n = NOW.fetch_add(new - l.size(), Ordering::Relaxed) + new - l.size();
                PEAK.fetch_max(n, Ordering::Relaxed);
            } else {
                NOW.fetch_sub(l.size() - new, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn mb(b: usize) -> f64 {
    b as f64 / 1_048_576.0
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// The document's files, every transform and the deleted set, as one digest.
fn state_digest(d: &Doc) -> String {
    let mut h = sha1_smol::Sha1::new();
    if let Some(b) = d.partition() {
        h.update(&b);
    }
    for (name, w) in &d.files().groups {
        h.update(name.as_bytes());
        h.update(&w.write().unwrap());
    }
    for e in &d.scene.entities {
        for v in e.matrix.iter().flatten() {
            h.update(&v.to_bits().to_le_bytes());
        }
    }
    for k in &d.deleted {
        h.update(&k.to_le_bytes());
    }
    h.digest().to_string()
}

/// A ray down onto the middle of the level (the walk camera's): its time and its hit's digest.
fn ray(s: &Scene, at: [f64; 3]) -> (f64, String) {
    let t = Instant::now();
    let hit = query::ray_hit_except(s, at, [0.0, -1.0, 0.0], &|_| false);
    let took = ms(t);
    let mut h = sha1_smol::Sha1::new();
    if let Some(x) = hit {
        h.update(&x.t.to_bits().to_le_bytes());
        h.update(&x.entity.map_or(u64::MAX, |e| e as u64).to_le_bytes());
    }
    (took, h.digest().to_string()[..8].to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(&args[1]);
    let lv = args[2].to_lowercase();
    let n_ops: usize = args.get(3).map_or(200, |r| r.parse().unwrap());
    let n_members: usize = args.get(4).map_or(100, |r| r.parse().unwrap());
    let found = levels::load_options(&root, &lv).unwrap();
    let t = Instant::now();
    let base = scene::load(&found.options).unwrap();
    println!("{lv}: loaded in {:.2} s", t.elapsed().as_secs_f64());
    let tris = base.terrain.as_ref().map_or(0, |t| t.1.len());
    println!(
        "{lv}: {} entities, {tris} terrain triangles, {} placed collision meshes",
        base.entities.len(),
        base.collision.as_ref().map_or(0, |c| c.meshes.len())
    );
    let mid = base.heightfield.as_ref().map_or([0.0, 2000.0, 0.0], |hf| {
        [
            f64::from(hf.min[0] + hf.max[0]) / 2.0,
            f64::from(hf.max[1]) + 10.0,
            f64::from(hf.min[2] + hf.max[2]) / 2.0,
        ]
    });
    // the base scene's clone: its time and its heap
    let before = NOW.load(Ordering::Relaxed);
    let t = Instant::now();
    let copy = base.clone();
    println!(
        "scene clone: {:.1} ms, {:.1} MB",
        ms(t),
        mb(NOW.load(Ordering::Relaxed).saturating_sub(before))
    );
    let (r0, _) = ray(&copy, mid);
    let (r1, _) = ray(&copy, mid);
    println!("first ray on the clone {r0:.2} ms, then {r1:.3} ms");
    drop(copy);
    let dir = std::env::temp_dir().join(format!("bc2_editor_core_bench_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let movable: Vec<(String, bc2_levelwrite::Rows)> = {
        let w = bc2_levelwrite::write::LevelWriter::parse(base.file(0).unwrap().1).unwrap();
        w.d.of_type(b".ReferenceObjectData")
            .into_iter()
            .filter_map(|e| {
                let g = String::from_utf8_lossy(w.d.attr(e, b"guid")?).to_ascii_uppercase();
                w.transform(&g).map(|r| (g, r))
            })
            .collect()
    };
    let members: Vec<(String, [f64; 16])> = base
        .entities
        .iter()
        .filter(|e| matches!(e.source, Source::GroupMember { .. }))
        .filter_map(|e| Some((e.guid.clone(), e.matrix?)))
        .collect();
    println!("{} movable objects, {} group members", movable.len(), members.len());
    let heap0 = NOW.load(Ordering::Relaxed);
    let t = Instant::now();
    let mut d = Doc::create("Core bench", &dir, &lv, base).unwrap();
    println!(
        "document made in {:.0} ms; heap +{:.1} MB",
        ms(t),
        mb(NOW.load(Ordering::Relaxed).saturating_sub(heap0))
    );
    let heap1 = NOW.load(Ordering::Relaxed);
    PEAK.store(heap1, Ordering::Relaxed);
    // n_ops moves, cycling over the movable objects
    let t = Instant::now();
    let mut each = Vec::new();
    for i in 0..n_ops {
        let (g, from) = &movable[i % movable.len()];
        let mut to = *from;
        to[3][0] += 1.0 + (i / movable.len()) as f32;
        let tt = Instant::now();
        d.commit(
            Op::Move {
                items: vec![Moved {
                    guid: g.clone(),
                    from: *from,
                    to,
                    placement: None,
                }],
            },
            "a move",
        );
        each.push(ms(tt));
    }
    each.sort_by(f64::total_cmp);
    println!(
        "{n_ops} moves in {:.0} ms (median {:.2} ms, max {:.1} ms); heap +{:.1} MB, peak +{:.1} MB",
        ms(t),
        each[each.len() / 2],
        each[each.len() - 1],
        mb(NOW.load(Ordering::Relaxed).saturating_sub(heap1)),
        mb(PEAK.load(Ordering::Relaxed).saturating_sub(heap1))
    );
    let b = NOW.load(Ordering::Relaxed);
    let t = Instant::now();
    let w = d.writer().unwrap().clone();
    println!(
        "one copy of the partition's writer: {:.1} ms, {:.1} MB",
        ms(t),
        mb(NOW.load(Ordering::Relaxed).saturating_sub(b))
    );
    let t = Instant::now();
    let groups = bc2_levelwrite::members::group_entities(&w);
    println!(
        "the partition's group entities: {} found in {:.1} ms",
        groups.len(),
        ms(t)
    );
    drop(w);
    let b = NOW.load(Ordering::Relaxed);
    let files = d.files().clone();
    println!(
        "one copy of the document's files: {:.1} MB",
        mb(NOW.load(Ordering::Relaxed).saturating_sub(b))
    );
    drop(files);
    // undo, redo, History; the first ray after each
    let mut steps: Vec<(&str, Box<dyn Fn(&mut Doc)>)> = Vec::new();
    for _ in 0..3 {
        steps.push((
            "undo",
            Box::new(|d: &mut Doc| {
                d.undo();
            }),
        ));
    }
    for _ in 0..3 {
        steps.push((
            "redo",
            Box::new(|d: &mut Doc| {
                d.redo();
            }),
        ));
    }
    let half = n_ops / 2;
    steps.push(("history middle", Box::new(move |d: &mut Doc| d.go_to(half))));
    steps.push(("history base", Box::new(|d: &mut Doc| d.go_to(0))));
    steps.push(("history end", Box::new(move |d: &mut Doc| d.go_to(n_ops))));
    let mut sha = sha1_smol::Sha1::new();
    for (name, f) in &steps {
        let t = Instant::now();
        f(&mut d);
        let took = ms(t);
        let (r, hit) = ray(&d.scene, mid);
        let state = state_digest(&d);
        sha.update(state.as_bytes());
        sha.update(hit.as_bytes());
        println!(
            "{name}: {took:.1} ms ({} replayed), first ray {r:.2} ms, state {}",
            d.last_replayed,
            &state[..8]
        );
    }
    println!(
        "heap after the History steps +{:.1} MB, peak +{:.1} MB",
        mb(NOW.load(Ordering::Relaxed).saturating_sub(heap1)),
        mb(PEAK.load(Ordering::Relaxed).saturating_sub(heap1))
    );
    // member edits: moves, then deletes (each checked for refusal first, as the editor does)
    if !members.is_empty() && n_members > 0 {
        let pick: Vec<&(String, [f64; 16])> = members
            .iter()
            .step_by((members.len() / n_members).max(1))
            .take(n_members)
            .collect();
        let mut moves = Vec::new();
        for (g, m) in &pick {
            let from = bc2_levelwrite::rows_of(m);
            let mut to = from;
            to[3][1] += 0.5;
            let t = Instant::now();
            d.commit(
                Op::Move {
                    items: vec![Moved {
                        guid: g.clone(),
                        from,
                        to,
                        placement: None,
                    }],
                },
                "a member move",
            );
            moves.push(ms(t));
        }
        let mut dels = Vec::new();
        let mut refusals = Vec::new();
        let mut refused = 0;
        for (g, _) in &pick {
            let op = Op::Delete { guids: vec![g.clone()] };
            let t = Instant::now();
            let why = d.refusal(&op);
            refusals.push(ms(t));
            if why.is_some() {
                refused += 1;
                continue;
            }
            let t = Instant::now();
            d.commit(op, "a member delete");
            dels.push(ms(t));
        }
        let med = |v: &mut Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v.get(v.len() / 2).copied().unwrap_or(0.0)
        };
        let (sm, sr, sd) = (
            moves.iter().sum::<f64>(),
            refusals.iter().sum::<f64>(),
            dels.iter().sum::<f64>(),
        );
        println!(
            "{} member moves: {sm:.0} ms (median {:.1} ms); refusal checks {sr:.0} ms (median {:.1} ms, {refused} \
             refused); {} member deletes {sd:.0} ms (median {:.1} ms)",
            moves.len(),
            med(&mut moves),
            med(&mut refusals),
            dels.len(),
            med(&mut dels)
        );
        let t = Instant::now();
        d.undo();
        let (r, _) = ray(&d.scene, mid);
        println!("undo after the member edits: {:.1} ms, first ray {r:.2} ms", ms(t));
        let state = state_digest(&d);
        sha.update(state.as_bytes());
        println!(
            "heap after the member edits +{:.1} MB, peak +{:.1} MB",
            mb(NOW.load(Ordering::Relaxed).saturating_sub(heap1)),
            mb(PEAK.load(Ordering::Relaxed).saturating_sub(heap1))
        );
    }
    println!("digest {}", sha.digest());
    drop(d);
    let _ = std::fs::remove_dir_all(&dir);
}
