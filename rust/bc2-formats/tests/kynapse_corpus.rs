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

// spec: KYN-EDGE-4
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn one_edge_per_source_and_target() {
    each(|f, h, _| {
        for s in &h.sectors {
            let mut seen = HashSet::new();
            for e in &s.edges {
                assert!(
                    seen.insert((e.src, e.dst)),
                    "{}: sector {:#x} has an edge twice",
                    f.name,
                    s.id
                );
            }
        }
    });
}

// spec: KYG-NODE-2
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn border_ids_have_edges_in_two_sectors() {
    each(|f, h, _| {
        let mut joins: HashMap<u32, HashSet<u32>> = HashMap::new();
        for s in &h.sectors {
            for n in &s.nodes {
                if n.id & 0x8000_0000 != 0 {
                    joins.entry(n.id).or_default();
                }
            }
            for e in &s.edges {
                for i in [e.src, e.dst] {
                    let n = &s.nodes[i as usize];
                    if n.id & 0x8000_0000 != 0 {
                        joins.entry(n.id).or_default().insert(s.id);
                    }
                }
            }
        }
        let lone = joins.values().filter(|v| v.len() < 2).count();
        assert_eq!(lone, 0, "{}: border ids with edges in fewer than two sectors", f.name);
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

// spec: KYG-NODE-4
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
#[allow(clippy::iter_over_hash_type, reason = "counts and a maximum: the order is not read")]
fn nodes_pair_one_step_apart_along_an_axis() {
    // Ordinary nodes of a sector that share their x or their z exactly (f32): the gap along the other axis in
    // millimetres, per kind (coarse, detailed, vehicle).
    let mut gaps: [HashMap<i64, u64>; 3] = [HashMap::new(), HashMap::new(), HashMap::new()];
    each(|f, h, meshes| {
        for (s, m) in h.sectors.iter().zip(meshes) {
            let k = if f.vehicle {
                2
            } else {
                usize::from((m.pitch - 0.2).abs() < 1e-4)
            };
            let ord: Vec<&hpd::Node> = s.nodes.iter().filter(|n| n.id & 0x8000_0000 == 0).collect();
            for (axis, other) in [(0usize, 2usize), (2, 0)] {
                let mut by: HashMap<u32, Vec<f32>> = HashMap::new();
                for n in &ord {
                    by.entry(n.pos[axis].to_bits()).or_default().push(n.pos[other]);
                }
                for v in by.values() {
                    for (i, a) in v.iter().enumerate() {
                        for b in &v[i + 1..] {
                            let mm = (f64::from((a - b).abs()) * 1000.0).round() as i64;
                            *gaps[k].entry(mm).or_default() += 1;
                        }
                    }
                }
            }
        }
    });
    if gaps[0].is_empty() {
        return;
    }
    // The step is GraphAccuracy + 0.35 m (3 / 0.5 / 15 m); the cell grid's own gaps (multiples of half the pitch)
    // are left out of the comparison.
    for (k, (name, step, pitch, least, ratio)) in [
        ("coarse", 3350i64, 800i64, 6000u64, 20u64),
        ("detailed", 850, 200, 200, 20),
        ("vehicle", 15350, 1000, 40, 10),
    ]
    .into_iter()
    .enumerate()
    {
        let g = &gaps[k];
        let at: u64 = (step - 2..=step + 2).map(|mm| g.get(&mm).copied().unwrap_or(0)).sum();
        let half = pitch / 2;
        let other = g
            .iter()
            .filter(|(mm, _)| {
                (**mm - step).abs() > 2
                    && (step * 4 / 5..=step * 6 / 5).contains(*mm)
                    && (**mm % half).min(half - **mm % half) > 2
            })
            .map(|(_, n)| *n)
            .max()
            .unwrap_or(0);
        println!("{name}: {at} pairs one step ({step} mm) apart; the most at any other gap within 20 %: {other}");
        assert!(at >= least, "{name}: under {least} pairs one step apart");
        assert!(
            at >= ratio * other.max(1),
            "{name}: the step is not {ratio} x any other gap near it"
        );
    }
}

// spec: KYN-FILE-2
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn header_tables_and_record_layout() {
    // sectors whose longest edge (3-D) is longer than the first node's +20 float; tile end slots holding the debug
    // heap's 0xBAADF00D fill (KYN-FILE-2's note on the generator's memory)
    let (mut longer, mut sectors, mut baad) = (0u64, 0u64, 0u64);
    each(|f, h, meshes| {
        for (s, m) in h.sectors.iter().zip(meshes) {
            sectors += 1;
            let len = |e: &hpd::Edge| {
                let (p, q) = (s.nodes[e.src as usize].pos, s.nodes[e.dst as usize].pos);
                let d: f32 = (0..3).map(|k| (p[k] - q[k]) * (p[k] - q[k])).sum();
                d.sqrt()
            };
            longer += u64::from(s.edges.iter().map(len).fold(0.0f32, f32::max) > s.nodes[0].value + 1e-3);
            baad += u64::from(m.end_slot == 0xBAAD_F00Du32.to_le_bytes());
        }
        let w = &h.header;
        // words 0 and 1 are one field written twice (the file version); 4-7 the mode-0 floats, zero in mode 1
        assert_eq!(w[1], w[0], "{}: word 1", f.name);
        assert_eq!(w[3], 1, "{}: word 3 (mode)", f.name);
        assert!(w[4..8].iter().all(|v| *v == 0), "{}: words 4-7", f.name);
        // two tables of four {u32, u32, u32}: one entry, descriptor 2 at offset 0 of size 8
        assert_eq!(&w[8..11], &[2, 0, 8], "{}: the first table's first entry", f.name);
        assert!(w[11..32].iter().all(|v| *v == 0), "{}: words 11-31", f.name);
        // the node record: 28 bytes of the node and the 8 bytes of descriptor 2 (the lookup lists b, c)
        assert_eq!(w[32], 28 + w[10], "{}: node stride", f.name);
        assert_eq!(w[34], 1, "{}: word 34", f.name);
        // the first node's +20 float is DistEdgeMax, not the sector's longest edge
        let dist_edge_max = if f.vehicle { 50.0 } else { 15.0 };
        for s in &h.sectors {
            assert_eq!(s.nodes[0].value, dist_edge_max, "{}: sector {:#x}", f.name, s.id);
        }
    });
    if sectors > 0 {
        println!("{longer} of {sectors} sectors hold an edge longer than the +20 value; {baad} end slots 0xBAADF00D");
        assert!(longer * 2 > sectors, "the +20 value is not the longest edge");
    }
}

// spec: KYG-NODE-6
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn node_ids_count_from_101_in_table_order() {
    let (mut used, mut span) = (0u64, 0u64);
    each(|f, h, _| {
        // a border id is numbered in the first sector (in table order) that holds it
        let mut seen = HashSet::new();
        let mut last = 100u32;
        let mut first = None;
        for s in &h.sectors {
            let mut new: Vec<u32> = Vec::new();
            for n in &s.nodes {
                if n.id & 0x8000_0000 == 0 || seen.insert(n.id) {
                    new.push(n.id & 0x7FFF_FFFF);
                }
            }
            let (Some(lo), Some(hi)) = (new.iter().min().copied(), new.iter().max().copied()) else {
                continue;
            };
            first.get_or_insert(lo);
            assert!(
                lo > last,
                "{}: sector {:#x} numbered before the sector ahead of it",
                f.name,
                s.id
            );
            used += new.len() as u64;
            span += u64::from(hi - lo + 1);
            last = hi;
        }
        assert_eq!(first, Some(101), "{}: the first id", f.name);
    });
    if span > 0 {
        let share = used as f64 / span as f64;
        println!(
            "ids used in the sectors' ranges: {used} of {span} ({:.1} %)",
            share * 100.0
        );
        assert!((0.7..0.9).contains(&share), "the ranges' fill {share}");
    }
}

// spec: HPD-TAB-4
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn sector_times_are_the_write_time() {
    let secs = |t: u32| (t / 1_000_000) * 3600 + (t / 10_000 % 100) * 60 + t / 100 % 100;
    each(|f, h, _| {
        let date = h.sectors[0].date;
        let t0 = secs(h.sectors[0].time);
        let mut prev = t0;
        for s in &h.sectors {
            assert_eq!(s.time % 100, 0, "{}: hundredths", f.name);
            assert_eq!(s.date, date, "{}: one date per file", f.name);
            let t = secs(s.time);
            assert!(
                t >= prev,
                "{}: sector {:#x} written before the one ahead of it",
                f.name,
                s.id
            );
            prev = t;
        }
        assert!(prev - t0 <= 10, "{}: the sectors written over {} s", f.name, prev - t0);
    });
}

// spec: KYG-PAR-1
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn nearest_node_at_dist_propagation() {
    // DistPropagation = GraphAccuracy + 0.35 (Kynapse's default EntityRadius): 3.35 / 0.85 / 15.35 m. Count the
    // ordinary nodes whose nearest ordinary node of the same floor (height within 2 m) lies at it within 2 mm, and
    // at 1.05 times it as a control.
    let mut at = [[0u64; 2]; 3];
    each(|f, h, meshes| {
        for (s, m) in h.sectors.iter().zip(meshes) {
            let k = if f.vehicle {
                2
            } else {
                usize::from((m.pitch - 0.2).abs() < 1e-4)
            };
            let dp = [3.35f64, 0.85, 15.35][k];
            let ord: Vec<[f64; 3]> = s
                .nodes
                .iter()
                .filter(|n| n.id & 0x8000_0000 == 0)
                .map(|n| n.pos.map(f64::from))
                .collect();
            for (i, p) in ord.iter().enumerate() {
                let mut best = f64::INFINITY;
                for (j, q) in ord.iter().enumerate() {
                    if i != j && (p[1] - q[1]).abs() < 2.0 {
                        best = best.min((p[0] - q[0]).hypot(p[2] - q[2]));
                    }
                }
                at[k][0] += u64::from((best - dp).abs() <= 0.002);
                at[k][1] += u64::from((best - dp * 1.05).abs() <= 0.002);
            }
        }
    });
    if at[0][0] == 0 {
        return;
    }
    for (k, (name, least)) in [("coarse", 7000u64), ("detailed", 2000), ("vehicle", 30)]
        .into_iter()
        .enumerate()
    {
        println!(
            "{name}: {} nodes with the nearest at DistPropagation, {} at 1.05 x it",
            at[k][0], at[k][1]
        );
        assert!(at[k][0] >= least, "{name}: under {least}");
        assert!(at[k][0] >= 10 * at[k][1].max(1), "{name}: not 10 x the control");
    }
}

// spec: KYG-NODE-7
#[test]
#[ignore = "game tier: reads DICE's path data in the game copy"]
fn step_pairs_lie_on_the_nbsectors_probe_directions() {
    // The propagation sends NbSectors probes per node, one every 2 pi / NbSectors radians from +x (32 coarse, 128
    // detailed, 64 vehicle), each DistPropagation long. Pairs of ordinary nodes of a sector at the same floor exactly
    // one step apart in plan (+- 2 mm) lie on those directions (within 0.01 degrees); about half on the odd ones, so
    // the count is NbSectors, not half of it. Control: pairs at 1.02-1.10 steps.
    let mut on = [[0u64; 4]; 3]; // per kind: step pairs on / off the directions, on an odd direction; control on
    let mut ctrl = [0u64; 3];
    each(|f, h, meshes| {
        for (s, m) in h.sectors.iter().zip(meshes) {
            let k = if f.vehicle {
                2
            } else {
                usize::from((m.pitch - 0.2).abs() < 1e-4)
            };
            let (dp, n) = [(3.35f64, 32u32), (0.85, 128), (15.35, 64)][k];
            let ord: Vec<[f64; 3]> = s
                .nodes
                .iter()
                .filter(|v| v.id & 0x8000_0000 == 0)
                .map(|v| v.pos.map(f64::from))
                .collect();
            let off = |a: &[f64; 3], b: &[f64; 3], n: u32| {
                let ang = (b[2] - a[2]).atan2(b[0] - a[0]).to_degrees().rem_euclid(360.0);
                let step = 360.0 / f64::from(n);
                let r = ang.rem_euclid(step);
                r.min(step - r)
            };
            for (i, a) in ord.iter().enumerate() {
                for b in &ord[i + 1..] {
                    if (a[1] - b[1]).abs() > 3.0 {
                        continue;
                    }
                    let d = (a[0] - b[0]).hypot(a[2] - b[2]);
                    if (d - dp).abs() <= 0.002 {
                        if off(a, b, n) < 0.01 {
                            on[k][0] += 1;
                            on[k][2] += u64::from(off(a, b, n / 2) >= 0.01);
                        } else {
                            on[k][1] += 1;
                        }
                    } else if (d - dp * 1.06).abs() <= dp * 0.04 {
                        on[k][3] += 1;
                        ctrl[k] += u64::from(off(a, b, n) < 0.01);
                    }
                }
            }
        }
    });
    if on[0][0] == 0 {
        return;
    }
    for (k, name) in ["coarse", "detailed", "vehicle"].into_iter().enumerate() {
        println!(
            "{name}: step pairs {} on the directions ({} on odd ones), {} off; control {} of {} on",
            on[k][0], on[k][2], on[k][1], ctrl[k], on[k][3]
        );
        assert!(on[k][0] >= 15 * on[k][1], "{name}: under 93 % on the directions");
        assert!(
            4 * on[k][2] >= on[k][0] && 4 * on[k][2] <= 3 * on[k][0],
            "{name}: odd directions not about half"
        );
        assert!(
            ctrl[k] * 20 <= on[k][3].max(1),
            "{name}: the control on the directions too"
        );
    }
}
