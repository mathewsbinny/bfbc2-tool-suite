//! Round NKS's measurements of DICE's Kynapse path data (`spec/formats/kynapse.md`,
//! `spec/transforms/kynapse_generation.md`): read-only, structure and counts only, no content printed.
//!
//!   cargo run --release -p bc2-formats --example kynapse_probe [-- <levels folder>]
//!
//! The folder defaults to Project OPFOR BFBC2's `game\Output\win32\ai\levels` (`OPFOR_ROOT`, else
//! `project-opfor-bfbc2` beside the suite). Every `<level>\{infantry,vehicle}\pathdata.hpd` there is read; the rule
//! each count backs is named on its line.
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::bool_to_int_with_if,
    clippy::cast_lossless,
    clippy::manual_midpoint,
    clippy::if_not_else,
    clippy::type_complexity,
    // every hash-map iteration below only counts (the order reaches no output)
    clippy::iter_over_hash_type,
    clippy::indexing_slicing,
    clippy::doc_markdown,
    clippy::naive_bytecount,
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::float_cmp,
    missing_docs
)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use bc2_formats::{hpd, kymesh};

fn levels_dir() -> PathBuf {
    if let Some(a) = std::env::args().nth(1) {
        return PathBuf::from(a);
    }
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    root.join("game").join("Output").join("win32").join("ai").join("levels")
}

/// Named counters, printed sorted.
#[derive(Default)]
struct C(BTreeMap<String, u64>);

impl C {
    fn add(&mut self, k: &str, n: u64) {
        *self.0.entry(k.to_owned()).or_default() += n;
    }
    fn inc(&mut self, k: &str) {
        self.add(k, 1);
    }
}

fn class(m: &kymesh::Mesh, vehicle: bool) -> &'static str {
    if vehicle {
        "veh"
    } else if (m.pitch - 0.2).abs() < 1e-4 {
        "det"
    } else {
        "inf"
    }
}

fn near(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

/// Distance of `v` from the nearest (k + 0.5) * p.
fn off_half(v: f32, p: f32) -> f32 {
    let t = f64::from(v) / f64::from(p) - 0.5;
    ((t - t.round()).abs() * f64::from(p)) as f32
}

/// Distance of `v` from the nearest k * p.
fn off_whole(v: f32, p: f32) -> f32 {
    let t = f64::from(v) / f64::from(p);
    ((t - t.round()).abs() * f64::from(p)) as f32
}

struct FileData {
    vehicle: bool,
    sectors: Vec<(u32, [f32; 6], u32, Vec<hpd::Node>, Vec<hpd::Edge>, kymesh::Mesh)>,
    header: Vec<u32>,
}

fn load(path: &Path, vehicle: bool) -> FileData {
    let data = std::fs::read(path).expect("read");
    let f = hpd::parse(&data).expect("path data");
    let sectors = f
        .sectors
        .iter()
        .map(|s| {
            let m = kymesh::parse(s.mesh).expect("mesh");
            (s.id, s.bounds, s.reserved, s.nodes.clone(), s.edges.clone(), m)
        })
        .collect();
    FileData {
        vehicle,
        sectors,
        header: f.header.to_vec(),
    }
}

fn main() {
    let mut files: Vec<(String, PathBuf, bool)> = Vec::new();
    // `.hpd` files named on the line (infantry; `--vehicle` before a file marks it vehicle): NavKit's builds.
    let mut vehicle_next = false;
    for a in std::env::args().skip(1) {
        if a == "--vehicle" {
            vehicle_next = true;
        } else if a.to_ascii_lowercase().ends_with(".hpd") {
            files.push((a.clone(), PathBuf::from(&a), std::mem::take(&mut vehicle_next)));
        }
    }
    let mut levels: Vec<_> = if files.is_empty() {
        std::fs::read_dir(levels_dir())
            .expect("levels folder")
            .flatten()
            .collect()
    } else {
        Vec::new()
    };
    levels.sort_by_key(std::fs::DirEntry::file_name);
    for l in levels {
        for (kind, veh) in [("infantry", false), ("vehicle", true)] {
            let p = l.path().join(kind).join("pathdata.hpd");
            if p.is_file() {
                files.push((format!("{}/{kind}", l.file_name().to_string_lossy()), p, veh));
            }
        }
    }
    println!("files {}", files.len());
    let mut c = C::default();
    for (name, path, veh) in &files {
        let fd = load(path, *veh);
        probe_file(name, &fd, &mut c);
    }
    println!("--- totals");
    for (k, v) in &c.0 {
        println!("{k} {v}");
    }
}

fn probe_file(name: &str, fd: &FileData, c: &mut C) {
    let t = if fd.vehicle { "veh" } else { "infF" };
    c.inc(&format!("files.{t}"));
    // HPD-HDR / HPD-LOAD: header words and the kind word (the table's +0x1C).
    if fd.header[35] != 0 {
        c.inc("hdr.w35_nonzero");
    }
    if fd.header[0] == 2 {
        c.inc("hdr.w0_is_2");
    }
    let by_id: HashMap<u32, usize> = fd.sectors.iter().enumerate().map(|(i, s)| (s.0, i)).collect();
    // Sector offsets as the file's own ids give them (most common floor(centre / 100) - grid id).
    let mut offs: BTreeMap<(i64, i64), u32> = BTreeMap::new();
    let (mut minx_id, mut maxx_id, mut minz_id, mut maxz_id) = (u32::MAX, 0, u32::MAX, 0);
    for s in &fd.sectors {
        let (gx, gz) = (s.0 >> 8, s.0 & 0xFF);
        minx_id = minx_id.min(gx);
        maxx_id = maxx_id.max(gx);
        minz_id = minz_id.min(gz);
        maxz_id = maxz_id.max(gz);
        let m = &s.5;
        let cx = f64::from(m.rect[0] + m.rect[1]) / 2.0;
        let cz = f64::from(m.rect[2] + m.rect[3]) / 2.0;
        let key = (
            (cx / 100.0).floor() as i64 - i64::from(gx),
            (cz / 100.0).floor() as i64 - i64::from(gz),
        );
        *offs.entry(key).or_default() += 1;
    }
    let (off, off_n) = offs.iter().max_by_key(|(_, n)| **n).map(|(k, n)| (*k, *n)).unwrap();
    c.add("ids.sectors", fd.sectors.len() as u64);
    c.add("ids.sectors_on_file_offset", u64::from(off_n));
    if minx_id >= 1 && minz_id >= 1 && maxx_id <= 255 && maxz_id <= 255 {
        c.inc("ids.files_in_1_255");
    }
    let mut first_ground = [f32::MAX, f32::MIN, f32::MAX, f32::MIN];
    let mut det_sectors = 0;
    for (sid, bounds, reserved, nodes, edges, m) in &fd.sectors {
        let cl = class(m, fd.vehicle);
        c.inc(&format!("sec.{cl}"));
        if cl == "det" {
            det_sectors += 1;
        }
        if *reserved == 0 {
            c.inc("tab.kind0");
        }
        let (gx, gz) = (*sid >> 8, *sid & 0xFF);
        // Sector square in world space by the file's offsets.
        let sq_x = (f64::from(gx) + off.0 as f64) * 100.0;
        let sq_z = (f64::from(gz) + off.1 as f64) * 100.0;
        // KYM-RES-1: resolution = 1 / tile size; tile size x tiles covers the rectangle.
        let tsize = 1.0 / m.resolution;
        let want = match cl {
            "veh" => 50.0,
            "det" => 20.0,
            _ => 40.0,
        };
        if near(tsize, want, 1e-3) {
            c.inc(&format!("res.tile_{cl}_ok"));
        }
        let span_x = m.rect[1] - m.rect[0];
        let span_z = m.rect[3] - m.rect[2];
        if (span_x * m.resolution).ceil() as u32 == m.tiles_x && (span_z * m.resolution).ceil() as u32 == m.tiles_z {
            c.inc("res.tiles_eq_ceil_span_x_res");
        }
        // Table bounds against the mesh rectangle and the content.
        if bounds[0] == m.rect[0] && bounds[1] == m.rect[1] && bounds[4] == m.rect[2] && bounds[5] == m.rect[3] {
            c.inc("tab.bounds_xz_eq_rect");
        } else if near(bounds[0], sq_x as f32, 1e-3) && near(bounds[4], sq_z as f32, 1e-3) {
            c.inc("tab.bounds_xz_at_square");
        } else {
            c.inc("tab.bounds_xz_other");
        }
        let mut ymin = f32::MAX;
        let mut ymax = f32::MIN;
        for a in &m.areas {
            ymin = ymin.min(a.values[3]);
            ymax = ymax.max(a.values[2]);
        }
        for n in nodes {
            ymin = ymin.min(n.pos[1]);
            ymax = ymax.max(n.pos[1]);
        }
        let mut bb = [f32::MAX, f32::MIN, f32::MAX, f32::MIN];
        for a in &m.areas {
            for v in a.outline.iter().flatten() {
                bb = [bb[0].min(v[0]), bb[1].max(v[0]), bb[2].min(v[2]), bb[3].max(v[2])];
            }
            for q in &a.border_portals {
                for v in [q.p0, q.p1] {
                    bb = [bb[0].min(v[0]), bb[1].max(v[0]), bb[2].min(v[2]), bb[3].max(v[2])];
                }
            }
        }
        let mut nb = [f32::MAX, f32::MIN, f32::MAX, f32::MIN, f32::MAX, f32::MIN];
        for n in nodes {
            nb = [
                nb[0].min(n.pos[0]),
                nb[1].max(n.pos[0]),
                nb[2].min(n.pos[1]),
                nb[3].max(n.pos[1]),
                nb[4].min(n.pos[2]),
                nb[5].max(n.pos[2]),
            ];
        }
        if !nodes.is_empty() && (0..6).all(|i| bounds[i] == nb[i]) {
            c.inc("tab.bounds_eq_node_bbox");
        } else if !nodes.is_empty() && (0..6).all(|i| near(bounds[i], nb[i], 1e-3)) {
            c.inc("tab.bounds_near_node_bbox");
        } else {
            c.inc("tab.bounds_not_node_bbox");
        }
        if near(bounds[0], bb[0], 1e-3)
            && near(bounds[1], bb[1], 1e-3)
            && near(bounds[4], bb[2], 1e-3)
            && near(bounds[5], bb[3], 1e-3)
        {
            c.inc("tab.bounds_xz_eq_outline_bbox");
        } else if !m.areas.is_empty() && fd.sectors.len() < 10 {
            let _ = sid;
        }
        if m.areas.is_empty() && nodes.is_empty() {
            c.inc("tab.empty_sector");
        } else if near(bounds[2], ymin, 1e-3) && near(bounds[3], ymax, 1e-3) {
            c.inc("tab.bounds_y_eq_content");
        } else if bounds[2] <= ymin + 1e-3 && bounds[3] >= ymax - 1e-3 {
            c.inc("tab.bounds_y_contain_content");
        } else {
            c.inc("tab.bounds_y_other");
        }
        // Every area: its tile by the resolution rule, its tile item heights, its index in the tile.
        let mut area_tile: Vec<Option<(u32, u32, usize)>> = vec![None; m.areas.len()];
        for (ti, items) in m.tiles.iter().enumerate() {
            let tx = ti as u32 / m.tiles_z;
            let tz = ti as u32 % m.tiles_z;
            for (k, it) in items.iter().enumerate() {
                area_tile[it.area] = Some((tx, tz, k));
                let a = &m.areas[it.area];
                if it.a == a.values[3] && it.b == a.values[2] {
                    c.inc("tile.item_eq_area_heights");
                } else {
                    c.inc("tile.item_ne_area_heights");
                }
            }
        }
        c.add(&format!("areas.{cl}"), m.areas.len() as u64);
        for (ai, a) in m.areas.iter().enumerate() {
            let Some((tx, tz, k)) = area_tile[ai] else {
                c.inc("areas.unlisted");
                continue;
            };
            if a.ids[1] == tx && a.ids[2] == tz && a.ids[3] as usize == k && a.ids[0] == *sid {
                c.inc("areas.ids_ok");
            }
            if a.values[1] == m.pitch && a.values[0] == m.radius {
                c.inc("areas.pitch_radius_eq_header");
            }
            // Cell centres inside tile (tx, tz) by floor((centre - rect min) * res); outline vertices are cell
            // corners, so the tile's corner range is [lo - p/2, hi - p/2] with lo = rect min + tx * T.
            let p = m.pitch;
            let lo_x = m.rect[0] + tx as f32 * tsize - p / 2.0 - 0.01;
            let hi_x = m.rect[0] + (tx + 1) as f32 * tsize + p / 2.0 + 0.01;
            let lo_z = m.rect[2] + tz as f32 * tsize - p / 2.0 - 0.01;
            let hi_z = m.rect[2] + (tz + 1) as f32 * tsize + p / 2.0 + 0.01;
            let alt = span_x / m.tiles_x as f32;
            let alo_x = m.rect[0] + tx as f32 * alt - p / 2.0 - 0.01;
            let ahi_x = m.rect[0] + (tx + 1) as f32 * alt + p / 2.0 + 0.01;
            let (mut ok, mut bad, mut abad) = (0u64, 0u64, 0u64);
            for line in &a.outline {
                for v in line {
                    if v[0] >= lo_x && v[0] <= hi_x && v[2] >= lo_z && v[2] <= hi_z {
                        ok += 1;
                    } else {
                        bad += 1;
                    }
                    if v[0] < alo_x || v[0] > ahi_x {
                        abad += 1;
                    }
                    first_ground[0] = first_ground[0].min(v[0]);
                    first_ground[1] = first_ground[1].max(v[0]);
                    first_ground[2] = first_ground[2].min(v[2]);
                    first_ground[3] = first_ground[3].max(v[2]);
                    // Outline on the half-pitch grid.
                    if off_half(v[0], p) < 0.01 && off_half(v[2], p) < 0.01 {
                        c.inc(&format!("e.vtx_half_grid_{cl}"));
                    } else {
                        c.inc(&format!("e.vtx_off_grid_{cl}"));
                    }
                }
            }
            c.add("res.vtx_in_tile", ok);
            c.add("res.vtx_out_tile", bad);
            c.add("res.alt_span_rule_vtx_out", abad);
            if a.outline.is_empty() && a.portals.is_empty() && a.border_portals.is_empty() {
                c.inc("areas.no_boundary");
            }
            // B portals: on a tile line, a tile midline, or elsewhere (DICE's conditional midline cut).
            for b in &a.portals {
                let horiz = near(b.p0[2], b.p1[2], 1e-3);
                let vert = near(b.p0[0], b.p1[0], 1e-3);
                let (coord, rmin) = if vert {
                    (b.p0[0], m.rect[0])
                } else if horiz {
                    (b.p0[2], m.rect[2])
                } else {
                    c.inc(&format!("b.diag_{cl}"));
                    continue;
                };
                // Tile line: rect min + k * T - p/2 (the corner before the tile's first centre).
                let rel = coord - rmin + p / 2.0;
                let kt = (rel / tsize).round();
                let km = ((rel - tsize / 2.0) / tsize).round();
                if near(rel, kt * tsize, 0.01) {
                    c.inc(&format!("b.tile_line_{cl}"));
                } else if near(rel, km * tsize + tsize / 2.0, 0.01) {
                    c.inc(&format!("b.midline_{cl}"));
                } else {
                    c.inc(&format!("b.other_{cl}"));
                }
                // KYM-B-1 symmetry: the target lists a portal back.
                let back = m.areas[b.target].portals.iter().any(|q| q.target == ai);
                c.inc(if back { "b.has_back" } else { "b.no_back" });
            }
            // C portals: where they sit against the rectangle, and their directions.
            let n_b = a.portals.len();
            for (k, bp) in a.border_portals.iter().enumerate() {
                let _ = k + n_b;
                let d = bp.direction;
                let dir = if d == [0.0, 1.0, 0.0] {
                    "up"
                } else if d[0] != 0.0 && d[2] != 0.0 {
                    "diag"
                } else {
                    "axis"
                };
                c.inc(&format!("c.dir_{dir}_{cl}"));
                if dir == "axis" {
                    let (v, lo, hi) = if d[0] != 0.0 {
                        (bp.p0[0], m.rect[0], m.rect[1])
                    } else {
                        (bp.p0[2], m.rect[2], m.rect[3])
                    };
                    let pos = if near(v, lo + p / 2.0, 0.01) {
                        "min_plus_half"
                    } else if near(v, hi - p / 2.0, 0.01) {
                        "max_minus_half"
                    } else if near(v, lo, 0.01) {
                        "min"
                    } else if near(v, hi, 0.01) {
                        "max"
                    } else {
                        "other"
                    };
                    c.inc(&format!("c.at_{pos}_{cl}"));
                    let toward_max = d[0] > 0.0 || d[2] > 0.0;
                    let (out_same, out_other) = if toward_max { (hi - v, v - lo) } else { (v - lo, hi - v) };
                    let first_corner = |o: f32| near(o, -0.5 * p, 0.01) || near(o, -0.4 * p, 0.01);
                    c.inc(&format!(
                        "c.axis_{}_{cl}",
                        if first_corner(out_same) {
                            "first_corner_outside_its_side"
                        } else if first_corner(out_other) {
                            "first_corner_outside_other_side"
                        } else {
                            "elsewhere"
                        }
                    ));
                }
            }
            // D and F.
            for d in &a.d_lines {
                c.inc(&format!("d.lines_{cl}"));
                c.inc(&format!("d.pts{}_{cl}", d.len().min(4)));
                for w in d.windows(2) {
                    let diag = !near(w[0][0], w[1][0], 1e-3) && !near(w[0][2], w[1][2], 1e-3);
                    c.inc(&format!("d.seg_{}_{cl}", if diag { "diag" } else { "axis" }));
                }
            }
            let mut fdirs: Vec<&'static str> = Vec::new();
            for f in &a.cells {
                let d = f.direction;
                let s = if d[0] != 0.0 && d[2] != 0.0 {
                    "diag"
                } else if d[0] < 0.0 {
                    "-x"
                } else if d[0] > 0.0 {
                    "+x"
                } else if d[2] < 0.0 {
                    "-z"
                } else {
                    "+z"
                };
                c.inc(&format!("f.dir_{s}_{cl}"));
                if s != "diag" && (d[1].to_bits() != 0x8000_0000 || (d[0] == 0.0 && d[0].to_bits() != 0x8000_0000)) {
                    c.inc(&format!("f.zero_not_negzero_{cl}"));
                }
                if f.points.first() != f.points.last() {
                    c.inc(&format!("f.open_{cl}"));
                }
                fdirs.push(s);
            }
            if !a.cells.is_empty() {
                // Infantry order: all -x, then +x, -z, +z.
                let rank = |s: &str| match s {
                    "-x" => 0,
                    "+x" => 1,
                    "-z" => 2,
                    "+z" => 3,
                    _ => 4,
                };
                let sorted = fdirs.windows(2).all(|w| rank(w[0]) <= rank(w[1]));
                c.inc(&format!(
                    "f.areas_{}_{cl}",
                    if sorted { "ordered" } else { "unordered" }
                ));
                let cnt = |s: &str| fdirs.iter().filter(|x| **x == s).count();
                if cnt("-x") == cnt("+x") && cnt("-z") == cnt("+z") {
                    c.inc(&format!("f.areas_twinned_{cl}"));
                }
            }
            if a.end_row != [0.0; 3] && !a.cells.is_empty() {
                c.inc("f.end_row_nonzero");
            }
        }
        // KYM-LINK: resolve each link object in the neighbour's mesh.
        for l in &m.links {
            let Some(&ni) = by_id.get(&l.sector_id) else {
                c.inc("link.neighbour_absent");
                continue;
            };
            let nm = &fd.sectors[ni].5;
            let (ngx, ngz) = (l.sector_id >> 8, l.sector_id & 0xFF);
            let adjacent = gx.abs_diff(ngx) <= 1 && gz.abs_diff(ngz) <= 1 && l.sector_id != *sid;
            c.inc(if adjacent { "link.adjacent" } else { "link.not_adjacent" });
            if gx != ngx && gz != ngz {
                c.inc("link.diagonal_neighbour");
            }
            for o in &l.objects {
                let [tx, tz, idx] = o.header;
                c.inc("link.objects");
                let ok_tile = tx < nm.tiles_x && tz < nm.tiles_z;
                let item = ok_tile
                    .then(|| {
                        nm.tiles
                            .get((tx * nm.tiles_z + tz) as usize)
                            .and_then(|t| t.get(idx as usize))
                    })
                    .flatten();
                let Some(item) = item else {
                    c.inc("link.unresolved");
                    continue;
                };
                c.inc("link.resolved");
                if nm.pitch == m.pitch {
                    c.inc("link.same_pitch");
                } else {
                    c.inc("link.cross_pitch");
                }
                // Does a C portal of this object touch the target area's C portals (same segment within a pitch)?
                let target = &nm.areas[item.area];
                let mut touch = 0;
                for &(owner, offset) in &o.refs {
                    let a = &m.areas[owner];
                    let k = (offset as usize - 0x50 - 0x20 * a.portals.len()) / 0x2C;
                    let Some(bp) = a.border_portals.get(k) else {
                        c.inc("link.ref_not_c");
                        continue;
                    };
                    c.inc("link.c_refs");
                    let mid = [(bp.p0[0] + bp.p1[0]) / 2.0, (bp.p0[2] + bp.p1[2]) / 2.0];
                    let hit = target.border_portals.iter().any(|q| {
                        let (x0, x1) = (q.p0[0].min(q.p1[0]) - 1.5, q.p0[0].max(q.p1[0]) + 1.5);
                        let (z0, z1) = (q.p0[2].min(q.p1[2]) - 1.5, q.p0[2].max(q.p1[2]) + 1.5);
                        mid[0] >= x0 && mid[0] <= x1 && mid[1] >= z0 && mid[1] <= z1
                    });
                    if hit {
                        touch += 1;
                    }
                }
                c.add("link.c_refs_meeting_target_c", touch);
                // Mirrored tile: same tile index across the shared side.
                let mirrored = if ngx != gx && ngz == gz {
                    tz == area_tile[o.refs[0].0].map_or(u32::MAX, |t| t.1)
                } else if ngz != gz && ngx == gx {
                    tx == area_tile[o.refs[0].0].map_or(u32::MAX, |t| t.0)
                } else {
                    false
                };
                c.inc(if mirrored {
                    "link.mirrored_tile"
                } else {
                    "link.other_tile"
                });
            }
        }
        // The route graph.
        probe_graph(nodes, edges, m, cl, c);
    }
    // Node ids across the file: ordinary ids unique?  Border ids: owners (copies without bit 16).
    let mut ord_ids: HashMap<u32, u32> = HashMap::new();
    let mut border: HashMap<u32, (u32, u32, bool)> = HashMap::new();
    let mut max_ord = 0u32;
    for s in &fd.sectors {
        let (gx, gz) = (s.0 >> 8, s.0 & 0xFF);
        for n in &s.3 {
            if n.id & 0x8000_0000 == 0 {
                *ord_ids.entry(n.id).or_default() += 1;
                max_ord = max_ord.max(n.id);
            } else {
                let e = border.entry(n.id).or_insert((0, 0, false));
                e.0 += 1;
                let sx = (f64::from(n.pos[0]) / 100.0).floor() as i64 - off.0;
                let sz = (f64::from(n.pos[2]) / 100.0).floor() as i64 - off.1;
                let holds = sx == i64::from(gx) && sz == i64::from(gz);
                if n.c & 0x1_0000 == 0 {
                    e.1 += 1;
                    if holds {
                        e.2 = true;
                    }
                }
            }
        }
    }
    let clash = border
        .keys()
        .filter(|k| ord_ids.contains_key(&(*k & 0x7FFF_FFFF)))
        .count();
    c.add("ids.border_low_bits_equal_an_ordinary_id", clash as u64);
    c.add("ids.border_ids", border.len() as u64);
    let max_b = border.keys().map(|k| k & 0x7FFF_FFFF).max().unwrap_or(0);
    c.add("ids.border_max_low_plus1", u64::from(max_b) + 1);
    let dup = ord_ids.values().filter(|v| **v > 1).count();
    c.add("ids.ordinary_ids", ord_ids.len() as u64);
    c.add("ids.ordinary_ids_in_two_sectors", dup as u64);
    c.add("ids.ordinary_max_plus1", u64::from(max_ord) + 1);
    for (copies, owners, owner_holds) in border.values() {
        c.inc(&format!("own.copies{}", copies.min(&5)));
        c.inc(&format!("own.owners{}", owners.min(&3)));
        if *owners == 1 && *owner_holds {
            c.inc("own.single_owner_holds_node");
        }
    }
    // The generator's rectangle: the ground's extent against the sector grid.
    let lo_x = (minx_id as f64 + off.0 as f64) * 100.0;
    let hi_x = (maxx_id as f64 + 1.0 + off.0 as f64) * 100.0;
    let lo_z = (minz_id as f64 + off.1 as f64) * 100.0;
    let hi_z = (maxz_id as f64 + 1.0 + off.1 as f64) * 100.0;
    println!(
        "{name}: sectors {} (detailed {det_sectors}) offsets {:?} ({off_n} agree); ground x {:.1}..{:.1} z {:.1}..{:.1}; inside the grid by {:.1} / {:.1} / {:.1} / {:.1} m",
        fd.sectors.len(),
        off,
        first_ground[0],
        first_ground[1],
        first_ground[2],
        first_ground[3],
        f64::from(first_ground[0]) - lo_x,
        hi_x - f64::from(first_ground[1]),
        f64::from(first_ground[2]) - lo_z,
        hi_z - f64::from(first_ground[3]),
    );
}

fn probe_graph(nodes: &[hpd::Node], edges: &[hpd::Edge], m: &kymesh::Mesh, cl: &str, c: &mut C) {
    let n = nodes.len();
    c.add(&format!("g.nodes_{cl}"), n as u64);
    c.add(&format!("g.edges_{cl}"), edges.len() as u64);
    // Ordinary ids: dense 0..k-1?
    let ord: Vec<u32> = nodes.iter().filter(|x| x.id & 0x8000_0000 == 0).map(|x| x.id).collect();
    if ord.iter().enumerate().all(|(i, &id)| id as usize == i) {
        c.inc("g.sectors_ordinary_ids_dense");
    } else {
        c.inc("g.sectors_ordinary_ids_sparse");
    }
    // Lookup lists: each a permutation of 0..n, sorted by its coordinate; ties tested against hypotheses.
    for (axis, name) in [(0usize, "x"), (1, "y"), (2, "z")] {
        let list: Vec<usize> = nodes
            .iter()
            .map(|x| match axis {
                0 => (x.b & 0xFFFF) as usize,
                1 => (x.b >> 16) as usize,
                _ => (x.c & 0xFFFF) as usize,
            })
            .collect();
        let mut seen = vec![false; n];
        let perm = list.iter().all(|&i| i < n && !std::mem::replace(&mut seen[i], true));
        if !perm {
            c.inc(&format!("look.{name}_not_perm"));
            continue;
        }
        let sorted = list.windows(2).all(|w| nodes[w[0]].pos[axis] <= nodes[w[1]].pos[axis]);
        c.inc(&format!("look.{name}_{}", if sorted { "sorted" } else { "unsorted" }));
        // MSVC CRT qsort on the indices 0..n in node order, comparing the coordinate.
        let key: Vec<f32> = nodes.iter().map(|x| x.pos[axis]).collect();
        let mut q: Vec<usize> = (0..n).collect();
        msvc_qsort(&mut q, &|a: &usize, b: &usize| cmp_f(key[*a], key[*b]));
        c.inc(&format!(
            "look.{name}_msvc_qsort_{}",
            if q == list { "exact" } else { "differs" }
        ));
        let mut st: Vec<usize> = (0..n).collect();
        st.sort_by(|a, b| key[*a].partial_cmp(&key[*b]).unwrap());
        c.inc(&format!(
            "look.{name}_stable_sort_{}",
            if st == list { "exact" } else { "differs" }
        ));
        // Tie groups.
        let mut i = 0;
        while i < n {
            let mut j = i + 1;
            while j < n && nodes[list[j]].pos[axis] == nodes[list[i]].pos[axis] {
                j += 1;
            }
            if j - i > 1 {
                let g = &list[i..j];
                c.inc(&format!("look.{name}_ties"));
                if g.windows(2).all(|w| w[0] < w[1]) {
                    c.inc(&format!("look.{name}_ties_index_asc"));
                }
                if g.windows(2).all(|w| w[0] > w[1]) {
                    c.inc(&format!("look.{name}_ties_index_desc"));
                }
                // Lexicographic by the other coordinates, in axis order.
                let (o1, o2) = match axis {
                    0 => (1, 2),
                    1 => (0, 2),
                    _ => (0, 1),
                };
                if g.windows(2).all(|w| {
                    let (a, b) = (&nodes[w[0]].pos, &nodes[w[1]].pos);
                    (a[o1], a[o2]) <= (b[o1], b[o2])
                }) {
                    c.inc(&format!("look.{name}_ties_by_other_axes"));
                }
            }
            i = j;
        }
    }
    // Edges: cost formula, order, symmetry, length against DistEdgeMax.
    let dmax = if cl == "veh" { 50.0 } else { 15.0 };
    let mut pairs: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for e in edges {
        pairs.insert((e.src, e.dst));
    }
    let mut prev: Option<(u32, u32)> = None;
    for e in edges {
        let (a, b) = (&nodes[e.src as usize], &nodes[e.dst as usize]);
        let d = ((a.pos[0] - b.pos[0]).powi(2) + (a.pos[1] - b.pos[1]).powi(2) + (a.pos[2] - b.pos[2]).powi(2)).sqrt();
        let want = 0x8000_0000 | (d * 16.0).floor() as u32;
        if e.tag == want {
            c.inc("e.cost_exact");
        } else if e.tag.abs_diff(want) == 1 {
            c.inc("e.cost_off_by_one");
        } else {
            c.inc("e.cost_other");
        }
        if e.tag & 0x8000_0000 == 0 {
            c.inc("e.cost_bit31_clear");
        }
        if let Some(p) = prev {
            if p.0 == e.src {
                let (pa, pb) = (&nodes[p.1 as usize], &nodes[e.dst as usize]);
                c.inc(if e.dst < p.1 {
                    "e.same_src_dst_desc"
                } else {
                    "e.same_src_dst_asc"
                });
                let dp = ((a.pos[0] - pa.pos[0]).powi(2) + (a.pos[2] - pa.pos[2]).powi(2)).sqrt();
                let dn = ((a.pos[0] - pb.pos[0]).powi(2) + (a.pos[2] - pb.pos[2]).powi(2)).sqrt();
                c.inc(if dn >= dp {
                    "e.same_src_len_nondecr"
                } else {
                    "e.same_src_len_decr"
                });
            }
            if e.src < p.0 {
                c.inc("e.not_sorted_src");
            } else if (e.src, e.dst) < p {
                c.inc("e.not_sorted_src_dst");
            } else if e.src == p.0 && e.dst == p.1 {
                c.inc("e.duplicate");
            }
        }
        prev = Some((e.src, e.dst));
        c.inc(if pairs.contains(&(e.dst, e.src)) {
            "e.two_way"
        } else {
            "e.one_way"
        });
        let both_border = a.id & b.id & 0x8000_0000 != 0;
        let lim = if both_border { 2.0 * dmax } else { dmax };
        if d > lim + 0.01 {
            c.inc(&format!("e.longer_than_limit_{cl}"));
        }
        if d > dmax + 0.01 {
            let kind = match (a.id >> 31, b.id >> 31) {
                (0, 0) => "ord_ord",
                (1, 1) => "bord_bord",
                _ => "ord_bord",
            };
            c.inc(&format!("e.longer_than_distedgemax_{cl}_{kind}"));
            let h = ((a.pos[0] - b.pos[0]).powi(2) + (a.pos[2] - b.pos[2]).powi(2)).sqrt();
            if h > dmax + 0.01 {
                c.inc(&format!("e.longer_than_distedgemax_2d_{cl}_{kind}"));
            }
            if h > 2.0 * dmax + 0.01 {
                c.inc(&format!("e.longer_than_2x_distedgemax_2d_{cl}_{kind}"));
            }
        }
        c.inc(&format!("hist.elen_{cl}_{:03}", (d / 5.0).floor() as u32 * 5));
        if a.id & 0x8000_0000 != 0 && b.id & 0x8000_0000 != 0 {
            c.inc(&format!("e.border_border_{cl}"));
        }
        if a.c & 0x1_0000 != 0 {
            c.inc("e.from_nonowner_copy");
        }
    }
    // Node placement: on the cell-centre grid (k * p) or the corner grid ((k + 0.5) * p); within the rectangle.
    let p = m.pitch;
    for x in nodes {
        let w = off_whole(x.pos[0], p) < 0.005 && off_whole(x.pos[2], p) < 0.005;
        let h = off_half(x.pos[0], p) < 0.005 && off_half(x.pos[2], p) < 0.005;
        c.inc(&format!(
            "n.grid_{}_{cl}",
            if w {
                "centre"
            } else if h {
                "corner"
            } else {
                "off"
            }
        ));
        let inside = x.pos[0] >= m.rect[0] && x.pos[0] <= m.rect[1] && x.pos[2] >= m.rect[2] && x.pos[2] <= m.rect[3];
        c.inc(if inside { "n.in_rect" } else { "n.out_rect" });
        if !inside {
            c.inc(if x.id & 0x8000_0000 != 0 {
                "n.out_rect_border"
            } else {
                "n.out_rect_ordinary"
            });
        }
        if x.flags != 0x8000_0000 {
            c.inc("n.flags_other");
        }
    }
}

fn cmp_f(a: f32, b: f32) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}

/// The Visual C++ 2005-2010 CRT `qsort` (median of three, `shortsort` up to 8 elements), on a slice.
fn msvc_qsort<T: Copy>(v: &mut [T], cmp: &dyn Fn(&T, &T) -> i32) {
    fn shortsort<T: Copy>(v: &mut [T], lo: usize, mut hi: usize, cmp: &dyn Fn(&T, &T) -> i32) {
        while hi > lo {
            let mut max = lo;
            for p in lo + 1..=hi {
                if cmp(&v[p], &v[max]) > 0 {
                    max = p;
                }
            }
            v.swap(max, hi);
            hi -= 1;
        }
    }
    let num = v.len();
    if num < 2 {
        return;
    }
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let (mut lo, mut hi) = (0usize, num - 1);
    loop {
        let size = hi - lo + 1;
        if size <= 8 {
            shortsort(v, lo, hi, cmp);
        } else {
            let mut mid = lo + size / 2;
            if cmp(&v[lo], &v[mid]) > 0 {
                v.swap(lo, mid);
            }
            if cmp(&v[lo], &v[hi]) > 0 {
                v.swap(lo, hi);
            }
            if cmp(&v[mid], &v[hi]) > 0 {
                v.swap(mid, hi);
            }
            let mut loguy = lo as isize;
            let mut higuy = hi as isize;
            let hi_i = hi as isize;
            loop {
                if (mid as isize) > loguy {
                    loop {
                        loguy += 1;
                        if !(loguy < mid as isize && cmp(&v[loguy as usize], &v[mid]) <= 0) {
                            break;
                        }
                    }
                }
                if (mid as isize) <= loguy {
                    loop {
                        loguy += 1;
                        if !(loguy <= hi_i && cmp(&v[loguy as usize], &v[mid]) <= 0) {
                            break;
                        }
                    }
                }
                loop {
                    higuy -= 1;
                    if !(higuy > mid as isize && cmp(&v[higuy as usize], &v[mid]) > 0) {
                        break;
                    }
                }
                if higuy < loguy {
                    break;
                }
                v.swap(loguy as usize, higuy as usize);
                if mid as isize == higuy {
                    mid = loguy as usize;
                }
            }
            higuy += 1;
            if (mid as isize) < higuy {
                loop {
                    higuy -= 1;
                    if !(higuy > mid as isize && cmp(&v[higuy as usize], &v[mid]) == 0) {
                        break;
                    }
                }
            }
            if (mid as isize) >= higuy {
                loop {
                    higuy -= 1;
                    if !(higuy > lo as isize && cmp(&v[higuy as usize], &v[mid]) == 0) {
                        break;
                    }
                }
            }
            let (l, h) = (lo as isize, hi as isize);
            if higuy - l >= h - loguy {
                if l < higuy {
                    stack.push((lo, higuy as usize));
                }
                if loguy < h {
                    lo = loguy as usize;
                    continue;
                }
            } else {
                if loguy < h {
                    stack.push((loguy as usize, hi));
                }
                if l < higuy {
                    hi = higuy as usize;
                    continue;
                }
            }
        }
        match stack.pop() {
            Some((a, b)) => {
                lo = a;
                hi = b;
            }
            None => return,
        }
    }
}
