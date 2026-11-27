//! The generated terrain's visual side (round TG2; spec TGEN-VIS-1 / -3 / -4, TGEN-WATER-1, TGEN-OCC-1, TGEN-REQ-4 /
//! -7): from one donor terrain of the biome (TGEN-BIOME-4) - its visual terrain, visual water, mask atlas, colour /
//! occlusion map and the level's three shader databases, as read from the game copy - the new terrain's own:
//!
//! | Resource | Bundle | Made |
//! |---|---|---|
//! | `Terrains/<T>/<T>.Visual.res` | terrain | the donor's patch shaders renamed (TGEN-REQ-4), empty road blocks, a patch map per block: each cell the donor's rule-only shader its ground asks for ([`pick`], TGEN-VIS-3), 4 x 4 cells a ring block |
//! | `Terrains/<T>/<T>.MaskTileAtlas.res` | terrain | the donor's atlas (its layout in every patch map's header, TERR-VT-5) |
//! | `Terrains/<T>/<T>.Water.res` | terrain | one open-water quad at the water level over the outer square, DICE's open-water vertices (WATER-VIS-6), the donor's slots renamed |
//! | `Terrains/<T>/Textures/<T>_AmbientOcclusion.res` | level | sky visibility over 2,048 m mapped onto the donor map's own distribution, in its format and size ([`occlusion`]) |
//! | `Levels/<T>/shaders/Dx9/10/11_Single.res` | level | the donor level's databases with the donor terrain's entries cloned under the new name, the occlusion map retargeted ([`bc2_formats::shaderdb::terrain`]) |

use std::collections::BTreeMap;

use rayon::prelude::*;

use bc2_formats::itexture::{self, Format, Picture};
use bc2_formats::shaderdb::{open, terrain as sterrain};
use bc2_formats::visual_terrain::{self, PatchMap, VisualTerrain};
use bc2_formats::water::{self, VisualWaterFile};

use crate::paint::STEEP_DEG;
use crate::ring::RING_N;
use crate::shape::{percentile_sorted, slopes};
use crate::{Resource, Terrain};

/// What a rule-only patch shader draws, from its layer names (TGEN-VIS-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuleKind {
    /// A sea-floor layer (`Seafloor`, `Riverbed`, `Underwater`).
    pub sea: bool,
    /// A slope layer (`Mountain`, `Cliff`, `Slopes`).
    pub slope: bool,
    /// A destruction layer (`Destruction`, `Destr`, `dest`, `dst`).
    pub destruction: bool,
}

/// The kind of a patch shader by its entry's layer names (the part after `PatchMaskShaders/`, any case).
#[must_use]
pub fn rule_kind(entry: &str) -> RuleKind {
    let low = entry.to_ascii_lowercase();
    let layers = low.rsplit_once("/patchmaskshaders/").map_or(low.as_str(), |x| x.1);
    let has = |w: &[&str]| w.iter().any(|k| layers.contains(k));
    RuleKind {
        sea: has(&["seafloor", "sea_floor", "riverbed", "underwater"]),
        slope: has(&["mountain", "cliff", "slope"]),
        destruction: has(&["destr", "dest", "dst"]),
    }
}

/// The rule-only patch shader (tiled count 0) a cell that wants `want` (its sea and slope) draws with: the fewest
/// mismatches (a sea floor missed counts double), then one with a destruction layer, then the one the donor's own
/// main patch map uses most, then the first. `None` when the donor has no rule-only shader (DICE's 30 all have:
/// TGEN-VIS-1).
#[must_use]
pub fn pick(shaders: &[visual_terrain::PatchShader], usage: &[usize], want: RuleKind) -> Option<u8> {
    shaders
        .iter()
        .enumerate()
        .filter(|(_, s)| s.tiled == 0)
        .min_by_key(|(i, s)| {
            let k = rule_kind(&s.name);
            (
                2 * usize::from(k.sea != want.sea) + usize::from(k.slope != want.slope),
                !k.destruction,
                std::cmp::Reverse(usage.get(*i).copied().unwrap_or(0)),
                *i,
            )
        })
        .and_then(|(i, _)| u8::try_from(i).ok())
}

/// Each `side` x `side` cell's wants over `n` x `n` heights (metres, rows along z): a sea floor where a sample of the
/// cell (with the row and column after it) lies under `water_y`, a slope layer where one is steeper than
/// [`STEEP_DEG`] (the material paint's rock: TGEN-MAT-2).
#[must_use]
pub fn cell_wants(h: &[f64], n: usize, cell_m: f64, side: usize, water_y: Option<f64>) -> Vec<RuleKind> {
    let sl = slopes(h, n, cell_m);
    let q = (n / side).max(1);
    (0..side * side)
        .into_par_iter()
        .map(|c| {
            let (cx, cz) = (c % side, c / side);
            let mut k = RuleKind::default();
            for j in cz * q..((cz + 1) * q + 1).min(n) {
                for i in cx * q..((cx + 1) * q + 1).min(n) {
                    let at = j * n + i;
                    k.sea |= water_y.is_some_and(|w| h.get(at).is_some_and(|v| *v < w));
                    k.slope |= sl.get(at).is_some_and(|s| *s > STEEP_DEG);
                }
            }
            k
        })
        .collect()
}

/// A donor terrain's files (TGEN-BIOME-4).
#[derive(Debug, Clone, Default)]
pub struct Donor<'a> {
    /// The donor terrain's name (`MP_005`).
    pub terrain: String,
    /// Its `VisualTerrain`.
    pub visual: &'a [u8],
    /// Its `VisualWater`, if it has one.
    pub water: Option<&'a [u8]>,
    /// Its mask atlas (`ITexture`).
    pub atlas: Option<&'a [u8]>,
    /// Its colour / occlusion map: the name its patch shaders sample (`Terrains/MP_005/Textures/mp05_AmbientOcclusion`)
    /// and the `ITexture`.
    pub occlusion: Option<(String, &'a [u8])>,
    /// The donor level's shader databases: (9, 10 or 11, the file).
    pub databases: Vec<(u32, &'a [u8])>,
}

/// What [`build`] made.
#[derive(Debug, Clone, Default)]
pub struct Visual {
    /// The terrain bundle's: the visual terrain, the mask atlas, the visual water.
    pub terrain: Vec<Resource>,
    /// The level bundle's: the colour / occlusion map, the three databases.
    pub level: Vec<Resource>,
    /// Lines for the report (patch shaders used, entries cloned, the map's statistics).
    pub notes: Vec<String>,
    /// The main patch map's cells per patch shader index.
    pub cells: BTreeMap<u8, usize>,
}

fn res(name: String, type_name: &'static str, data: Vec<u8>) -> Resource {
    Resource { name, type_name, data }
}

/// The world heights the colour / occlusion map and the cells see: the main block inside, the ring's nearest sample
/// outside (the main block's nearest edge without a ring).
fn world_height(t: &Terrain, x: f64, z: f64) -> f64 {
    let n = t.height.size as usize;
    let side = n as f64;
    let scale = t.height.range_m / 65535.0;
    let main = |x: f64, z: f64| {
        let i = (x + side / 2.0).floor().clamp(0.0, side - 1.0) as usize;
        let j = (z + side / 2.0).floor().clamp(0.0, side - 1.0) as usize;
        f64::from(t.height.raw.get(j * n + i).copied().unwrap_or(0)) * scale
    };
    if x.abs() < side / 2.0 && z.abs() < side / 2.0 || t.ring.is_empty() {
        return main(x, z);
    }
    for b in &t.ring {
        let [x0, _, z0, x1, _, z1] = b.bounds.map(f64::from);
        if x >= x0 && x < x1 && z >= z0 && z < z1 {
            let sp = (x1 - x0) / RING_N as f64;
            let i = ((x - x0) / sp).floor().clamp(0.0, (RING_N - 1) as f64) as usize;
            let j = ((z - z0) / sp).floor().clamp(0.0, (RING_N - 1) as f64) as usize;
            return f64::from(b.raw.get(j * RING_N + i).copied().unwrap_or(0)) * scale;
        }
    }
    let lim = side;
    main(x.clamp(-lim, lim), z.clamp(-lim, lim))
}

/// The world square the colour / occlusion map covers (LIGHT-BAKE-2: 2,048 m centred on the origin, sampled at
/// `world / 2048 + 0.5`).
pub const OCCLUSION_M: f64 = 2048.0;

/// Cosine-weighted sky visibility per texel of a `w` x `w` grid over [`OCCLUSION_M`]: for 16 directions the highest
/// horizon over 2 - 64 texels, `cos^2` of its elevation, averaged.
#[must_use]
pub fn sky_visibility(t: &Terrain, w: usize) -> Vec<f64> {
    sky_visibility_over(&|x, z| world_height(t, x, z), w, None)
}

/// The farthest a texel's horizon search reaches, in texels ([`sky_visibility`]'s largest radius).
pub const HORIZON_TEXELS: usize = 64;

/// [`sky_visibility`] of any ground (`height(x, z)` in metres at world x, z; round E5-rest's adapter for the
/// editor's height field) over the texels of `area` (columns, rows: `[x0, z0, x1, z1)` of the `w` x `w` grid; the
/// whole grid when `None`), row by row. Only the heights within [`HORIZON_TEXELS`] of the area are read, so a re-bake
/// of an edited region costs that region.
#[must_use]
pub fn sky_visibility_over(height: &(dyn Fn(f64, f64) -> f64 + Sync), w: usize, area: Option<[usize; 4]>) -> Vec<f64> {
    let [ax0, az0, ax1, az1] = area.map_or([0, 0, w, w], |a| a.map(|v| v.min(w)));
    // the heights read: the area and the horizon around it
    let (hx0, hz0) = (ax0.saturating_sub(HORIZON_TEXELS), az0.saturating_sub(HORIZON_TEXELS));
    let (hx1, hz1) = ((ax1 + HORIZON_TEXELS).min(w), (az1 + HORIZON_TEXELS).min(w));
    let hw = hx1.saturating_sub(hx0);
    let step = OCCLUSION_M / w as f64;
    let h: Vec<f64> = (0..hw * hz1.saturating_sub(hz0))
        .into_par_iter()
        .map(|k| {
            let (i, j) = (hx0 + k % hw, hz0 + k / hw);
            height(
                (i as f64 + 0.5) * step - OCCLUSION_M / 2.0,
                (j as f64 + 0.5) * step - OCCLUSION_M / 2.0,
            )
        })
        .collect();
    let at = |i: usize, j: usize| {
        let (di, dj) = (i.checked_sub(hx0)?, j.checked_sub(hz0)?);
        if di >= hw {
            return None;
        }
        h.get(dj * hw + di).copied()
    };
    let radii = [2.0f64, 4.0, 8.0, 16.0, 32.0, 64.0];
    let dirs: Vec<(f64, f64)> = (0..16)
        .map(|a| {
            let th = std::f64::consts::TAU * f64::from(a) / 16.0;
            (th.cos(), th.sin())
        })
        .collect();
    let aw = ax1.saturating_sub(ax0);
    (0..aw * az1.saturating_sub(az0))
        .into_par_iter()
        .map(|k| {
            let (ii, jj) = (ax0 + k % aw, az0 + k / aw);
            let (i, j) = (ii as f64, jj as f64);
            let h0 = at(ii, jj).unwrap_or(0.0);
            let mut vis = 0.0;
            for &(dx, dz) in &dirs {
                let mut hor = 0.0f64;
                for r in radii {
                    let (x, z) = ((i + dx * r).round(), (j + dz * r).round());
                    if x < 0.0 || z < 0.0 || x >= w as f64 || z >= w as f64 {
                        continue;
                    }
                    let d = (dx * r).hypot(dz * r) * step;
                    let other = at(x as usize, z as usize).unwrap_or(h0);
                    hor = hor.max(((other - h0) / d).atan());
                }
                vis += hor.cos().powi(2);
            }
            vis / dirs.len() as f64
        })
        .collect()
}

/// The generated terrain's colour / occlusion map (TGEN-OCC-1, LIGHT-BAKE-2): DICE painted its maps (computed sky
/// visibility explains 0 - 27 % of a map's red), so the red channel is the terrain's own sky visibility ranked and
/// given the donor map's red values at the same ranks (its distribution exactly, its look on this ground); green and
/// blue the donor's medians, or the red where the donor's map is grey (L8, or every texel R = G = B). The donor's
/// format, size, flags and mip chain.
///
/// # Errors
/// The donor's map is in a format this suite does not write (DXT1 and L8 are DICE's on 13 of 14 maps).
pub fn occlusion(t: &Terrain, donor: &[u8]) -> Result<(Vec<u8>, String), String> {
    let h = itexture::Header::read(donor).ok_or("the donor's colour / occlusion map is not a PC texture")?;
    let f = Format::from_code(h.format).ok_or_else(|| {
        format!(
            "the donor's colour / occlusion map is format {}, not DXT1 or L8",
            h.format
        )
    })?;
    let pic = itexture::decode_level(donor, 0).ok_or("the donor's colour / occlusion map does not decode")?;
    let w = pic.w;
    let grey = pic.rgb.iter().all(|p| p[0] == p[1] && p[1] == p[2]);
    let mut reds: Vec<u8> = pic.rgb.iter().map(|p| p[0]).collect();
    reds.par_sort_unstable();
    let med = |c: usize| {
        let mut v: Vec<u8> = pic.rgb.iter().map(|p| p.get(c).copied().unwrap_or(0)).collect();
        v.par_sort_unstable();
        v.get(v.len() / 2).copied().unwrap_or(0)
    };
    let (g, b) = (med(1), med(2));
    let vis = sky_visibility(t, w);
    let mut order: Vec<u32> = (0..vis.len() as u32).collect();
    order.par_sort_unstable_by(|a, c| {
        let (x, y) = (
            vis.get(*a as usize).copied().unwrap_or(0.0),
            vis.get(*c as usize).copied().unwrap_or(0.0),
        );
        x.total_cmp(&y).then(a.cmp(c))
    });
    let mut red = vec![0u8; vis.len()];
    for (rank, k) in order.iter().enumerate() {
        if let (Some(slot), Some(v)) = (red.get_mut(*k as usize), reds.get(rank * reds.len() / order.len())) {
            *slot = *v;
        }
    }
    let rgb = red.iter().map(|&r| if grey { [r; 3] } else { [r, g, b] }).collect();
    let out = itexture::encode(&Picture { w, h: pic.h, rgb }, f, h.flags, h.levels > 1);
    let mut v: Vec<f64> = vis.clone();
    v.par_sort_unstable_by(f64::total_cmp);
    let note = format!(
        "colour / occlusion map: {w} x {} {f:?} (the donor's), sky visibility p10 / p50 / p90 {:.3} / {:.3} / {:.3} \
         mapped onto the donor's red (p10 / p50 / p90 {} / {} / {}){}",
        pic.h,
        percentile_sorted(&v, 10.0),
        percentile_sorted(&v, 50.0),
        percentile_sorted(&v, 90.0),
        reds.get(reds.len() / 10).copied().unwrap_or(0),
        reds.get(reds.len() / 2).copied().unwrap_or(0),
        reds.get(reds.len() * 9 / 10).copied().unwrap_or(0),
        if grey {
            ", grey".to_owned()
        } else {
            format!(", green {g}, blue {b}")
        }
    );
    Ok((out, note))
}

/// The outer square's half side: the ring's (the main block's side) or the main block's half side.
fn outer_half(t: &Terrain) -> f64 {
    let side = f64::from(t.height.size);
    if t.ring.is_empty() { side / 2.0 } else { side }
}

/// The visual water (TGEN-WATER-1): one open-water quad at `y` over the outer square, its vertices DICE's open-water
/// vertices (WATER-VIS-6), its indices the water mesh's (WATER-VIS-4), drawn with the donor's open-water slot (the
/// slot whose ranges hold most open-water vertices; slot 1, DICE's usual one, when the donor has none); the donor's
/// slots renamed for terrain `name` (`Shaders/WaterOcean` and the like kept), its far plane kept.
///
/// # Errors
/// The donor's visual water does not decode.
pub fn visual_water(t: &Terrain, y: f64, donor: &[u8], from: &str, name: &str) -> Result<Vec<u8>, String> {
    let d = VisualWaterFile::decode(donor).map_err(|e| e.to_string())?;
    let verts = d.vertices.as_chunks::<36>().0;
    let mut best: (usize, u32) = (0, 1);
    for r in &d.ranges {
        let [slot, _, first, count] = *r;
        let open = d
            .indices
            .get(first as usize..(first + count) as usize)
            .unwrap_or_default()
            .iter()
            .filter(|&&i| {
                verts.get(i as usize).is_some_and(|v| {
                    let xyz = |k: usize| half(v.as_chunks::<2>().0.get(k).map_or(0, |b| u16::from_le_bytes(*b)));
                    water::open_water_vertex(xyz(0), xyz(1), xyz(2)) == *v
                })
            })
            .count();
        if open > best.0 {
            best = (open, slot);
        }
    }
    let a = outer_half(t) as f32;
    let y = y as f32;
    let mut vertices = Vec::with_capacity(4 * 36);
    for [x, z] in [[-a, -a], [a, -a], [a, a], [-a, a]] {
        vertices.extend(water::open_water_vertex(x, y, z));
    }
    let slots = d
        .slots
        .iter()
        .map(|(s, f)| {
            (
                visual_terrain::rename(s, from, name).unwrap_or_else(|| s.clone()),
                f.clone(),
            )
        })
        .collect();
    let out = VisualWaterFile {
        format: d.format,
        stride: 36,
        vertices,
        indices: vec![0, 2, 1, 0, 3, 2],
        slots,
        ranges: vec![[best.1, 2, 0, 6]],
        tail: d.tail,
    };
    Ok(out.encode())
}

/// An IEEE half's value.
fn half(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = i32::from((h >> 10) & 0x1F);
    let m = f32::from(h & 0x3FF);
    s * if e == 0 {
        m * 2f32.powi(-24)
    } else {
        (1.0 + m / 1024.0) * 2f32.powi(e - 15)
    }
}

/// The new terrain's visual terrain (TGEN-VIS-1, -3): the donor's patch shaders renamed for `name`, 2N empty road
/// blocks, the main patch map (cells of 16 m: [`cell_wants`] and [`pick`]) and one 4 x 4 map per ring block by the
/// same rule on the ring's heights (DICE's rings: rule-only cells, most of them the plain shader).
///
/// # Errors
/// The donor's visual terrain does not decode, or it has no rule-only patch shader.
pub fn visual_terrain(
    t: &Terrain,
    donor: &[u8],
    from: &str,
    name: &str,
) -> Result<(VisualTerrain, BTreeMap<u8, usize>), String> {
    let d = VisualTerrain::decode(donor).map_err(|e| e.to_string())?;
    let main = d.maps.first().ok_or("the donor's visual terrain has no patch map")?;
    let mut usage = vec![0usize; d.shaders.len()];
    for &s in &main.shader {
        if let Some(u) = usage.get_mut(usize::from(s)) {
            *u += 1;
        }
    }
    let plain = pick(&d.shaders, &usage, RuleKind::default()).ok_or("the donor has no rule-only patch shader")?;
    let atlas_words = [main.header[0], main.header[1], main.header[2], main.header[3]];
    let atlas = visual_terrain::rename(&main.atlas, from, name)
        .unwrap_or_else(|| format!("Terrains/{name}/{name}.MaskTileAtlas"));
    let n = t.height.size as usize;
    let side = n / 16;
    let scale = t.height.range_m / 65535.0;
    let h: Vec<f64> = t.height.raw.par_iter().map(|v| f64::from(*v) * scale).collect();
    let wants = cell_wants(&h, n, 1.0, side, t.height.water_y);
    let mut cache: BTreeMap<(bool, bool), u8> = BTreeMap::new();
    let mut cells = BTreeMap::new();
    let shader: Vec<u8> = wants
        .iter()
        .map(|w| {
            let s = *cache
                .entry((w.sea, w.slope))
                .or_insert_with(|| pick(&d.shaders, &usage, *w).unwrap_or(plain));
            *cells.entry(s).or_insert(0) += 1;
            s
        })
        .collect();
    let base = format!("Terrains/{name}/{name}");
    let mut maps = vec![PatchMap {
        shader,
        ..PatchMap::rule_only(
            t.height.bounds(),
            atlas_words,
            side as u32,
            plain,
            &format!("{base}.Heightfield-000"),
            &atlas,
        )
    }];
    for (k, b) in t.ring.iter().enumerate() {
        let rh: Vec<f64> = b.raw.iter().map(|v| f64::from(*v) * scale).collect();
        let sp = f64::from(b.bounds[3] - b.bounds[0]) / RING_N as f64;
        let shader = cell_wants(&rh, RING_N, sp, 4, t.height.water_y)
            .iter()
            .map(|w| {
                *cache
                    .entry((w.sea, w.slope))
                    .or_insert_with(|| pick(&d.shaders, &usage, *w).unwrap_or(plain))
            })
            .collect();
        maps.push(PatchMap {
            shader,
            ..PatchMap::rule_only(
                b.bounds,
                atlas_words,
                4,
                plain,
                &format!("{base}.Heightfield-{:03}", k + 1),
                &atlas,
            )
        });
    }
    let mut vt = VisualTerrain {
        roads: vec![VisualTerrain::EMPTY_ROADS.to_vec(); 2 * maps.len()],
        shaders: d.shaders.clone(),
        maps,
    };
    vt.rename_terrain(from, name);
    Ok((vt, cells))
}

/// The donor's entries the new terrain draws with: the patch and undergrowth mask shaders, the undergrowth cache
/// shaders the visual terrain names and the visual water's slots, where they are the donor terrain's.
#[must_use]
pub fn donor_entries(
    db: &bc2_formats::shaderdb::ShaderDb<'_>,
    vt: &VisualTerrain,
    water: Option<&VisualWaterFile>,
    from: &str,
) -> Vec<String> {
    let mut out = sterrain::mask_entries(db, from);
    let mut extra: Vec<String> = vt
        .shaders
        .iter()
        .flat_map(|s| s.groups.iter().flatten().map(|m| m.shader.clone()))
        .chain(water.iter().flat_map(|w| w.slots.iter().map(|s| s.0.clone())))
        .filter(|s| visual_terrain::rename(s, from, "X").is_some())
        .collect();
    extra.sort();
    extra.dedup();
    for e in extra {
        if !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

/// The colour / occlusion map a donor terrain's patch shaders sample (LIGHT-BAKE-2): the texture under
/// `Terrains/<from>/` whose base name holds `occ` (DICE's `mp05_AmbientOcclusion`, `MP008_Occlusion`, `sp05_snowocc`,
/// `heightmap_OCC`) that the most patch shader entries sample; `None` when they sample none (`MP_003`, `MP_004`).
#[must_use]
pub fn occlusion_name(db: &bc2_formats::shaderdb::ShaderDb<'_>, from: &str) -> Option<String> {
    let scan = bc2_formats::shaderdb::levels::scan(db).ok()?;
    let pre = format!("terrains/{0}/{0}/patchmaskshaders/", from.to_ascii_lowercase());
    let folder = format!("terrains/{}/", from.to_ascii_lowercase());
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in &scan {
        let name: String = e.name.iter().map(|&b| char::from(b)).collect();
        if !name.to_ascii_lowercase().starts_with(&pre) {
            continue;
        }
        let mut seen = std::collections::BTreeSet::new();
        for t in &e.textures {
            let tex: String = t.2.iter().map(|&b| char::from(b)).collect();
            let low = tex.to_ascii_lowercase();
            let base = low.rsplit('/').next().unwrap_or_default();
            if low.starts_with(&folder) && base.contains("occ") && seen.insert(tex.clone()) {
                *counts.entry(tex).or_insert(0) += 1;
            }
        }
    }
    counts
        .into_iter()
        .max_by_key(|(n, c)| (*c, std::cmp::Reverse(n.clone())))
        .map(|(n, _)| n)
}

/// The new terrain `name`'s visual side from `donor` (the module's table).
///
/// # Errors
/// A donor file does not decode, the donor has no rule-only patch shader, or a database clone is refused.
pub fn build(t: &Terrain, name: &str, donor: &Donor<'_>) -> Result<Visual, String> {
    let from = donor.terrain.as_str();
    let base = format!("Terrains/{name}/{name}");
    let mut out = Visual::default();
    let (vt, cells) = visual_terrain(t, donor.visual, from, name)?;
    out.notes.push(format!(
        "visual terrain: {} blocks, {} patch shaders from {from}; main cells per shader {}",
        vt.maps.len(),
        vt.shaders.len(),
        cells
            .iter()
            .map(|(s, c)| format!(
                "{} x {c}",
                vt.shaders
                    .get(usize::from(*s))
                    .map_or("?", |p| p.name.rsplit('/').next().unwrap_or("?"))
            ))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.cells = cells;
    out.terrain
        .push(res(format!("{base}.Visual.res"), "VisualTerrain", vt.encode()));
    if let Some(a) = donor.atlas {
        out.terrain
            .push(res(format!("{base}.MaskTileAtlas.res"), "ITexture", a.to_vec()));
    }
    let donor_water = donor
        .water
        .map(VisualWaterFile::decode)
        .transpose()
        .map_err(|e| e.to_string())?;
    if let (Some(y), Some(w)) = (t.height.water_y, donor.water) {
        out.terrain.push(res(
            format!("{base}.Water.res"),
            "VisualWater",
            visual_water(t, y, w, from, name)?,
        ));
    }
    let mut textures = Vec::new();
    if let Some((occ_name, occ)) = &donor.occlusion {
        let (bytes, note) = occlusion(t, occ)?;
        let new = format!("Terrains/{name}/Textures/{name}_AmbientOcclusion");
        out.level.push(res(format!("{new}.res"), "ITexture", bytes));
        out.notes.push(note);
        textures.push((occ_name.clone(), new));
    }
    let donor_vt = VisualTerrain::decode(donor.visual).map_err(|e| e.to_string())?;
    for &(dx, data) in &donor.databases {
        let layout = if dx == 11 { 10 } else { dx };
        let db = open::open(data, Some(layout), Some(false), "the donor database")?;
        let entries = donor_entries(&db.db, &donor_vt, donor_water.as_ref(), from);
        let c = sterrain::clone_terrain(data, data, dx, &entries, from, name, &textures)?;
        out.notes.push(format!(
            "Dx{dx}: {} entries cloned ({} programs), {} missing, textures {:?}",
            c.added.len(),
            c.added.iter().map(|a| a.2).sum::<usize>(),
            c.missing.len(),
            c.retextured
        ));
        let kind = match dx {
            9 => "Dx9ShaderDatabase",
            10 => "Dx10ShaderDatabase",
            _ => "Dx11ShaderDatabase",
        };
        out.level
            .push(res(format!("Levels/{name}/shaders/Dx{dx}_Single.res"), kind, c.bytes));
    }
    Ok(out)
}
