//! The Kynapse specification's corpus rules (`spec/formats/kynapse.md`, `spec/transforms/kynapse_generation.md`)
//! checked on DICE's path data in Project OPFOR BFBC2's game copy (run by `run_tests.py game`; ignored by a plain
//! `cargo test`):
//!
//!   cargo test --release -p bc2-formats --test kynapse_corpus -- --ignored --nocapture
//!
//! Every `Output\win32\ai\levels\<level>\{infantry,vehicle}\pathdata.hpd` of the game copy (`OPFOR_ROOT`, else
//! `project-opfor-bfbc2` beside the suite) is read. A test passes with a note when the folder is missing. The
//! measurements behind the observed rules are `examples/kynapse_probe.rs`.
#![allow(
    clippy::many_single_char_names,
    clippy::indexing_slicing,
    clippy::doc_markdown,
    clippy::naive_bytecount,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    missing_docs
)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bc2_formats::{hpd, kymesh};

struct File {
    name: String,
    vehicle: bool,
    data: Vec<u8>,
}

fn corpus() -> Vec<File> {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    let dir = root.join("game").join("Output").join("win32").join("ai").join("levels");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        println!("skipped: no game copy's path data (OPFOR_ROOT or beside the suite)");
        return Vec::new();
    };
    let mut levels: Vec<_> = rd.flatten().collect();
    levels.sort_by_key(std::fs::DirEntry::file_name);
    let mut out = Vec::new();
    for l in levels {
        for (kind, vehicle) in [("infantry", false), ("vehicle", true)] {
            let p = l.path().join(kind).join("pathdata.hpd");
            if let Ok(data) = std::fs::read(&p) {
                out.push(File {
                    name: format!("{}/{kind}", l.file_name().to_string_lossy()),
                    vehicle,
                    data,
                });
            }
        }
    }
    println!("{} path-data files", out.len());
    out
}

/// Each file's sectors with their parsed meshes.
fn each(mut f: impl FnMut(&File, &hpd::HpdFile<'_>, &[kymesh::Mesh])) {
    for file in corpus() {
        let h = hpd::parse(&file.data).expect("path data");
        let meshes: Vec<kymesh::Mesh> = h.sectors.iter().map(|s| kymesh::parse(s.mesh).expect("mesh")).collect();
        f(&file, &h, &meshes);
    }
}

// spec: KYN-FILE-1, KYN-SEC-1
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn header_words_and_sector_kinds() {
    each(|f, h, _| {
        assert_eq!(h.header[0], 2, "{}: word 0", f.name);
        assert_eq!(h.header[3], 1, "{}: word 3", f.name);
        assert_eq!(h.header[32], 36, "{}: word 32", f.name);
        assert_eq!(h.header[33], 12, "{}: word 33", f.name);
        assert_eq!(h.header[35], 0, "{}: word 35", f.name);
        for s in &h.sectors {
            assert_eq!(s.reserved, 0, "{}: sector {:#x} kind", f.name, s.id);
            let (gx, gz) = (s.id >> 8, s.id & 0xFF);
            assert!(
                s.id <= 0xFFFF && (1..=255).contains(&gx) && (1..=255).contains(&gz),
                "{}: id {:#x}",
                f.name,
                s.id
            );
        }
    });
}

// spec: KYN-SEC-2
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn table_bounds_are_the_route_nodes_box() {
    each(|f, h, _| {
        let mut seen = HashSet::new();
        for s in &h.sectors {
            assert!(!s.nodes.is_empty(), "{}: sector {:#x} has no node", f.name, s.id);
            let mut b = [f32::MAX, f32::MIN, f32::MAX, f32::MIN, f32::MAX, f32::MIN];
            for n in &s.nodes {
                for k in 0..3 {
                    b[2 * k] = b[2 * k].min(n.pos[k]);
                    b[2 * k + 1] = b[2 * k + 1].max(n.pos[k]);
                }
            }
            assert_eq!(s.bounds, b, "{}: sector {:#x}", f.name, s.id);
            assert!(
                seen.insert(s.bounds.map(f32::to_bits)),
                "{}: two sectors with one box",
                f.name
            );
        }
    });
}

// spec: KYN-SEC-3, KYG-SEC-1
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn one_sector_offset_per_file() {
    each(|f, h, meshes| {
        let offs: HashSet<(i64, i64)> = h
            .sectors
            .iter()
            .zip(meshes)
            .map(|(s, m)| {
                let cx = f64::from(m.rect[0] + m.rect[1]) / 2.0;
                let cz = f64::from(m.rect[2] + m.rect[3]) / 2.0;
                (
                    (cx / 100.0).floor() as i64 - i64::from(s.id >> 8),
                    (cz / 100.0).floor() as i64 - i64::from(s.id & 0xFF),
                )
            })
            .collect();
        assert_eq!(offs.len(), 1, "{}: {offs:?}", f.name);
    });
}

// spec: KYN-TILE-1, KYN-TILE-2
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn resolution_is_one_over_the_tile_size() {
    let mut vertices = 0u64;
    each(|f, _, meshes| {
        for m in meshes {
            let size = if f.vehicle {
                50.0
            } else if (m.pitch - 0.2).abs() < 1e-4 {
                20.0
            } else {
                40.0
            };
            assert!(
                (1.0 / m.resolution - size).abs() < 1e-3,
                "{}: resolution {}",
                f.name,
                m.resolution
            );
            assert_eq!(
                ((m.rect[1] - m.rect[0]) * m.resolution).ceil() as u32,
                m.tiles_x,
                "{}",
                f.name
            );
            assert_eq!(
                ((m.rect[3] - m.rect[2]) * m.resolution).ceil() as u32,
                m.tiles_z,
                "{}",
                f.name
            );
            for (t, items) in m.tiles.iter().enumerate() {
                let (tx, tz) = (t as u32 / m.tiles_z, t as u32 % m.tiles_z);
                let lo_x = m.rect[0] + tx as f32 * size - m.pitch / 2.0 - 0.01;
                let hi_x = m.rect[0] + (tx + 1) as f32 * size + m.pitch / 2.0 + 0.01;
                let lo_z = m.rect[2] + tz as f32 * size - m.pitch / 2.0 - 0.01;
                let hi_z = m.rect[2] + (tz + 1) as f32 * size + m.pitch / 2.0 + 0.01;
                for it in items {
                    for v in m.areas[it.area].outline.iter().flatten() {
                        vertices += 1;
                        assert!(
                            v[0] >= lo_x && v[0] <= hi_x && v[2] >= lo_z && v[2] <= hi_z,
                            "{}: an outline vertex outside its tile",
                            f.name
                        );
                    }
                }
            }
        }
    });
    println!("{vertices} outline vertices inside their tiles");
}

// spec: KYN-NODE-1
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn node_ids_are_unique_in_the_file() {
    each(|f, h, _| {
        let mut ordinary: HashMap<u32, u32> = HashMap::new();
        let mut border = HashSet::new();
        for s in &h.sectors {
            for n in &s.nodes {
                if n.id & 0x8000_0000 == 0 {
                    *ordinary.entry(n.id).or_default() += 1;
                } else {
                    border.insert(n.id & 0x7FFF_FFFF);
                }
            }
        }
        assert!(
            ordinary.values().all(|v| *v == 1),
            "{}: an ordinary id in two sectors",
            f.name
        );
        assert!(
            border.iter().all(|b| !ordinary.contains_key(b)),
            "{}: a border id's number reused",
            f.name
        );
    });
}

// spec: KYN-EDGE-1, KYG-EDGE-1
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn edge_reach_is_dist_edge_max_across() {
    each(|f, h, _| {
        let dmax = if f.vehicle { 50.0f32 } else { 15.0 };
        for s in &h.sectors {
            for e in &s.edges {
                let (a, b) = (&s.nodes[e.src as usize], &s.nodes[e.dst as usize]);
                let across = ((a.pos[0] - b.pos[0]).powi(2) + (a.pos[2] - b.pos[2]).powi(2)).sqrt();
                let border = (a.id | b.id) & 0x8000_0000 != 0;
                let limit = if border { 2.0 * dmax } else { dmax };
                assert!(
                    across <= limit + 0.01,
                    "{}: sector {:#x} edge {across} m across",
                    f.name,
                    s.id
                );
            }
        }
    });
}

// spec: KYN-LINK-1, KYG-LINK-1
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn link_objects_name_a_tile_item_of_an_adjacent_sector() {
    let mut objects = 0u64;
    each(|f, h, meshes| {
        let by_id: HashMap<u32, usize> = h.sectors.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        for (s, m) in h.sectors.iter().zip(meshes) {
            for l in &m.links {
                let (gx, gz, nx, nz) = (s.id >> 8, s.id & 0xFF, l.sector_id >> 8, l.sector_id & 0xFF);
                assert!(
                    gx.abs_diff(nx) <= 1 && gz.abs_diff(nz) <= 1 && l.sector_id != s.id,
                    "{}",
                    f.name
                );
                let Some(&ni) = by_id.get(&l.sector_id) else {
                    continue; // two links in the corpus name a sector the file does not hold
                };
                let nm = &meshes[ni];
                for o in &l.objects {
                    let [tx, tz, idx] = o.header;
                    objects += 1;
                    assert!(tx < nm.tiles_x && tz < nm.tiles_z, "{}", f.name);
                    let tile = &nm.tiles[(tx * nm.tiles_z + tz) as usize];
                    assert!(
                        (idx as usize) < tile.len(),
                        "{}: link object past its tile's items",
                        f.name
                    );
                }
            }
        }
    });
    println!("{objects} link objects resolved");
}

// spec: KYN-DF-2
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn infantry_f_records_by_direction_and_twinned() {
    each(|f, _, meshes| {
        if f.vehicle {
            return;
        }
        for m in meshes {
            for a in &m.areas {
                let ranks: Vec<u8> = a
                    .cells
                    .iter()
                    .map(|c| {
                        let d = c.direction;
                        assert!(d[0] == 0.0 || d[2] == 0.0, "{}: a diagonal infantry F record", f.name);
                        assert_eq!(d[1].to_bits(), 0x8000_0000, "{}: F direction y is not -0.0", f.name);
                        match (d[0].partial_cmp(&0.0), d[2].partial_cmp(&0.0)) {
                            (Some(std::cmp::Ordering::Less), _) => 0,
                            (Some(std::cmp::Ordering::Greater), _) => 1,
                            (_, Some(std::cmp::Ordering::Less)) => 2,
                            _ => 3,
                        }
                    })
                    .collect();
                assert!(
                    ranks.windows(2).all(|w| w[0] <= w[1]),
                    "{}: F records out of order",
                    f.name
                );
                let n = |r: u8| ranks.iter().filter(|x| **x == r).count();
                assert!(n(0) == n(1) && n(2) == n(3), "{}: F records not twinned", f.name);
                for c in &a.cells {
                    assert_eq!(c.points.first(), c.points.last(), "{}: an open F polygon", f.name);
                }
            }
        }
    });
}

// spec: KYG-EDGE-2, KYN-EDGE-2
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn edge_costs_are_the_f32_length_in_sixteenths() {
    let mut edges = 0u64;
    each(|f, h, _| {
        for s in &h.sectors {
            for e in &s.edges {
                let (a, b) = (&s.nodes[e.src as usize].pos, &s.nodes[e.dst as usize].pos);
                let l = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
                edges += 1;
                assert_eq!(
                    e.tag,
                    0x8000_0000 | (l * 16.0).floor() as u32,
                    "{}: sector {:#x}",
                    f.name,
                    s.id
                );
            }
        }
    });
    println!("{edges} edge costs exact");
}

// spec: KYG-EDGE-4
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn edges_have_a_reverse_by_node_id() {
    // [one-way by masked id, edges, one-way in their sector, of them into a non-owner copy] per type
    let mut n = [[0u64; 4]; 2];
    each(|f, h, _| {
        let t = usize::from(f.vehicle);
        let mut ids: HashSet<(u32, u32)> = HashSet::new();
        for s in &h.sectors {
            for e in &s.edges {
                ids.insert((
                    s.nodes[e.src as usize].id & 0x7FFF_FFFF,
                    s.nodes[e.dst as usize].id & 0x7FFF_FFFF,
                ));
            }
        }
        for s in &h.sectors {
            let local: HashSet<(u32, u32)> = s.edges.iter().map(|e| (e.src, e.dst)).collect();
            for e in &s.edges {
                let (a, b) = (&s.nodes[e.src as usize], &s.nodes[e.dst as usize]);
                n[t][1] += 1;
                if !ids.contains(&(b.id & 0x7FFF_FFFF, a.id & 0x7FFF_FFFF)) {
                    n[t][0] += 1;
                }
                if !local.contains(&(e.dst, e.src)) {
                    n[t][2] += 1;
                    if b.c & 0x1_0000 != 0 {
                        n[t][3] += 1;
                    }
                }
            }
        }
    });
    if n[0][1] == 0 {
        return;
    }
    println!(
        "infantry: {} of {} edges without a reverse by id ({} one-way in their sector, {} of them into a non-owner copy); vehicle: {} of {} ({}, {})",
        n[0][0], n[0][1], n[0][2], n[0][3], n[1][0], n[1][1], n[1][2], n[1][3]
    );
    // DICE's 35 files: infantry 14,692 of 1,646,482, vehicle 2,343 of 47,584.
    assert!(
        n[0][0] * 100 <= n[0][1],
        "infantry: more than 1 % of edges one-way by id"
    );
    assert!(n[1][0] * 20 <= n[1][1], "vehicle: more than 5 % of edges one-way by id");
    // A sector's one-way edges are mostly edges into a border copy that holds no edges of its own.
    assert!(
        n[0][3] * 100 >= n[0][2] * 95,
        "infantry: under 95 % of in-sector one-way edges end at a non-owner copy"
    );
}

/// The sector graph's shortest route from `i` to `j` that does not take the edge between them, if one is at most
/// `limit` long (3-D lengths of the stored positions).
fn route_without(nodes: &[hpd::Node], adj: &[Vec<usize>], i: usize, j: usize, limit: f64) -> Option<f64> {
    let d3 = |u: usize, v: usize| -> f64 {
        let (a, b) = (&nodes[u].pos, &nodes[v].pos);
        let w = [f64::from(a[0] - b[0]), f64::from(a[1] - b[1]), f64::from(a[2] - b[2])];
        (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt()
    };
    let mut best: HashMap<usize, f64> = HashMap::new();
    let mut heap = std::collections::BinaryHeap::new();
    best.insert(i, 0.0);
    heap.push((std::cmp::Reverse(0u64), i));
    while let Some((std::cmp::Reverse(k), u)) = heap.pop() {
        let du = k as f64 / 1e6;
        if du > best.get(&u).copied().unwrap_or(f64::MAX) + 1e-9 {
            continue;
        }
        if u == j {
            return Some(du);
        }
        for &v in &adj[u] {
            if (u == i && v == j) || (u == j && v == i) {
                continue;
            }
            let dv = du + d3(u, v);
            if dv <= limit && dv < best.get(&v).copied().unwrap_or(f64::MAX) {
                best.insert(v, dv);
                heap.push((std::cmp::Reverse((dv * 1e6) as u64), v));
            }
        }
    }
    None
}

// spec: KYG-EDGE-3
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn edges_are_not_redundant_at_1_2() {
    // [edges between ordinary nodes, of them with another route at most 1.2 x as long] per type
    let mut n = [[0u64; 2]; 2];
    each(|f, h, _| {
        let t = usize::from(f.vehicle);
        for s in &h.sectors {
            let mut adj: Vec<Vec<usize>> = vec![Vec::new(); s.nodes.len()];
            for e in &s.edges {
                let (a, b) = (e.src as usize, e.dst as usize);
                if !adj[a].contains(&b) {
                    adj[a].push(b);
                }
                if !adj[b].contains(&a) {
                    adj[b].push(a);
                }
            }
            for e in &s.edges {
                let (i, j) = (e.src as usize, e.dst as usize);
                if (s.nodes[i].id | s.nodes[j].id) & 0x8000_0000 != 0 || i > j {
                    continue;
                }
                let (a, b) = (&s.nodes[i].pos, &s.nodes[j].pos);
                let w = [f64::from(a[0] - b[0]), f64::from(a[1] - b[1]), f64::from(a[2] - b[2])];
                let d = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
                n[t][0] += 1;
                if route_without(&s.nodes, &adj, i, j, 1.2 * d).is_some() {
                    n[t][1] += 1;
                }
            }
        }
    });
    if n[0][0] == 0 {
        return;
    }
    println!(
        "edges between ordinary nodes with another route at most 1.2 x as long: infantry {} of {}, vehicle {} of {}",
        n[0][1], n[0][0], n[1][1], n[1][0]
    );
    assert!(
        n[0][1] * 100 <= n[0][0],
        "infantry: more than 1 % of edges redundant at 1.2"
    );
    assert!(
        n[1][1] * 20 <= n[1][0],
        "vehicle: more than 5 % of edges redundant at 1.2"
    );
}
