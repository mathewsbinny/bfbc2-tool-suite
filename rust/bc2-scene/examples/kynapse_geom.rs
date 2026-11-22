//! Round NKS-rest2's fits of DICE's Kynapse generator rules against the levels' own geometry
//! (`spec/transforms/kynapse_generation.md`): DICE's path data (read-only) beside each level's Havok collision, height
//! field and entities as the suite's scene reader loads them. Counts and histograms only, no content printed.
//!
//!   cargo run --release -p bc2-scene --example kynapse_geom -- [<level>...] [--only <fit>] [--rows] [--opfor <folder>]
//!
//! Fits: `clearance` (KYG-NODE-1: each node's distance from the collision in the soldier's height band, against control
//! points beside it), `detailed` (KYG-DET-1: each infantry sector's level data against DICE's choice of pitch, the best
//! thresholds; `--rows` prints every sector's numbers), `rect` (KYG-RECT-1: the ground's box against the boxes of the
//! level's entity categories and outlines, and the spawns' box raw).
//!
//! The OPFOR folder defaults to `OPFOR_ROOT`, else `project-opfor-bfbc2` beside the suite. A level must be prepared
//! (`work\levels\<lv>\dist` and its terrain unpack); nothing is written.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::iter_over_hash_type,
    clippy::float_cmp,
    clippy::doc_markdown,
    missing_docs
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use bc2_formats::{hpd, kymesh};
use bc2_scene::{scene, walk};

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

type P2 = [f64; 2];

fn len(a: P2) -> f64 {
    a[0].hypot(a[1])
}

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn pt_seg(p: P2, a: P2, b: P2) -> (f64, f64) {
    let ab = sub(b, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 < 1e-12 {
        0.0
    } else {
        (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0)
    };
    (len(sub(p, [a[0] + ab[0] * t, a[1] + ab[1] * t])), t)
}

fn opfor_root(given: Option<&str>) -> PathBuf {
    if let Some(g) = given {
        return PathBuf::from(g);
    }
    if let Some(v) = std::env::var_os("OPFOR_ROOT").filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("project-opfor-bfbc2")
}

fn files_in(dir: &Path, f: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir).map_or_else(
        |_| Vec::new(),
        |r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| f(&p.file_name().unwrap().to_string_lossy().to_lowercase()))
                .collect()
        },
    );
    out.sort();
    out
}

fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    walk::find(dir, &[&name.to_lowercase()])
        .into_iter()
        .flatten()
        .find(|f| f.rel.rsplit('/').next() == Some(name.to_lowercase().as_str()))
        .map(|f| f.path)
}

/// The level's scene with collision and terrain, as the editor's `levels::load_options` finds it.
fn load_scene(root: &Path, lv: &str) -> Option<scene::Scene> {
    let base = root.join("work").join("levels").join(lv);
    if !base.join("dist").is_dir() {
        return None;
    }
    let mut bases = vec![base.join("pkg"), base.join("dist")];
    bases.extend(files_in(&root.join("work").join("navkit").join("common"), |_| true));
    let partition = bases[..2]
        .iter()
        .flat_map(|b| files_in(&b.join("levels"), |n| n == format!("{lv}.dbx")))
        .next()?;
    let name = partition.file_stem().unwrap().to_string_lossy().into_owned();
    let mut groups = Vec::new();
    for b in &bases[..2] {
        for g in files_in(&b.join("levels").join(lv), |n| n.ends_with("_havokasset.dbx")) {
            if !groups.iter().any(|x: &PathBuf| x.file_name() == g.file_name()) {
                groups.push(g);
            }
        }
    }
    let tname = scene::terrain_name(&partition)
        .ok()
        .flatten()
        .unwrap_or_else(|| lv.to_owned());
    let tname = tname.rsplit('/').next().unwrap_or(&tname).to_owned();
    let heightfield = find(&base.join("terrain"), &format!("{tname}.heightfield-000.res"));
    let options = scene::LoadOptions {
        groups,
        geometry: true,
        heightfield,
        terrain_stride: 1,
        water: None,
        ..scene::LoadOptions::new(name, partition, bases)
    };
    scene::load(&options).ok()
}

struct Sec {
    id: u32,
    nodes: Vec<hpd::Node>,
    mesh: kymesh::Mesh,
}

fn read_hpd(path: &Path) -> Option<Vec<Sec>> {
    let data = fs::read(path).ok()?;
    let f = hpd::parse(&data).ok()?;
    Some(
        f.sectors
            .iter()
            .map(|s| Sec {
                id: s.id,
                nodes: s.nodes.clone(),
                mesh: kymesh::parse(s.mesh).expect("mesh"),
            })
            .collect(),
    )
}

/// World collision triangles of every placed object, indexed on a 2-D grid.
struct Tris {
    t: Vec<[[f64; 3]; 3]>,
    grid: HashMap<(i64, i64), Vec<u32>>,
}

const GC: f64 = 2.0;

impl Tris {
    fn of(s: &scene::Scene) -> Self {
        let mut t = Vec::new();
        if let Some(c) = &s.collision {
            for (e, _) in &c.meshes {
                if let Some(w) = s.world_triangles(*e) {
                    t.extend(w);
                }
            }
        }
        let mut grid: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for (i, tri) in t.iter().enumerate() {
            let x0 = tri.iter().map(|v| v[0]).fold(f64::MAX, f64::min);
            let x1 = tri.iter().map(|v| v[0]).fold(f64::MIN, f64::max);
            let z0 = tri.iter().map(|v| v[2]).fold(f64::MAX, f64::min);
            let z1 = tri.iter().map(|v| v[2]).fold(f64::MIN, f64::max);
            if (x1 - x0) / GC > 200.0 || (z1 - z0) / GC > 200.0 {
                continue;
            }
            for gx in (x0 / GC).floor() as i64..=(x1 / GC).floor() as i64 {
                for gz in (z0 / GC).floor() as i64..=(z1 / GC).floor() as i64 {
                    grid.entry((gx, gz)).or_default().push(i as u32);
                }
            }
        }
        Self { t, grid }
    }

    /// The obstacle footprint near `p` in the height band [y0, y1]: each triangle clipped to the band, as polygons.
    fn band(&self, p: P2, reach: f64, y0: f64, y1: f64, out: &mut Vec<Vec<P2>>) {
        out.clear();
        let mut seen: HashSet<u32> = HashSet::new();
        let r = (reach / GC).ceil() as i64;
        let (gx, gz) = ((p[0] / GC).floor() as i64, (p[1] / GC).floor() as i64);
        for dx in -r..=r {
            for dz in -r..=r {
                let Some(list) = self.grid.get(&(gx + dx, gz + dz)) else {
                    continue;
                };
                for &i in list {
                    if !seen.insert(i) {
                        continue;
                    }
                    let tri = &self.t[i as usize];
                    if tri.iter().all(|v| v[1] < y0) || tri.iter().all(|v| v[1] > y1) {
                        continue;
                    }
                    let poly = clip_band(tri, y0, y1);
                    if poly.len() >= 2 {
                        out.push(poly);
                    }
                }
            }
        }
    }
}

/// A triangle clipped to y0 <= y <= y1, its plan (x, z).
fn clip_band(tri: &[[f64; 3]; 3], y0: f64, y1: f64) -> Vec<P2> {
    let mut poly: Vec<[f64; 3]> = tri.to_vec();
    for (lim, keep_above) in [(y0, true), (y1, false)] {
        let inside = |v: &[f64; 3]| if keep_above { v[1] >= lim } else { v[1] <= lim };
        let mut out = Vec::new();
        for k in 0..poly.len() {
            let a = poly[k];
            let b = poly[(k + 1) % poly.len()];
            let (ia, ib) = (inside(&a), inside(&b));
            if ia {
                out.push(a);
            }
            if ia != ib {
                let t = (lim - a[1]) / (b[1] - a[1]);
                out.push([a[0] + (b[0] - a[0]) * t, lim, a[2] + (b[2] - a[2]) * t]);
            }
        }
        poly = out;
        if poly.is_empty() {
            break;
        }
    }
    poly.iter().map(|v| [v[0], v[2]]).collect()
}

/// Distance from p to the polygons (0 inside one), and to their nearest vertex.
fn dist_polys(p: P2, polys: &[Vec<P2>]) -> (f64, f64) {
    let mut d = f64::MAX;
    let mut dv = f64::MAX;
    for poly in polys {
        let n = poly.len();
        let mut odd = false;
        for k in 0..n {
            let a = poly[k];
            let b = poly[(k + 1) % n];
            d = d.min(pt_seg(p, a, b).0);
            dv = dv.min(len(sub(p, a)));
            if n >= 3 && (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]);
                if x > p[0] {
                    odd = !odd;
                }
            }
        }
        if odd {
            d = 0.0;
        }
    }
    (d, dv)
}

fn class(m: &kymesh::Mesh, veh: bool) -> &'static str {
    if veh {
        "veh"
    } else if (m.pitch - 0.2).abs() < 1e-4 {
        "det"
    } else {
        "inf"
    }
}

fn band_cm(d: f64, step: f64, max: i64) -> String {
    if d == f64::MAX {
        return "open".to_owned();
    }
    format!("{:04}", ((d * 100.0 / step).floor() as i64).min(max) * step as i64)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opfor = args
        .iter()
        .position(|a| a == "--opfor")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let root = opfor_root(opfor.as_deref());
    let ai = root.join("game").join("Output").join("win32").join("ai").join("levels");
    let only = args
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let mut wanted: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--") && Some(*a) != opfor.as_ref() && Some(*a) != only.as_ref())
        .map(|a| a.to_lowercase())
        .collect();
    if wanted.is_empty() {
        wanted = files_in(&ai, |_| true)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_lowercase())
            .collect();
    }
    let run = |f: &str| only.as_deref().is_none_or(|o| o == f);
    let mut rows: Vec<Row> = Vec::new();
    let mut c = C::default();
    for lv in &wanted {
        let t0 = std::time::Instant::now();
        let Some(sc) = load_scene(&root, lv) else {
            println!("{lv}: not prepared, skipped");
            continue;
        };
        let tris = Tris::of(&sc);
        println!(
            "{lv}: {} entities, {} collision triangles, terrain {}, {:.1} s",
            sc.entities.len(),
            tris.t.len(),
            sc.heightfield.is_some(),
            t0.elapsed().as_secs_f64()
        );
        c.inc("levels");
        for (kind, veh) in [("infantry", false), ("vehicle", true)] {
            let Some(secs) = read_hpd(&ai.join(lv).join(kind).join("pathdata.hpd")) else {
                continue;
            };
            if run("clearance") {
                fit_clearance(&secs, veh, &tris, &mut c);
            }
            if !veh && run("detailed") {
                rows.extend(sector_features(lv, &sc, &secs, &tris));
            }
            if run("rect") {
                fit_rect(lv, veh, &sc, &secs, &mut c);
            }
        }
    }
    if !rows.is_empty() {
        detailed_report(&rows, &mut c);
    }
    println!("--- totals");
    for (k, v) in &c.0 {
        println!("{k} {v}");
    }
}

// ---------------------------------------------------------------- KYG-DET-1: detailed sectors against level data

/// One infantry sector's facts: level, detailed, and named features.
struct Row {
    level: String,
    id: u32,
    detailed: bool,
    f: BTreeMap<&'static str, f64>,
}

/// The sector's 100 m square (min x, min z) to within a pitch (the rectangle starts at most a pitch before it).
fn square(m: &kymesh::Mesh) -> [f64; 2] {
    [f64::from(m.rect[0] + m.pitch), f64::from(m.rect[2] + m.pitch)]
}

fn in_square(sq: [f64; 2], p: [f64; 3], grow: f64) -> bool {
    p[0] >= sq[0] - grow && p[0] < sq[0] + 100.0 + grow && p[2] >= sq[1] - grow && p[2] < sq[1] + 100.0 + grow
}

fn sector_features(lv: &str, sc: &scene::Scene, secs: &[Sec], tris: &Tris) -> Vec<Row> {
    let mut out = Vec::new();
    for s in secs {
        let m = &s.mesh;
        let sq = square(m);
        let mut f: BTreeMap<&'static str, f64> = BTreeMap::new();
        for e in &sc.entities {
            let Some(p) = e.position() else { continue };
            if !in_square(sq, p, 0.0) {
                continue;
            }
            let k: Option<&'static str> = match e.category.as_str() {
                "soldier spawn" => Some(if e.ai == Some(true) { "spawn_ai" } else { "spawn_player" }),
                "vehicle spawn" => Some("vehicle_spawn"),
                "objective" => Some("objective"),
                "capture point" => Some("capture_point"),
                "ai order" => Some("ai_order"),
                "ladder" => Some("ladder"),
                _ => None,
            };
            if let Some(k) = k {
                *f.entry(k).or_default() += 1.0;
            }
        }
        if let Some(col) = &sc.collision {
            let n = col
                .meshes
                .iter()
                .filter(|(e, _)| sc.entities[*e].position().is_some_and(|p| in_square(sq, p, 0.0)))
                .count();
            f.insert("objects_with_collision", n as f64);
        }
        // Collision triangles whose first corner is in the square (thousands).
        let mut nt = 0usize;
        let (g0x, g0z) = ((sq[0] / GC).floor() as i64, (sq[1] / GC).floor() as i64);
        let k = (100.0 / GC) as i64;
        let mut seen: HashSet<u32> = HashSet::new();
        for dx in 0..k {
            for dz in 0..k {
                if let Some(l) = tris.grid.get(&(g0x + dx, g0z + dz)) {
                    for &i in l {
                        if seen.insert(i) {
                            nt += 1;
                        }
                    }
                }
            }
        }
        f.insert("collision_ktris", nt as f64 / 1000.0);
        // DICE's own ground: the share of its sample points (0.8 m apart) standing on objects (area ground above
        // the terrain by 0.5 m), and the share in columns that hold ground twice (stacked floors).
        if let Some(hf) = &sc.heightfield {
            let mut cols: HashMap<(i64, i64), u32> = HashMap::new();
            let (mut n, mut on_obj) = (0usize, 0usize);
            for a in &m.areas {
                for p in area_points(a, 0.8) {
                    if p[0] < sq[0] || p[0] >= sq[0] + 100.0 || p[1] < sq[1] || p[1] >= sq[1] + 100.0 {
                        continue;
                    }
                    n += 1;
                    if f64::from(a.values[3]) > hf.height(p[0], p[1]) + 0.5 {
                        on_obj += 1;
                    }
                    *cols
                        .entry(((p[0] / 0.8).round() as i64, (p[1] / 0.8).round() as i64))
                        .or_default() += 1;
                }
            }
            let stacked = cols.values().filter(|v| **v > 1).count();
            f.insert("ground_m2_k", n as f64 * 0.64 / 1000.0);
            f.insert(
                "on_objects_pct",
                if n == 0 { 0.0 } else { on_obj as f64 * 100.0 / n as f64 },
            );
            f.insert("on_objects_m2", on_obj as f64 * 0.64);
            f.insert("stacked_m2", stacked as f64 * 0.64);
        }
        out.push(Row {
            level: lv.to_owned(),
            id: s.id,
            detailed: (m.pitch - 0.2).abs() < 1e-4,
            f,
        });
    }
    out
}

/// For each feature: the best single threshold (detailed when the feature is at least it) over every sector, and the
/// share of sectors it classifies right; then per level the best threshold of the best feature.
fn detailed_report(rows: &[Row], c: &mut C) {
    let mut names: Vec<&'static str> = rows.iter().flat_map(|r| r.f.keys().copied()).collect();
    names.sort_unstable();
    names.dedup();
    let best = |rs: &[&Row], name: &str| -> (f64, usize) {
        let mut vals: Vec<f64> = rs.iter().map(|r| r.f.get(name).copied().unwrap_or(0.0)).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        vals.dedup();
        let mut top = (0.0, 0usize);
        for &t in &vals {
            let ok = rs
                .iter()
                .filter(|r| (r.f.get(name).copied().unwrap_or(0.0) >= t) == r.detailed)
                .count();
            if ok > top.1 {
                top = (t, ok);
            }
        }
        top
    };
    let all: Vec<&Row> = rows.iter().collect();
    if std::env::args().any(|a| a == "--rows") {
        for r in &all {
            let f: Vec<String> = r.f.iter().map(|(k, v)| format!("{k}={v:.2}")).collect();
            println!("row {} {} {} {}", r.level, r.id, u8::from(r.detailed), f.join(" "));
        }
    }
    let det = all.iter().filter(|r| r.detailed).count();
    println!("detailed sectors {det} of {}", all.len());
    for name in &names {
        let (t, ok) = best(&all, name);
        let mean = |d: bool| {
            let v: Vec<f64> = all
                .iter()
                .filter(|r| r.detailed == d)
                .map(|r| r.f.get(name).copied().unwrap_or(0.0))
                .collect();
            v.iter().sum::<f64>() / v.len().max(1) as f64
        };
        println!(
            "  {name:24} threshold {t:9.2}: {ok} of {} right ({:.1} %); mean detailed {:.2}, coarse {:.2}",
            all.len(),
            ok as f64 * 100.0 / all.len() as f64,
            mean(true),
            mean(false)
        );
        c.add(&format!("det.feature.{name}.right"), ok as u64);
    }
    c.add("det.sectors", all.len() as u64);
    // Two features together: on objects (m2) or stacked (m2), the best pair of thresholds.
    let mut levels: Vec<&str> = rows.iter().map(|r| r.level.as_str()).collect();
    levels.dedup();
    for lv in levels {
        let rs: Vec<&Row> = rows.iter().filter(|r| r.level == lv).collect();
        let (t, ok) = best(&rs, "on_objects_m2");
        println!(
            "  {lv:12} {} of {} detailed; on_objects_m2 >= {t:.0}: {ok} of {} right",
            rs.iter().filter(|r| r.detailed).count(),
            rs.len(),
            rs.len()
        );
    }
}

/// The sample points (whole multiples of `step`) inside the area's E + B + C boundary by the even-odd rule.
fn area_points(a: &kymesh::Area, step: f64) -> Vec<P2> {
    let xz = |v: &[f32; 3]| [f64::from(v[0]), f64::from(v[2])];
    let mut segs: Vec<(P2, P2)> = Vec::new();
    for l in &a.outline {
        for w in l.windows(2) {
            segs.push((xz(&w[0]), xz(&w[1])));
        }
    }
    for b in &a.portals {
        segs.push((xz(&b.p0), xz(&b.p1)));
    }
    for b in &a.border_portals {
        segs.push((xz(&b.p0), xz(&b.p1)));
    }
    let mut out = Vec::new();
    if segs.is_empty() {
        return out;
    }
    let z0 = segs.iter().map(|s| s.0[1].min(s.1[1])).fold(f64::MAX, f64::min);
    let z1 = segs.iter().map(|s| s.0[1].max(s.1[1])).fold(f64::MIN, f64::max);
    let mut xs: Vec<f64> = Vec::new();
    for k in (z0 / step).ceil() as i64..=(z1 / step).floor() as i64 {
        let z = k as f64 * step;
        xs.clear();
        for (s0, s1) in &segs {
            let (lo, hi) = if s0[1] < s1[1] { (s0, s1) } else { (s1, s0) };
            if z >= lo[1] && z < hi[1] {
                xs.push(lo[0] + (hi[0] - lo[0]) * (z - lo[1]) / (hi[1] - lo[1]));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for w in xs.chunks(2) {
            if w.len() == 2 {
                for i in (w[0] / step).ceil() as i64..=(w[1] / step).floor() as i64 {
                    out.push([i as f64 * step, z]);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------- KYG-RECT-1: where the ground stops

/// The ground's box (DICE's outline) against the level's data: the combat areas' box, the terrain's extent, the
/// height field's box, and the placed objects' box; printed per file, and counted by how close each side is.
fn fit_rect(lv: &str, veh: bool, sc: &scene::Scene, secs: &[Sec], c: &mut C) {
    let mut gb = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    for s in secs {
        for a in &s.mesh.areas {
            for v in a.outline.iter().flatten() {
                gb = [
                    gb[0].min(f64::from(v[0])),
                    gb[1].max(f64::from(v[0])),
                    gb[2].min(f64::from(v[2])),
                    gb[3].max(f64::from(v[2])),
                ];
            }
        }
    }
    let mut boxes: Vec<(&str, [f64; 4])> = Vec::new();
    let mut ca = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    let mut any = false;
    for sh in &sc.shapes {
        let e = &sc.entities[sh.entity];
        if e.category != "combat area" || sh.kind != "area" {
            continue;
        }
        for p in &sh.points {
            any = true;
            ca = [ca[0].min(p[0]), ca[1].max(p[0]), ca[2].min(p[2]), ca[3].max(p[2])];
        }
    }
    if any {
        boxes.push(("combat_area", ca));
    }
    if let Some(hf) = &sc.heightfield {
        boxes.push((
            "heightfield",
            [
                f64::from(hf.min[0]),
                f64::from(hf.max[0]),
                f64::from(hf.min[2]),
                f64::from(hf.max[2]),
            ],
        ));
    }
    let mut ob = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    if let Some(col) = &sc.collision {
        for (e, _) in &col.meshes {
            if let Some(p) = sc.entities[*e].position() {
                ob = [ob[0].min(p[0]), ob[1].max(p[0]), ob[2].min(p[2]), ob[3].max(p[2])];
            }
        }
    }
    boxes.push(("objects", ob));
    let mut sp = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    for e in &sc.entities {
        if (e.category == "soldier spawn" || e.category == "ai order" || e.category == "vehicle spawn")
            && let Some(p) = e.position()
        {
            sp = [sp[0].min(p[0]), sp[1].max(p[0]), sp[2].min(p[2]), sp[3].max(p[2])];
        }
    }
    boxes.push(("spawns_orders", sp));
    // Per entity category, and for the order's own outline points (area orders' Geometry).
    let mut cats: BTreeMap<String, [f64; 4]> = BTreeMap::new();
    for e in &sc.entities {
        if let Some(p) = e.position() {
            let k = format!("cat:{}", e.category.replace(' ', "_"));
            let b = cats.entry(k).or_insert([f64::MAX, f64::MIN, f64::MAX, f64::MIN]);
            *b = [b[0].min(p[0]), b[1].max(p[0]), b[2].min(p[2]), b[3].max(p[2])];
        }
    }
    for sh in &sc.shapes {
        let e = &sc.entities[sh.entity];
        let k = format!("shape:{}:{}", e.category.replace(' ', "_"), sh.kind.replace(' ', "_"));
        for p in &sh.points {
            let b = cats
                .entry(k.clone())
                .or_insert([f64::MAX, f64::MIN, f64::MAX, f64::MIN]);
            *b = [b[0].min(p[0]), b[1].max(p[0]), b[2].min(p[2]), b[3].max(p[2])];
        }
    }
    let kind0 = if veh { "veh" } else { "inf" };
    for (k, b) in &cats {
        let d = [gb[0] - b[0], b[1] - gb[1], gb[2] - b[2], b[3] - gb[3]];
        println!(
            "rectcat {lv} {kind0} {k} {:.2} {:.2} {:.2} {:.2}",
            d[0], d[1], d[2], d[3]
        );
    }
    // The spawns' box (soldier and vehicle spawns) and the ground's box, raw, for the margin fit.
    let mut sv = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    for e in &sc.entities {
        if (e.category == "soldier spawn" || e.category == "vehicle spawn")
            && let Some(p) = e.position()
        {
            sv = [sv[0].min(p[0]), sv[1].max(p[0]), sv[2].min(p[2]), sv[3].max(p[2])];
        }
    }
    println!(
        "rectraw {lv} {kind0} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
        sv[0], sv[1], sv[2], sv[3], gb[0], gb[1], gb[2], gb[3]
    );
    let kind = if veh { "veh" } else { "inf" };
    let mut line = format!(
        "rect {lv} {kind}: ground x {:.1}..{:.1} z {:.1}..{:.1}",
        gb[0], gb[1], gb[2], gb[3]
    );
    for (name, b) in &boxes {
        // inside distances: how far the ground's side lies inside the box's side (negative: outside)
        let d = [gb[0] - b[0], b[1] - gb[1], gb[2] - b[2], b[3] - gb[3]];
        let _ = write!(line, "; {name} {:.1}/{:.1}/{:.1}/{:.1}", d[0], d[1], d[2], d[3]);
        for x in d {
            c.inc(&format!(
                "rect.{kind}.{name}.side_{}",
                if x < -2.0 {
                    "ground_outside"
                } else if x <= 2.0 {
                    "within_2m"
                } else if x <= 25.0 {
                    "inside_2_25m"
                } else {
                    "inside_more"
                }
            ));
        }
    }
    println!("{line}");
}

// ---------------------------------------------------------------- KYG-NODE-1: nodes against the collision

fn fit_clearance(secs: &[Sec], veh: bool, tris: &Tris, c: &mut C) {
    let (yoff, top) = if veh { (2.0, 4.0) } else { (0.95, 1.9) };
    let mut polys = Vec::new();
    for s in secs {
        let cl = class(&s.mesh, veh);
        for n in &s.nodes {
            let tag = if n.id & 0x8000_0000 != 0 { "b" } else { "o" };
            let g = f64::from(n.pos[1]) - yoff;
            let p = [f64::from(n.pos[0]), f64::from(n.pos[2])];
            for (what, q) in [("node", p), ("control", [p[0] + 1.7, p[1] - 1.1])] {
                tris.band(q, 3.0, g + 0.3, g + top, &mut polys);
                let (d, dv) = dist_polys(q, &polys);
                c.inc(&format!("clear.{cl}.{tag}.{what}.d_{}", band_cm(d, 5.0, 300)));
                c.inc(&format!("clear.{cl}.{tag}.{what}.vertex_{}", band_cm(dv, 5.0, 300)));
            }
        }
    }
}
