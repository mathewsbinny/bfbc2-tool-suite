//! Round NKG3's fit of DICE's D lines and F cells (`spec/transforms/kynapse_generation.md` KYG-DF-1): DICE's
//! infantry path data (read-only) beside NavKit's sample runs and the level's height field. Counts only, no content
//! printed.
//!
//!   cargo run --release -p bc2-navbuild --example kynapse_df -- [<level>...] [--opfor <folder>] [--cand <folder>]
//!
//! Default: every in-area side of DICE's areas (two straight-neighbour cells of one area, DICE's cells from the area
//! boundaries, heights from `work\navsample\<lv>_full.bin` or `_navkit.bin` at pitch 0.8, `_navkit_p02.bin` at 0.2)
//! against DICE's label (a D side either way, a side of an F cell along its axis): the step rule and, between two
//! terrain cells, the slope rule (`stage::terrain_slope` on the level's height field), as true / false positives and
//! misses, and the D / F share by slope near the threshold. `--cand <folder>`: where each of DICE's D / F sides stands
//! in NavKit's build `<folder>\<lv>.hpd` (matched, a cell not ground, the other pitch there, split between areas, or
//! one area without a break, by the sampled step), and each of NavKit's in DICE's file.
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
    clippy::type_complexity,
    missing_docs
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use bc2_formats::{hpd, kymesh};
use bc2_navbuild::navsample;
use bc2_navbuild::parity::area_cells;
use bc2_navbuild::stage::{DF_TERRAIN_SLOPE, terrain_slope};

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

fn opfor_root(given: Option<&str>) -> PathBuf {
    if let Some(g) = given {
        return PathBuf::from(g);
    }
    if let Ok(v) = std::env::var("OPFOR_ROOT") {
        return PathBuf::from(v);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("project-opfor-bfbc2")
}

/// A sample run: per column its surfaces (y, normal, flags).
struct Run {
    i0: i64,
    j0: i64,
    nx: i64,
    nz: i64,
    runs: navsample::Runs,
}

impl Run {
    fn load(path: &Path) -> Option<Self> {
        let d = fs::read(path).ok()?;
        let u32at = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let f32at = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let version = u32at(4);
        let pitch = f64::from(f32at(8));
        let (minx, minz) = (f64::from(f32at(12)), f64::from(f32at(16)));
        let (nx, nz) = (i64::from(u32at(20) as i32), i64::from(u32at(24) as i32));
        let runs = if version == 3 {
            navsample::parse_v3(&d, nx, nz).ok()?
        } else {
            navsample::parse_runs(&d, navsample::HEADER_SIZE, (nx * nz) as usize, version).ok()?
        };
        Some(Self {
            i0: (minx / pitch).round() as i64,
            j0: (minz / pitch).round() as i64,
            nx,
            nz,
            runs,
        })
    }

    /// The surfaces of world cell (i, j): indices into the records.
    fn surfaces(&self, i: i64, j: i64) -> std::ops::Range<usize> {
        let (ci, cj) = (i - self.i0, j - self.j0);
        if ci < 0 || cj < 0 || ci >= self.nx || cj >= self.nz {
            return 0..0;
        }
        let c = (cj * self.nx + ci) as usize;
        if c + 1 >= self.runs.start.len() {
            return 0..0;
        }
        self.runs.start[c] as usize..self.runs.start[c + 1] as usize
    }

    /// The surface of (i, j) nearest `y` within `tol`: (height, normal, flags).
    fn near(&self, i: i64, j: i64, y: f64, tol: f64) -> Option<(f64, [f64; 3], u8)> {
        let mut best: Option<(f64, usize)> = None;
        for k in self.surfaces(i, j) {
            let d = (f64::from(self.runs.y[k]) - y).abs();
            if d <= tol && best.is_none_or(|b| d < b.0) {
                best = Some((d, k));
            }
        }
        best.map(|(_, k)| {
            (
                f64::from(self.runs.y[k]),
                [
                    f64::from(self.runs.normal[3 * k]),
                    f64::from(self.runs.normal[3 * k + 1]),
                    f64::from(self.runs.normal[3 * k + 2]),
                ],
                self.runs.flags[k]
                    | if self.runs.hit.get(3 * k) == Some(&0x0145_4D14) {
                        0x80
                    } else {
                        0
                    },
            )
        })
    }
}

type P3 = (f64, f64, f64);

fn p3(v: kymesh::Vec3) -> P3 {
    (f64::from(v[0]), f64::from(v[1]), f64::from(v[2]))
}

fn segments(a: &kymesh::Area) -> Vec<(P3, P3)> {
    let mut out = Vec::new();
    for l in &a.outline {
        out.extend(l.iter().zip(l.iter().skip(1)).map(|(&a, &b)| (p3(a), p3(b))));
    }
    out.extend(a.portals.iter().map(|q| (p3(q.p0), p3(q.p1))));
    out.extend(a.border_portals.iter().map(|q| (p3(q.p0), p3(q.p1))));
    out
}

fn in_poly(x: f64, z: f64, pts: &[kymesh::Vec3]) -> bool {
    let mut inside = false;
    let n = pts.len();
    for k in 0..n {
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        let (ax, az, bx, bz) = (f64::from(a[0]), f64::from(a[2]), f64::from(b[0]), f64::from(b[2]));
        if (az > z) != (bz > z) && x < ax + (z - az) * (bx - ax) / (bz - az) {
            inside = !inside;
        }
    }
    inside
}

type Cell = (i64, i64);

/// One DICE area's cells and labels.
struct AreaDf {
    pitch: f64,
    /// cell -> ground height (nearest boundary vertex - 0.95)
    cells: HashMap<Cell, f64>,
    /// D unit sides: (owner, direction to the neighbour); diagonal pieces (corner cuts) left out
    d: HashSet<(Cell, Cell)>,
    fx: HashSet<Cell>,
    fz: HashSet<Cell>,
}

fn area_df(a: &kymesh::Area) -> AreaDf {
    let p = f64::from(a.values[1]);
    let cells: HashMap<Cell, f64> = area_cells(&segments(a), p, 0.95).into_iter().collect();
    let mut d = HashSet::new();
    for line in &a.d_lines {
        for w in line.windows(2) {
            let (p0, p1) = (p3(w[0]), p3(w[1]));
            let (dx, dz) = (p1.0 - p0.0, p1.2 - p0.2);
            let l = dx.hypot(dz);
            if l < 1e-6 {
                continue;
            }
            if dx.abs() > 1e-3 && dz.abs() > 1e-3 {
                continue;
            }
            let n = (l / p).round().max(1.0) as i64;
            let (ux, uz) = (dx / l, dz / l);
            for k in 0..n {
                let t = (k as f64 + 0.5) / n as f64;
                let (mx, mz) = (p0.0 + dx * t, p0.2 + dz * t);
                // owner on the right of the direction of travel (x right, z up): (uz, -ux)
                let (ox, oz) = (mx + uz * p / 2.0, mz - ux * p / 2.0);
                let owner = ((ox / p).round() as i64, (oz / p).round() as i64);
                let nb = (
                    ((mx - uz * p / 2.0) / p).round() as i64,
                    ((mz + ux * p / 2.0) / p).round() as i64,
                );
                let dir = (nb.0 - owner.0, nb.1 - owner.1);
                d.insert((owner, dir));
            }
        }
    }
    let (mut fx, mut fz) = (HashSet::new(), HashSet::new());
    for c in &a.cells {
        let pts = &c.points;
        let xs = pts.iter().map(|q| f64::from(q[0]));
        let zs = pts.iter().map(|q| f64::from(q[2]));
        let (x0, x1) = (xs.clone().fold(f64::MAX, f64::min), xs.fold(f64::MIN, f64::max));
        let (z0, z1) = (zs.clone().fold(f64::MAX, f64::min), zs.fold(f64::MIN, f64::max));
        let xaxis = f64::from(c.direction[0]).abs() > 0.5;
        for i in (x0 / p).floor() as i64..=(x1 / p).ceil() as i64 {
            for j in (z0 / p).floor() as i64..=(z1 / p).ceil() as i64 {
                if in_poly(i as f64 * p + p * 1.37e-3, j as f64 * p + p * 0.71e-3, pts) {
                    if xaxis {
                        fx.insert((i, j));
                    } else {
                        fz.insert((i, j));
                    }
                }
            }
        }
    }
    AreaDf {
        pitch: p,
        cells,
        d,
        fx,
        fz,
    }
}

fn bin(v: f64, w: f64, max: f64) -> String {
    let b = (v.min(max) / w).floor() * w;
    format!("{b:5.2}")
}

/// Ground height of each area cell from the run: the surface nearest the area's estimate, then re-fitted to the
/// mean of the neighbours' picks (two passes).
fn pick_heights(ad: &AreaDf, run: &Run) -> HashMap<Cell, (f64, [f64; 3], u8)> {
    let mut out: HashMap<Cell, (f64, [f64; 3], u8)> = HashMap::new();
    for (&c, &h) in &ad.cells {
        if let Some(s) = run.near(c.0, c.1, h, 1.5) {
            out.insert(c, s);
        }
    }
    for _ in 0..2 {
        let mut next = HashMap::new();
        for &c in ad.cells.keys() {
            let nb: Vec<f64> = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .filter_map(|d| out.get(&(c.0 + d.0, c.1 + d.1)).map(|s| s.0))
                .collect();
            let est = match out.get(&c) {
                Some(s) if nb.is_empty() => s.0,
                Some(s) => {
                    // keep the pick unless a surface lies much nearer the neighbours
                    let m = nb.iter().sum::<f64>() / nb.len() as f64;
                    if (s.0 - m).abs() > 1.0 { m } else { s.0 }
                }
                None => ad.cells[&c],
            };
            if let Some(s) = run.near(c.0, c.1, est, 1.5) {
                next.insert(c, s);
            }
        }
        out = next;
    }
    out
}

/// A file's areas by cell and its D / F sides, per pitch class (0 normal, 1 detailed).
#[derive(Default)]
struct Index {
    /// (class, cell) -> (area id, ground height)
    cells: HashMap<(u8, Cell), Vec<(usize, f64)>>,
    /// (class, low cell, axis) -> height of the low cell's side: a D or F side (unordered)
    df: HashMap<(u8, Cell, u8), Vec<f64>>,
}

fn side_key(cls: u8, a: Cell, b: Cell) -> (u8, Cell, u8) {
    let lo = if a <= b { a } else { b };
    (cls, lo, u8::from(a.1 != b.1))
}

fn index(data: &[u8]) -> Index {
    let f = hpd::parse(data).expect("hpd");
    let mut ix = Index::default();
    let mut gid = 0usize;
    for s in &f.sectors {
        let Ok(mesh) = kymesh::parse(s.mesh) else { continue };
        for a in &mesh.areas {
            let ad = area_df(a);
            let cls = u8::from(ad.pitch < 0.5);
            for (&c, &h) in &ad.cells {
                ix.cells.entry((cls, c)).or_default().push((gid, h));
            }
            for &(o, dir) in &ad.d {
                let nb = (o.0 + dir.0, o.1 + dir.1);
                let h = ad.cells.get(&o).copied().unwrap_or(f64::NAN);
                ix.df.entry(side_key(cls, o, nb)).or_default().push(h);
            }
            for (set, dir) in [(&ad.fx, (1, 0)), (&ad.fz, (0, 1))] {
                for &c in set {
                    let h = ad.cells.get(&c).copied().unwrap_or(f64::NAN);
                    for nb in [(c.0 + dir.0, c.1 + dir.1), (c.0 - dir.0, c.1 - dir.1)] {
                        ix.df.entry(side_key(cls, c, nb)).or_default().push(h);
                    }
                }
            }
            gid += 1;
        }
    }
    ix
}

/// Where a side of `from` stands in `to`: matched, or why not.
fn classify(ix_to: &Index, key: (u8, Cell, u8), h: f64, run: Option<&Run>) -> String {
    let (cls, lo, axis) = key;
    let hi = if axis == 0 { (lo.0 + 1, lo.1) } else { (lo.0, lo.1 + 1) };
    if ix_to
        .df
        .get(&key)
        .is_some_and(|v| v.iter().any(|&y| h.is_nan() || y.is_nan() || (y - h).abs() <= 1.0))
    {
        return "match".into();
    }
    let near = |c: Cell| -> Vec<(usize, f64)> {
        ix_to.cells.get(&(cls, c)).map_or_else(Vec::new, |v| {
            v.iter()
                .copied()
                .filter(|&(_, y)| h.is_nan() || (y - h).abs() <= 1.5)
                .collect()
        })
    };
    let (a, b) = (near(lo), near(hi));
    if a.is_empty() && b.is_empty() {
        let other = 1 - cls;
        let f = if other == 1 { 4 } else { 1 };
        let oc = (
            lo.0 * if other == 1 { 4 } else { 1 } / if other == 1 { 1 } else { 4 },
            lo.1 * f / if other == 1 { 1 } else { 4 },
        );
        if ix_to
            .cells
            .get(&(other, oc))
            .is_some_and(|v| v.iter().any(|&(_, y)| h.is_nan() || (y - h).abs() <= 1.5))
        {
            return "other pitch there".into();
        }
        return "neither ground".into();
    }
    if a.is_empty() || b.is_empty() {
        return "one not ground".into();
    }
    if !a.iter().any(|x| b.iter().any(|y| x.0 == y.0)) {
        return "split between areas".into();
    }
    let step = run.and_then(|r| {
        let sa = r.near(lo.0, lo.1, a[0].1, 1.5)?;
        let sb = r.near(hi.0, hi.1, b[0].1, 1.5)?;
        Some((sa.0 - sb.0).abs())
    });
    match step {
        Some(s) => format!("same area, step {}", bin(s, 0.05, 1.0)),
        None => "same area, unsampled".into(),
    }
}

fn cross(c: &mut C, dice: &Index, cand: &Index, run08: Option<&Run>, run02: Option<&Run>) {
    for (who, from, to) in [("recall", dice, cand), ("precision", cand, dice)] {
        for (&key, hs) in &from.df {
            let run = if key.0 == 1 { run02 } else { run08 };
            let h = hs.first().copied().unwrap_or(f64::NAN);
            let k = classify(to, key, h, run);
            c.inc(&format!("{who} {} {k}", ["n", "d"][key.0 as usize]));
        }
    }
}

/// A rule's answer for one side against DICE's label: true / false positives, misses (D and F apart).
fn rule(c: &mut C, name: &str, says: bool, lab: &str) {
    let k = match (says, lab) {
        (true, "-") => "false positive",
        (true, _) => "true positive",
        (false, "-") => return,
        (false, "D") => "missed D",
        (false, _) => "missed F",
    };
    c.inc(&format!("{name}: {k}"));
    if says && lab != "-" {
        c.inc(&format!("{name}: found {lab}"));
    }
}

/// The level's terrain height field, if unpacked.
fn heightfield(root: &Path, lv: &str) -> Option<bc2_formats::terrain::Heightfield> {
    fn walk(d: &Path, out: &mut Option<PathBuf>) {
        let Ok(rd) = fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase().ends_with(".heightfield-000.res"))
                && out.is_none()
            {
                *out = Some(p);
            }
        }
    }
    let mut found = None;
    walk(&root.join("work/levels").join(lv).join("terrain"), &mut found);
    bc2_formats::terrain::parse(&fs::read(found?).ok()?).ok()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut levels = Vec::new();
    let mut opfor = None;
    let mut cand_dir: Option<String> = None;
    let mut before_dir: Option<String> = None;
    let mut k = 0;
    while k < args.len() {
        match args[k].as_str() {
            "--opfor" => {
                opfor = args.get(k + 1).cloned();
                k += 1;
            }
            "--cand" => {
                cand_dir = args.get(k + 1).cloned();
                k += 1;
            }
            "--before" => {
                before_dir = args.get(k + 1).cloned();
                k += 1;
            }
            s => levels.push(s.to_owned()),
        }
        k += 1;
    }
    let root = opfor_root(opfor.as_deref());
    if levels.is_empty() {
        levels = [
            "sp_01",
            "sp_02",
            "sp_03",
            "sp_04",
            "sp_003_b",
            "sp_004_b",
            "sp_005",
            "sp_005_b",
            "sp_006",
            "sp_007",
            "sp_008",
            "sp_008_b",
            "sp_009",
            "ons_mp_002",
            "ons_mp_004",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    }
    let mut c = C::default();
    for lv in &levels {
        let hpd_path = root
            .join("game/Output/win32/ai/levels")
            .join(lv)
            .join("infantry/pathdata.hpd");
        let Ok(data) = fs::read(&hpd_path) else {
            println!("{lv}: no DICE file");
            continue;
        };
        let f = hpd::parse(&data).expect("hpd");
        let ns = root.join("work/navsample");
        let pick = |names: &[&str]| names.iter().map(|n| ns.join(n)).find(|p| p.is_file());
        let n08 = pick(&[&format!("{lv}_full.bin"), &format!("{lv}_navkit.bin")]).and_then(|p| Run::load(&p));
        let n02 = pick(&[&format!("{lv}_navkit_p02.bin")]).and_then(|p| Run::load(&p));
        let hf = heightfield(&root, lv);
        println!(
            "{lv}: runs 0.8 {}, 0.2 {}, height field {:?}",
            n08.is_some(),
            n02.is_some(),
            hf.as_ref().map(|h| (h.cell(), h.min, h.size))
        );
        if let Some(cd) = &cand_dir {
            let Ok(cdata) = fs::read(Path::new(cd).join(format!("{lv}.hpd"))) else {
                println!("{lv}: no candidate");
                continue;
            };
            let (di, ci) = (index(&data), index(&cdata));
            cross(&mut c, &di, &ci, n08.as_ref(), n02.as_ref());
            // DICE's sides an earlier build matched and this one does not (and the reverse), by the side's slope
            if let (Some(bd), Some(hf)) = (&before_dir, &hf) {
                let Ok(bdata) = fs::read(Path::new(bd).join(format!("{lv}.hpd"))) else {
                    continue;
                };
                let bi = index(&bdata);
                for (&key, hs) in &di.df {
                    let h = hs.first().copied().unwrap_or(f64::NAN);
                    let (was, is) = (
                        classify(&bi, key, h, None) == "match",
                        classify(&ci, key, h, None) == "match",
                    );
                    if was == is || key.0 != 0 {
                        continue;
                    }
                    let (_, lo, axis) = key;
                    let dir = if axis == 0 { (1, 0) } else { (0, 1) };
                    let (mx, mz) = (
                        (lo.0 as f64 + dir.0 as f64 / 2.0) * 0.8,
                        (lo.1 as f64 + dir.1 as f64 / 2.0) * 0.8,
                    );
                    let g = terrain_slope(hf, mx, mz, dir);
                    let hi = (lo.0 + dir.0, lo.1 + dir.1);
                    let st = n08.as_ref().and_then(|r| {
                        let a = r.near(lo.0, lo.1, h, 1.5)?;
                        let b = r.near(hi.0, hi.1, a.0, 1.5)?;
                        Some(((a.0 - b.0).abs(), a.2 & b.2 & 0x80 != 0))
                    });
                    c.inc(&format!(
                        "{lv} {} slope {} step {:?}",
                        if was { "lost" } else { "gained" },
                        bin(g, 0.05, 1.5),
                        st.map(|(s, t)| (bin(s, 0.05, 1.5), t))
                    ));
                }
            }
            continue;
        }
        for s in &f.sectors {
            let Ok(mesh) = kymesh::parse(s.mesh) else { continue };
            for a in &mesh.areas {
                let ad = area_df(a);
                let cls = if ad.pitch < 0.5 { "d" } else { "n" };
                let run = if cls == "d" { &n02 } else { &n08 };
                // D sides whose owner or neighbour is not an area cell (45-degree cut half cells)
                for &(o, dir) in &ad.d {
                    let nb = (o.0 + dir.0, o.1 + dir.1);
                    let k = match (ad.cells.contains_key(&o), ad.cells.contains_key(&nb)) {
                        (true, true) => "both in area",
                        (true, false) => "neighbour outside",
                        (false, true) => "owner outside",
                        (false, false) => "neither",
                    };
                    c.inc(&format!("{cls} D unit side: {k}"));
                }
                let Some(run) = run else { continue };
                let hs = pick_heights(&ad, run);
                for (&cell, s0) in &hs {
                    for dir in [(1, 0), (0, 1)] {
                        let nb = (cell.0 + dir.0, cell.1 + dir.1);
                        let Some(s1) = hs.get(&nb) else { continue };
                        let back = (-dir.0, -dir.1);
                        let dd = ad.d.contains(&(cell, dir)) || ad.d.contains(&(nb, back));
                        let ff = if dir.0 != 0 {
                            ad.fx.contains(&cell) || ad.fx.contains(&nb)
                        } else {
                            ad.fz.contains(&cell) || ad.fz.contains(&nb)
                        };
                        let lab = if dd {
                            "D"
                        } else if ff {
                            "F"
                        } else {
                            "-"
                        };
                        let step = (s1.0 - s0.0).abs();
                        let terrain = s0.2 & 0x80 != 0 && s1.2 & 0x80 != 0;
                        let kind = if terrain { "terrain" } else { "object" };
                        let step_t = if cls == "d" { 0.175 } else { 0.6 };
                        // the step rule against DICE's label
                        rule(&mut c, &format!("{cls} {kind} step > {step_t}"), step > step_t, lab);
                        if let (true, Some(hf)) = (terrain, &hf) {
                            let p = ad.pitch;
                            let (mx, mz) = (
                                (cell.0 as f64 + dir.0 as f64 / 2.0) * p,
                                (cell.1 as f64 + dir.1 as f64 / 2.0) * p,
                            );
                            let g = terrain_slope(hf, mx, mz, dir);
                            let slope_t = if cls == "d" { 0.915 } else { DF_TERRAIN_SLOPE };
                            rule(&mut c, &format!("{cls} terrain slope > {slope_t}"), g > slope_t, lab);
                            rule(
                                &mut c,
                                &format!("{cls} terrain slope or step"),
                                g > slope_t || step > step_t,
                                lab,
                            );
                            for lo in [0.6, 0.65, 0.7, 0.74] {
                                rule(
                                    &mut c,
                                    &format!("{cls} terrain slope, or step and slope > {lo}"),
                                    g > slope_t || (step > step_t && g > lo),
                                    lab,
                                );
                            }
                            // F cells by the step (both sides of the axis), D sides by the slope
                            let stepf = |a: Cell| {
                                let y = hs.get(&a).map(|s| s.0);
                                let up = hs.get(&(a.0 + dir.0, a.1 + dir.1)).map(|s| s.0);
                                let dn = hs.get(&(a.0 - dir.0, a.1 - dir.1)).map(|s| s.0);
                                matches!((y, up, dn), (Some(y), Some(u), Some(d)) if (u - y).abs() > step_t && (d - y).abs() > step_t)
                            };
                            let fside = stepf(cell) || stepf(nb);
                            rule(
                                &mut c,
                                &format!("{cls} terrain F by step, D by slope"),
                                fside || g > slope_t,
                                lab,
                            );
                            if (0.74..0.80).contains(&g) {
                                let b = (g * 200.0).floor() / 200.0;
                                c.inc(&format!(
                                    "{cls} terrain slope {b:.3} {}",
                                    if lab == "-" { "none" } else { "D/F" }
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    for (k, v) in &c.0 {
        println!("{k}: {v}");
    }
}
