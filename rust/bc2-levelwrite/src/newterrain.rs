//! A new level's own terrain (rounds E7 and TG3, `spec/transforms/terrain_edit.md` TEDIT-NEW-*,
//! `spec/transforms/terrain_gen.md` TGEN-REQ-4 / -5): Create Level's terrain - from a 16-bit height map or from the
//! terrain generator (`bc2_terragen`) - written by the export as the level's own terrain bundle, under the level's
//! own terrain name, in place of the base level's.
//!
//! | Step | Here |
//! |---|---|
//! | the height map's samples (16-bit PNG or RAW; 1,025 / 2,049 cut to 1,024 / 2,048) | [`read_samples`] |
//! | the heights stored from 0 up (TEDIT-HF-1), the range the map's, the ring, the materials | [`from_heightmap`] |
//! | the generator's terrain (TGEN-GEN-5) | [`from_settings`] |
//! | either, from a project's record | [`terrain_of`] |
//! | the base level's visual side as the donor (TGEN-BIOME-4) | [`DonorFiles::read`] |
//! | the names: `Terrains/<LV>/<LV>` everywhere the base's terrain is named (TGEN-REQ-4) | [`rename_terrain`] |
//! | the terrain bundle's and the level bundle's new entries | [`build`] |
//!
//! A height map stores its values as they are when its lowest height is 0 m: a DICE height field written out as a
//! PNG and imported again gives DICE's stored samples, min / max tree and bounds back (the export's test on
//! `MP_005`), only the lower envelope (built under the heights, TEDIT-HF-3) and the depth map (255) are the writer's.

use std::collections::BTreeMap;
use std::path::Path;

use bc2_formats::dbx::edit::EditDbx;
use bc2_terragen::{Biome, Generated, Pad, Settings, Terrain};

use crate::export::{ExportError, Game, read_found};
use crate::project::{GenerateSpec, HeightFormat, TerrainImport};

type Result<T> = std::result::Result<T, ExportError>;

fn refuse(why: impl std::fmt::Display) -> ExportError {
    crate::rules::refusal("XVAL-TERR-4", why)
}

/// A height map's samples: (samples a side, row-major values), the last row and column of a 1,025 / 2,049 map dropped
/// (TEDIT-NEW-1). PNG samples are big-endian, a RAW's in the byte order recorded.
///
/// # Errors
/// The file does not read or decode, is not square, or is not 1,024 / 2,048 (+ 1) a side.
pub fn read_samples(path: &Path, format: HeightFormat) -> Result<(u32, Vec<u16>)> {
    let data = std::fs::read(path).map_err(|e| refuse(format!("{}: {e}", path.display())))?;
    let (side, vals) = match format {
        HeightFormat::Png16 => {
            let mut d = png::Decoder::new(std::io::Cursor::new(&data));
            d.set_transformations(png::Transformations::IDENTITY);
            let mut r = d
                .read_info()
                .map_err(|e| refuse(format!("{}: not a PNG that reads ({e})", path.display())))?;
            let (w, h, c, b) = {
                let i = r.info();
                (i.width, i.height, i.color_type, i.bit_depth)
            };
            if w != h || c != png::ColorType::Grayscale || b != png::BitDepth::Sixteen {
                return Err(refuse(format!(
                    "{}: a height map is a square 16-bit greyscale PNG",
                    path.display()
                )));
            }
            let mut buf = vec![0; r.output_buffer_size().unwrap_or(0)];
            let info = r
                .next_frame(&mut buf)
                .map_err(|e| refuse(format!("{}: {e}", path.display())))?;
            buf.truncate(info.buffer_size());
            (
                w,
                buf.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&b| u16::from_be_bytes(b))
                    .collect::<Vec<u16>>(),
            )
        }
        HeightFormat::Raw16 { side, little_endian } => {
            if data.len() as u64 != 2 * u64::from(side) * u64::from(side) {
                return Err(refuse(format!(
                    "{}: {} bytes is not a {side} x {side} RAW",
                    path.display(),
                    data.len()
                )));
            }
            let v = data
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&b| {
                    if little_endian {
                        u16::from_le_bytes(b)
                    } else {
                        u16::from_be_bytes(b)
                    }
                })
                .collect();
            (side, v)
        }
    };
    let n = match side {
        1024 | 2048 => side,
        1025 | 2049 => side - 1,
        _ => {
            return Err(refuse(format!(
                "{}: {side} a side; BFBC2's terrain is 1024 or 2048 (TEDIT-HF-1)",
                path.display()
            )));
        }
    };
    if vals.len() != (side as usize) * (side as usize) {
        return Err(refuse(format!("{}: the samples do not fill the map", path.display())));
    }
    let s = side as usize;
    let out = if n == side {
        vals
    } else {
        (0..n as usize)
            .flat_map(|j| vals.get(j * s..j * s + n as usize).unwrap_or_default().iter().copied())
            .collect()
    };
    Ok((n, out))
}

/// The stored terrain of a height map's samples (TEDIT-NEW-1): heights `lo + v / 65535 x (hi - lo)` from the
/// import's range; when `lo` is under 0 everything (the water too) is raised by `-lo`, since a main block's heights
/// start at 0 (TEDIT-HF-1). The block's range is the highest height; with `lo` = 0 each stored value is the map's
/// value itself. The water lies at `water` when ground is under it (else none).
#[must_use]
pub fn stored(vals: &[u16], range: [f64; 2], water: f64) -> (Generated, f64) {
    let lift = (-range[0]).max(0.0);
    let (lo, hi) = (range[0] + lift, range[1] + lift);
    let n = vals.len().isqrt() as u32;
    let raw: Vec<u16> = if lo == 0.0 {
        vals.to_vec()
    } else {
        vals.iter()
            .map(|&v| {
                let h = lo + f64::from(v) / 65535.0 * (hi - lo);
                (h / hi * 65535.0).round().clamp(0.0, 65535.0) as u16
            })
            .collect()
    };
    let wy = water + lift;
    let lowest = f64::from(raw.iter().copied().min().unwrap_or(0)) / 65535.0 * hi;
    let g = Generated {
        size: n,
        raw,
        range_m: hi,
        water_y: (wy > lowest).then_some(wy),
    };
    (g, lift)
}

/// A seed from a sha1's first 16 hex digits (the height map's ring seed).
fn seed_of(sha1: &str) -> u64 {
    u64::from_str_radix(sha1.get(..16).unwrap_or("0"), 16).unwrap_or(0)
}

/// A new terrain's seed (round TG4: its tiled paint's and vegetation's): the generator's, else the height map's
/// sha1's.
#[must_use]
pub fn seed_of_import(t: &TerrainImport) -> u64 {
    t.generate
        .as_ref()
        .map_or_else(|| seed_of(&t.heightmap.sha1), |g| g.seed)
}

/// A new terrain's biome (the generator's, else the import's materials', else arid).
#[must_use]
pub fn biome_of_import(t: &TerrainImport) -> Biome {
    t.generate
        .as_ref()
        .map(|g| g.biome.as_str())
        .or(t.biome.as_deref())
        .and_then(|b| b.parse().ok())
        .unwrap_or(Biome::Arid)
}

/// A base terrain file's name in the new terrain (round TG4: the strokes name the base's files): `from`'s stem
/// (`terrains/mp_005/mp_005`) replaced by `to`'s, or a renamed texture (`renames`: the colour / occlusion map, names
/// without `.res`); `None` for a name of neither.
#[must_use]
pub fn mapped_name(name: &str, from: &str, to: &str, renames: &[(String, String)]) -> Option<String> {
    let low = name.to_ascii_lowercase();
    if let Some(rest) = low.strip_prefix(from)
        && (rest.starts_with('.') || rest.starts_with('-'))
    {
        return Some(format!("{to}{rest}"));
    }
    renames
        .iter()
        .find(|(old, _)| format!("{}.res", old.to_ascii_lowercase()) == low)
        .map(|(_, new)| format!("{new}.res"))
}

/// A height map's terrain (round E7): its stored heights ([`stored`]), the ring when asked (TGEN-RING-1, its distant
/// hills seeded by the map's sha1), the 0.5 m material cells painted with the biome's names (TGEN-MAT-2).
///
/// # Errors
/// The map does not read ([`read_samples`]), its size is not the map's size in metres (TGEN-REQ-2: 1 m samples), or
/// the biome is unknown.
pub fn from_heightmap(t: &TerrainImport, folder: &Path) -> Result<(Terrain, Vec<String>)> {
    let (n, vals) = read_samples(&folder.join(&t.heightmap.file), t.format)?;
    if (t.map_size_m - f64::from(n)).abs() > 1e-6 {
        return Err(refuse(format!(
            "the map is {} m across and the height map {n} samples a side: BFBC2's terrain has 1 m samples \
             (TGEN-REQ-2), so the map size must be {n} m",
            t.map_size_m
        )));
    }
    let biome: Biome = t.biome.as_deref().unwrap_or("arid").parse().map_err(refuse)?;
    let (mut height, lift) = stored(&vals, t.height_range_m, t.water_level_m);
    let mut notes = vec![format!(
        "new terrain: the height map {} ({n} x {n}, values {} - {}), {:.1} - {:.1} m stored as 0 - {:.1} m{}",
        t.heightmap.file,
        t.sample_range[0],
        t.sample_range[1],
        t.height_range_m[0],
        t.height_range_m[1],
        height.range_m,
        if lift > 0.0 {
            format!(" (raised {lift:.1} m: a main block's heights start at 0, TEDIT-HF-1)")
        } else {
            String::new()
        }
    )];
    let ring = if t.ring {
        bc2_terragen::ring::ring(&mut height, seed_of(&t.heightmap.sha1))
    } else {
        Vec::new()
    };
    let nn = n as usize;
    let width = 2 * nn;
    let cells = bc2_terragen::paint::paint(&height.heights(), nn, 1.0, height.water_y, width);
    let mut settings = Settings::new(biome, n, seed_of(&t.heightmap.sha1));
    settings.ring = t.ring;
    notes.push(format!(
        "new terrain: water {}, ring {}, materials {} ({biome})",
        height
            .water_y
            .map_or_else(|| "none (no ground under it)".to_owned(), |w| format!("at {w:.2} m")),
        if t.ring { "12 blocks" } else { "none" },
        biome.materials().join(", ")
    ));
    Ok((
        Terrain {
            settings,
            height,
            width,
            cells,
            names: biome.materials().iter().map(|s| (*s).to_owned()).collect(),
            ring,
        },
        notes,
    ))
}

/// The generator's settings of a project's record, the water at `water_level` (TGEN-VIS-2: the donor's).
///
/// # Errors
/// An unknown biome.
pub fn settings_of(g: &GenerateSpec, water_level: Option<f64>) -> Result<Settings> {
    let biome: Biome = g.biome.parse().map_err(refuse)?;
    let mut s = Settings::new(biome, g.size, g.seed);
    s.border_m = g.border_m;
    s.water_share = g.water_share;
    s.pads = g
        .pads
        .iter()
        .map(|p| Pad {
            x: p[0],
            z: p[1],
            radius: p[2],
        })
        .collect();
    s.rough = g.rough;
    s.erosion = g.erosion;
    s.water_level_m = water_level;
    s.ring = g.ring;
    Ok(s)
}

/// The generator's terrain (round TG3, TGEN-GEN-5).
///
/// # Errors
/// The generator refuses its settings.
pub fn from_settings(s: &Settings) -> Result<(Terrain, Vec<String>)> {
    let t = Terrain::new(s).map_err(refuse)?;
    let sh = t.shape();
    let note = format!(
        "new terrain: generated ({} {} m, seed {}, generator version {}): range {} m, water {}; slope p10/50/90/99 \
         {:.1} / {:.1} / {:.1} / {:.1} deg, HI {:.3}, under water {:.3}",
        s.biome,
        s.size,
        s.seed,
        bc2_terragen::GENERATOR_VERSION,
        t.height.range_m,
        t.height
            .water_y
            .map_or_else(|| "none".to_owned(), |w| format!("{w:.2} m")),
        sh.slope[0],
        sh.slope[1],
        sh.slope[2],
        sh.slope[3],
        sh.hi,
        sh.wet
    );
    Ok((t, vec![note]))
}

/// A project's new terrain: the generator's or the height map's (kept beside `project_file`).
///
/// # Errors
/// As [`from_heightmap`] / [`from_settings`].
pub fn terrain_of(t: &TerrainImport, project_file: &Path, donor_water: Option<f64>) -> Result<(Terrain, Vec<String>)> {
    match &t.generate {
        // the water at the base's level unless the project gives one (TGEN-VIS-2)
        Some(g) => from_settings(&settings_of(g, g.water_level_m.or(donor_water))?),
        None => from_heightmap(t, project_file.parent().unwrap_or_else(|| Path::new("."))),
    }
}

/// The new terrain's name: the level's (TGEN-REQ-4, DICE names each terrain after its map).
#[must_use]
pub fn terrain_name(level: &str) -> String {
    level.to_ascii_uppercase()
}

/// `s` with the terrain `from`'s name (`terrains/<from>/<from>` followed by its end, `/`, `.` or `-`, any case)
/// renamed `to`, the case following the string's; `None` when it does not name it.
#[must_use]
pub fn renamed(s: &[u8], from: &str, to: &str) -> Option<Vec<u8>> {
    let pre = format!("terrains/{0}/{0}", from.to_ascii_lowercase());
    let low = s.to_ascii_lowercase();
    if !low.starts_with(pre.as_bytes()) || !matches!(low.get(pre.len()), None | Some(b'/' | b'.' | b'-')) {
        return None;
    }
    let orig = s.get(9..9 + from.len()).unwrap_or_default();
    let name = if orig.iter().any(u8::is_ascii_uppercase) {
        to.to_ascii_uppercase()
    } else {
        to.to_ascii_lowercase()
    };
    let mut out = s.get(..9).unwrap_or_default().to_vec();
    out.extend(name.as_bytes());
    out.push(b'/');
    out.extend(name.as_bytes());
    out.extend(s.get(pre.len()..).unwrap_or_default());
    Some(out)
}

/// Renames every reference, id and text of a partition naming the terrain `from` to `to` (TGEN-REQ-4: the level's
/// `TerrainEntityData` reference, the terrain partition's own names); how many.
///
/// # Errors
/// The writer refuses.
pub fn rename_terrain(d: &mut EditDbx, from: &str, to: &str) -> crate::write::Result<usize> {
    let mut n = 0;
    let roots = d.roots().to_vec();
    for r in roots {
        for e in d.walk(r) {
            let attrs: Vec<(Vec<u8>, Vec<u8>)> = d.attributes(e).map(|(k, v)| (k.to_vec(), v.to_vec())).collect();
            for (k, v) in attrs {
                if (k == b"ref" || k == b"id")
                    && let Some(x) = renamed(&v, from, to)
                {
                    d.set_attr(e, &k, &x)?;
                    n += 1;
                }
            }
            if d.kind(e) == bc2_formats::dbx::TEXT
                && let Some(x) = d.text(e).and_then(|t| renamed(t, from, to))
            {
                d.set_text(e, &x)?;
                n += 1;
            }
        }
    }
    Ok(n)
}

/// The base level's terrain as the donor of the new one's visual side (TGEN-BIOME-4: the base is a map of the
/// mode, its patch shaders drawn with their own atlas): read from the bundles the base's terrain and level load.
#[derive(Debug, Clone)]
pub struct DonorFiles {
    /// The base terrain's name as its patch shaders spell it (`MP_005`).
    pub terrain: String,
    /// Its lower-case entry stem (`terrains/mp_005/mp_005`).
    pub stem: String,
    /// The visual terrain.
    pub visual: Vec<u8>,
    /// The visual water.
    pub water: Option<Vec<u8>>,
    /// The mask atlas.
    pub atlas: Option<Vec<u8>>,
    /// The colour / occlusion map (its name, the texture).
    pub occlusion: Option<(String, Vec<u8>)>,
    /// The base's water level (its water mesh's median vertex height).
    pub water_level: Option<f64>,
    /// The terrain's partitions (entry name, type, bytes): `Terrains/<T>/<T>.dbx` and those named after it
    /// (`<T>-MaterialMap.dbx`), from the bundles the base's terrain and level load.
    pub partitions: Vec<(String, String, Vec<u8>)>,
    /// Round TG4: the base terrain's main height field (its vegetation's poses on its own ground, where its tiled
    /// layers lie).
    pub heights: Option<Vec<u8>>,
    /// Round TG4: the base terrain's material map (its tiled layers' paired materials, TEDIT-MM-2).
    pub materials: Option<Vec<u8>>,
}

impl DonorFiles {
    /// The base level's terrain files; `dx9` its Dx9 database (where the colour / occlusion map's name is found).
    ///
    /// # Errors
    /// The base names no terrain, or its visual terrain is in none of its bundles.
    pub fn read(game: &Game, base: &str, dx9: Option<&[u8]>) -> Result<Self> {
        let part = crate::export::level_partition(game, base)?;
        let hf = crate::export::heightfield_name(&crate::write::LevelWriter::parse(&part)?).ok_or_else(|| {
            refuse(format!(
                "{base} names no terrain: a new terrain needs a base map with one"
            ))
        })?;
        let stem = hf.strip_suffix(".heightfield-000.res").unwrap_or(&hf).to_owned();
        let mut bundles = game.loaded(base, "terrain");
        bundles.extend(game.loaded(base, "level"));
        let live = game.live(&bundles)?;
        let get = |n: &str| -> Result<Option<Vec<u8>>> {
            match live.get(n) {
                Some(f) => Ok(read_found(&[f])?.pop()),
                None => Ok(None),
            }
        };
        let visual = get(&format!("{stem}.visual.res"))?.ok_or_else(|| {
            refuse(format!(
                "{base}'s terrain {stem} has no visual terrain in its bundles (the donor of the new terrain's look)"
            ))
        })?;
        let vt = bc2_formats::visual_terrain::VisualTerrain::decode(&visual).map_err(|e| refuse(e.to_string()))?;
        let low = stem.rsplit('/').next().unwrap_or_default().to_owned();
        let terrain = vt
            .shaders
            .first()
            .and_then(|s| s.name.split('/').nth(1))
            .map_or_else(|| low.to_ascii_uppercase(), str::to_owned);
        let water_level = get(&format!("{stem}.water.mesh.res"))?
            .and_then(|m| bc2_formats::water::parse_mesh(&m).ok())
            .and_then(|m| {
                let mut ys: Vec<f32> = m.vertices.iter().map(|v| v[1]).collect();
                ys.sort_by(f32::total_cmp);
                ys.get(ys.len() / 2).map(|y| f64::from(*y))
            });
        let occ_name = dx9.and_then(|d| {
            let o = bc2_formats::shaderdb::open::open(d, Some(9), Some(false), "dx9").ok()?;
            bc2_terragen::visual::occlusion_name(&o.db, &terrain)
        });
        let mut occlusion = None;
        if let Some(n) = occ_name {
            let mut b = bundles.clone();
            b.extend(game.startup());
            let all = game.live(&b)?;
            if let Some(f) = all.get(&format!("{}.res", n.to_ascii_lowercase())) {
                occlusion = read_found(&[f])?.pop().map(|d| (n, d));
            }
        }
        let mut names: Vec<&String> = live
            .keys()
            .filter(|n| n.to_ascii_lowercase().ends_with(".dbx") && is_base_terrain_entry(n, &stem))
            .collect();
        names.sort();
        let mut partitions = Vec::new();
        for n in names {
            if let Some(f) = live.get(n)
                && let Some(d) = read_found(&[f])?.pop()
            {
                partitions.push((f.1.name.clone(), f.1.type_name.clone(), d));
            }
        }
        Ok(Self {
            terrain,
            heights: get(&hf)?,
            materials: get(&format!("{stem}-materialmap.res"))?,
            water: get(&format!("{stem}.water.res"))?,
            atlas: get(&format!("{stem}.masktileatlas.res"))?,
            stem,
            visual,
            occlusion,
            water_level,
            partitions,
        })
    }

    /// The generator's view of it (the databases are the export's own, cloned one by one).
    #[must_use]
    pub fn donor(&self) -> bc2_terragen::visual::Donor<'_> {
        bc2_terragen::visual::Donor {
            terrain: self.terrain.clone(),
            visual: &self.visual,
            water: self.water.as_deref(),
            atlas: self.atlas.as_deref(),
            occlusion: self.occlusion.as_ref().map(|(n, d)| (n.clone(), d.as_slice())),
            databases: Vec::new(),
        }
    }

    /// The base terrain's lower-case name (`mp_005`).
    #[must_use]
    pub fn low(&self) -> &str {
        self.stem.rsplit('/').next().unwrap_or_default()
    }
}

/// An entry of the new level's bundles.
#[derive(Debug, Clone)]
pub struct NewEntry {
    /// The entry's name.
    pub name: String,
    /// Its type.
    pub type_name: String,
    /// Its data.
    pub data: Vec<u8>,
}

/// What the export stages for a new terrain.
#[derive(Debug, Clone, Default)]
pub struct Built {
    /// The terrain bundle's new entries (TGEN-REQ-1): height fields, material map, water mesh, `Terrain`, visual
    /// terrain, mask atlas, visual water.
    pub terrain: Vec<NewEntry>,
    /// The level bundle's: the colour / occlusion map and the terrain partition `Terrains/<LV>/<LV>.dbx`.
    pub level: Vec<NewEntry>,
    /// The colour / occlusion map's renaming the cloned database entries apply.
    pub textures: Vec<(String, String)>,
    /// The report's lines.
    pub notes: Vec<String>,
    /// The main patch map's cells per patch shader.
    pub cells: BTreeMap<u8, usize>,
}

/// The new terrain `name`'s entries: the terrain's resources (`Terrain::resources`), the visual side from the donor
/// (`visual::build`, TGEN-VIS-1 / -3, TGEN-WATER-1, TGEN-OCC-1) and the donor's terrain partition renamed
/// (TGEN-REQ-4) with a partition guid of its own.
///
/// # Errors
/// A writer refuses, or the donor's files do not decode.
pub fn build(t: &Terrain, name: &str, donor: &DonorFiles) -> Result<Built> {
    let mut out = Built::default();
    for r in t.resources(name).map_err(|e| refuse(e.to_string()))? {
        out.terrain.push(NewEntry {
            name: r.name,
            type_name: r.type_name.into(),
            data: r.data,
        });
    }
    let d = donor.donor();
    let v = bc2_terragen::visual::build(t, name, &d).map_err(refuse)?;
    for r in v.terrain {
        out.terrain.push(NewEntry {
            name: r.name,
            type_name: r.type_name.into(),
            data: r.data,
        });
    }
    for r in v.level {
        out.level.push(NewEntry {
            name: r.name,
            type_name: r.type_name.into(),
            data: r.data,
        });
    }
    out.textures = bc2_terragen::visual::occlusion_rename(&d, name);
    out.notes.extend(v.notes);
    out.cells = v.cells;
    for (pname, ty, bytes) in &donor.partitions {
        let mut p = EditDbx::parse(bytes).map_err(|e| refuse(format!("{pname}: {e}")))?;
        let n = rename_terrain(&mut p, donor.low(), name)?;
        let tail = pname.get(donor.stem.len()..).unwrap_or(".dbx");
        let new_name = format!("Terrains/{name}/{name}{tail}");
        crate::mpconvert::set_partition_guid(&mut p, &crate::mpconvert::partition_guid_for(&new_name))?;
        out.notes.push(format!(
            "new terrain: the terrain partition {pname} copied as {new_name} ({n} names renamed)"
        ));
        out.level.push(NewEntry {
            name: new_name,
            type_name: ty.clone(),
            data: p.write().map_err(|e| refuse(e.0))?,
        });
    }
    Ok(out)
}

/// Whether a terrain bundle entry is the base terrain's own resource (`terrains/<b>/<b>.*` or `-materialmap`), which
/// the new level's own terrain bundle leaves out (TGEN-REQ-5).
#[must_use]
pub fn is_base_terrain_entry(name: &str, stem: &str) -> bool {
    let low = name.to_ascii_lowercase();
    low.strip_prefix(stem)
        .is_some_and(|rest| rest.starts_with('.') || rest.starts_with('-'))
}

/// A staged bundle's entries by lower-case name: (type, data).
pub type Entries = std::collections::HashMap<String, (String, Vec<u8>)>;

/// XVAL-TERR-4's checks of a stage with a new terrain (round E7 / TG3): the problems found and how many checks ran.
/// The terrain bundle holds every new resource and none of the base terrain's; the main block decodes, encodes back
/// byte for byte, has its heights' min / max tree and its lower envelope under every sample; the ring's blocks read
/// and are stitched to it (TERR-HF-12); the material map round-trips; the visual terrain's patch maps name the new
/// height field and their shaders are the new terrain's, each in every staged shader database; the water mesh reads;
/// the level bundle holds the renamed terrain partition; the terrain block of `package.mft` names no bundle of the
/// base level (TGEN-REQ-5).
#[must_use]
#[allow(clippy::too_many_lines, reason = "one check after another, in the resources' order")]
pub fn check_stage(
    rec: &crate::export::NewTerrainRecord,
    base: &str,
    terrain: Option<&Entries>,
    level: Option<&Entries>,
    terrain_block: &[String],
) -> (Vec<String>, usize) {
    let mut bad = Vec::new();
    let mut n = 0usize;
    let name = &rec.name;
    let stem = format!("terrains/{0}/{0}", name.to_ascii_lowercase());
    let Some(t) = terrain else {
        return (vec!["no staged terrain bundle".into()], 1);
    };
    let get = |s: &str| t.get(&format!("{stem}{s}")).map(|x| x.1.as_slice());
    n += 1;
    let mut stale: Vec<&String> = t.keys().filter(|k| is_base_terrain_entry(k, &rec.replaces)).collect();
    stale.sort();
    if !stale.is_empty() {
        bad.push(format!(
            "the base terrain's entries are staged: {:?}",
            stale.iter().take(3).collect::<Vec<_>>()
        ));
    }
    for e in &rec.terrain {
        n += 1;
        if !t.contains_key(&e.to_ascii_lowercase()) {
            bad.push(format!("{e}: not in the staged terrain bundle"));
        }
    }
    n += 1;
    let main =
        get(".heightfield-000.res").and_then(|d| bc2_formats::terrain_codec::MainBlock::decode(d).ok().map(|m| (d, m)));
    match &main {
        Some((d, m)) => {
            if m.encode() != *d {
                bad.push("the main block does not encode back byte for byte".into());
            }
            if m.tree != bc2_formats::terrain_codec::minmax_tree(&m.heights, m.size as usize) {
                bad.push("the main block's min / max tree is not its heights'".into());
            }
            if m.under_share() < 1.0 {
                bad.push(format!(
                    "the lower envelope lies over {:.4} of the samples",
                    1.0 - m.under_share()
                ));
            }
            if m.min[1] != 0.0 || !matches!(m.size, 1024 | 2048) {
                bad.push(format!(
                    "the main block is {} samples from {} m (TEDIT-HF-1: 1024 / 2048 from 0)",
                    m.size, m.min[1]
                ));
            }
        }
        None => bad.push(format!("{stem}.heightfield-000.res does not read as a main block")),
    }
    let ring: Vec<Vec<u16>> = (1..=12)
        .filter_map(|k| get(&format!(".heightfield-{k:03}.res")))
        .filter_map(|d| bc2_formats::terrain_codec::SubBlock::decode(d).ok().map(|b| b.heights))
        .collect();
    if let Some((_, m)) = &main
        && !ring.is_empty()
    {
        n += 1;
        let s = bc2_terragen::ring::stitching(&m.heights, m.size as usize, &ring);
        if ring.len() != 12 || s.main_seams.0 != s.main_seams.1 || s.ring_seams.0 != s.ring_seams.1 || s.straight > 0.5
        {
            bad.push(format!(
                "the ring ({} blocks) is not stitched (seams {} / {}, ring seams {} / {})",
                ring.len(),
                s.main_seams.0,
                s.main_seams.1,
                s.ring_seams.0,
                s.ring_seams.1
            ));
        }
    }
    n += 1;
    match get("-materialmap.res").map(bc2_formats::terrain_codec::MaterialMap::decode) {
        Some(Ok(m)) if Some(m.encode()) == get("-materialmap.res").map(<[u8]>::to_vec) => {
            if let Some((_, mb)) = &main
                && m.width != 2 * mb.size
            {
                bad.push(format!(
                    "the material map is {} cells for {} samples (0.5 m cells)",
                    m.width, mb.size
                ));
            }
        }
        _ => bad.push("the material map does not decode and encode back".into()),
    }
    if let Some(w) = get(".water.mesh.res") {
        n += 1;
        if bc2_formats::water::parse_mesh(w).is_err() {
            bad.push("the water mesh does not read".into());
        }
    }
    n += 1;
    let mut shaders = Vec::new();
    match get(".visual.res").map(bc2_formats::visual_terrain::VisualTerrain::decode) {
        Some(Ok(vt)) => {
            let hf = format!("{stem}.heightfield-");
            for (k, m) in vt.maps.iter().enumerate() {
                if !m.heightfield.to_ascii_lowercase().starts_with(&hf) {
                    bad.push(format!("patch map {k} names {}", m.heightfield));
                }
                if m.shader.iter().any(|&s| usize::from(s) >= vt.shaders.len()) {
                    bad.push(format!("patch map {k} names a patch shader the terrain lacks"));
                }
            }
            for s in &vt.shaders {
                if !s.name.to_ascii_lowercase().starts_with(&format!("{stem}/")) {
                    bad.push(format!("the patch shader {} is not the new terrain's", s.name));
                }
                shaders.push(s.name.to_ascii_lowercase());
            }
        }
        _ => bad.push("the visual terrain does not decode".into()),
    }
    if let Some(l) = level {
        n += 1;
        if !l.contains_key(&format!("{stem}.dbx")) {
            bad.push(format!("the level bundle has no terrain partition {stem}.dbx"));
        }
        let mut dbs: Vec<(&String, &(String, Vec<u8>))> = l.iter().collect();
        dbs.sort_by(|a, b| a.0.cmp(b.0));
        for (ename, (ty, data)) in dbs {
            let Some(file) = crate::shaders::database_file(ename, ty) else {
                continue;
            };
            n += 1;
            let dx = bc2_formats::shaderdb::open::dx_in_name(&file).unwrap_or(10);
            let Ok(o) =
                bc2_formats::shaderdb::open::open(data, Some(if dx == 11 { 10 } else { dx }), Some(false), ename)
            else {
                bad.push(format!("{ename}: does not read"));
                continue;
            };
            let have: std::collections::BTreeSet<String> =
                o.db.entries
                    .iter()
                    .map(|e| bc2_formats::shaderdb::merge::latin(&e.name).to_ascii_lowercase())
                    .collect();
            let missing: Vec<&String> = shaders.iter().filter(|s| !have.contains(*s)).collect();
            if !missing.is_empty() {
                bad.push(format!(
                    "{ename}: {} patch shader(s) of the new terrain missing (first {})",
                    missing.len(),
                    missing.first().map_or("", |m| m.as_str())
                ));
            }
        }
    } else {
        bad.push("no staged level bundle".into());
    }
    n += 1;
    let base_dir = format!("levels/{}/", base.to_ascii_lowercase());
    if terrain_block
        .iter()
        .filter_map(|l| l.strip_prefix("BundleTarget "))
        .any(|t| t.to_ascii_lowercase().contains(&base_dir))
    {
        bad.push(format!(
            "the terrain block of package.mft names a bundle of {base} (TGEN-REQ-5)"
        ));
    }
    (bad, n)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn names_renamed() {
        let r = |s: &str| renamed(s.as_bytes(), "mp_005", "atoll").map(|v| String::from_utf8(v).unwrap());
        assert_eq!(
            r("terrains/mp_005/mp_005/5C1A0D0B-0000-0000-0000-000000000000").as_deref(),
            Some("terrains/atoll/atoll/5C1A0D0B-0000-0000-0000-000000000000")
        );
        assert_eq!(
            r("Terrains/MP_005/MP_005.Visual").as_deref(),
            Some("Terrains/ATOLL/ATOLL.Visual")
        );
        assert_eq!(r("Terrains/MP_005/MP_005"), Some("Terrains/ATOLL/ATOLL".into()));
        // another terrain, a texture folder of the terrain, a longer name: not renamed
        assert_eq!(r("Terrains/MP_005/Textures/mp05_AmbientOcclusion"), None);
        assert_eq!(r("terrains/mp_005/mp_005b"), None);
        assert_eq!(r("levels/mp_005/mp_005"), None);
        assert!(is_base_terrain_entry(
            "Terrains/MP_005/MP_005-MaterialMap.res",
            "terrains/mp_005/mp_005"
        ));
        assert!(is_base_terrain_entry(
            "terrains/mp_005/mp_005.heightfield-003.res",
            "terrains/mp_005/mp_005"
        ));
        assert!(!is_base_terrain_entry(
            "terrains/mp_005/shaders/x.res",
            "terrains/mp_005/mp_005"
        ));
    }

    #[test]
    fn heights_stored_from_zero() {
        let vals: Vec<u16> = (0..16u16).map(|v| v * 4000).collect();
        // from 0: each value is stored as it is
        let (g, lift) = stored(&vals, [0.0, 300.0], 10.0);
        assert_eq!((g.raw.clone(), lift, g.size, g.range_m), (vals.clone(), 0.0, 4, 300.0));
        assert_eq!(g.water_y, Some(10.0));
        // from -20 m: raised 20 m, the water too; the top at 320 m
        let (g, lift) = stored(&vals, [-20.0, 300.0], 0.0);
        assert_eq!((lift, g.range_m), (20.0, 320.0));
        // the whole span is raised, so the values still run 0 - 320 m as they are
        assert_eq!(g.raw, vals);
        // from 5 m: 0 is stored as 5 m
        let (g5, _) = stored(&vals, [5.0, 300.0], 0.0);
        assert_eq!(g5.raw[0], (5.0f64 / 300.0 * 65535.0).round() as u16);
        assert_eq!(g.water_y, Some(20.0));
        // water under every sample: none
        let (g, _) = stored(&vals, [5.0, 300.0], 1.0);
        assert_eq!(g.water_y, None);
    }

    #[test]
    fn a_height_map_read_and_written() {
        let d = std::env::temp_dir().join(format!("bc2_newterrain_unit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        // a 1025 RAW, big-endian: its last row and column dropped
        let n = 1025usize;
        let vals: Vec<u16> = (0..n * n).map(|k| ((k % n) * 40 + (k / n) * 20) as u16).collect();
        let raw: Vec<u8> = vals.iter().flat_map(|v| v.to_be_bytes()).collect();
        std::fs::write(d.join("a.raw"), &raw).unwrap();
        let fmt = HeightFormat::Raw16 {
            side: 1025,
            little_endian: false,
        };
        let (side, s) = read_samples(&d.join("a.raw"), fmt).unwrap();
        assert_eq!((side, s.len(), s[1023], s[1024]), (1024, 1024 * 1024, 1023 * 40, 20));
        let mut t = TerrainImport {
            content_set: "BFBC2".into(),
            heightmap: crate::project::KeptFile {
                file: "a.raw".into(),
                source: String::new(),
                size: raw.len() as u64,
                sha1: "00ff".into(),
            },
            format: fmt,
            samples: 1025,
            sample_range: [0, 61_380],
            map_size_m: 2048.0,
            height_range_m: [0.0, 200.0],
            water_level_m: 5.0,
            colour_map: None,
            generate: None,
            biome: Some("snow".into()),
            ring: true,
            vegetation: None,
            strokes_on_base: false,
        };
        // the map's size must be its samples' (1 m)
        assert!(from_heightmap(&t, &d).unwrap_err().0.contains("1024 m"));
        t.map_size_m = 1024.0;
        let (terr, notes) = from_heightmap(&t, &d).unwrap();
        assert!(notes[1].contains("ring 12 blocks"), "{notes:?}");
        // the ring straightens the main block's edges between its knots (TERR-HF-12); the inside is the map's
        let inside = |v: &[u16]| -> Vec<u16> {
            (1..1023)
                .flat_map(|j| v[j * 1024 + 1..j * 1024 + 1023].to_vec())
                .collect()
        };
        assert_eq!(inside(&terr.height.raw), inside(&s));
        assert_eq!((terr.ring.len(), terr.names[1].as_str()), (12, "snow"));
        let res = terr.resources("ATOLL").unwrap();
        assert_eq!(res[0].name, "Terrains/ATOLL/ATOLL.Heightfield-000.res");
        assert!(res.iter().any(|r| r.name.ends_with("-012.res")));
        let _ = std::fs::remove_dir_all(&d);
    }
}

// ------------------------------------------------------------------------------------------------ tiled layers

/// One tiled layer as the donor lays it on its own ground (round TG4, TGEN-MAT-3): its mean weight over the dry
/// ground, and its weight's share per slope and height-over-water band over the ground's share (the vegetation's
/// bands, `bc2_terragen::vegetation::SLOPE_BANDS` / `HEIGHT_BANDS`; neutral 1 where the ground has under 1 %).
#[derive(Debug, Clone, PartialEq)]
pub struct LayerLay {
    /// The layer.
    pub layer: String,
    /// Its mean weight (0 - 1) over the dry ground.
    pub coverage: f64,
    /// Its preference per slope band.
    pub slope_pref: [f64; 6],
    /// Its preference per height-over-water band.
    pub height_pref: [f64; 6],
    /// The material most of its strong mask lies on (TEDIT-MM-2), when that is over half of it.
    pub material: Option<String>,
}

/// Each tiled layer of a terrain as it lies on its ground (its weights from [`crate::terrainpaint::layer_weights`],
/// the ground sampled every 4 m).
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "sample and cell counts, far below 2^52")]
pub fn layer_lays(
    weights: &BTreeMap<String, crate::delta::Raster>,
    g: &bc2_terragen::vegetation::Ground,
    pairs: &BTreeMap<String, crate::terrainpaint::Pair>,
) -> Vec<LayerLay> {
    use bc2_terragen::vegetation::{HEIGHT_BANDS, SLOPE_BANDS};
    let band = |b: &[f64], v: f64| b.iter().position(|&x| v < x).unwrap_or(b.len() - 1);
    let mut out = Vec::new();
    for (layer, r) in weights {
        let (mut gs, mut gh, mut ws, mut wh) = ([0f64; 6], [0f64; 6], [0f64; 6], [0f64; 6]);
        let (mut sum, mut n) = (0.0, 0usize);
        for j in (2..g.n).step_by(4) {
            for i in (2..g.n).step_by(4) {
                let (x, z) = (g.x0 + i as f64 * g.cell, g.z0 + j as f64 * g.cell);
                if g.water.is_some_and(|w| g.height(x, z) <= w) {
                    continue;
                }
                let w = f64::from(r.get(i as u32, j as u32)) / 255.0;
                let (bs, bh) = (
                    band(&SLOPE_BANDS, g.slope(x, z)),
                    band(&HEIGHT_BANDS, g.over_water(x, z)),
                );
                if let (Some(a), Some(b), Some(c), Some(d)) =
                    (gs.get_mut(bs), gh.get_mut(bh), ws.get_mut(bs), wh.get_mut(bh))
                {
                    *a += 1.0;
                    *b += 1.0;
                    *c += w;
                    *d += w;
                }
                sum += w;
                n += 1;
            }
        }
        let pref = |wv: [f64; 6], gv: [f64; 6]| -> [f64; 6] {
            let (tw, tg) = (wv.iter().sum::<f64>(), gv.iter().sum::<f64>().max(1.0));
            std::array::from_fn(|k| {
                let share = gv.get(k).copied().unwrap_or(0.0) / tg;
                if share >= 0.01 && tw > 0.0 {
                    wv.get(k).copied().unwrap_or(0.0) / tw / share
                } else {
                    1.0
                }
            })
        };
        out.push(LayerLay {
            layer: layer.clone(),
            coverage: sum / n.max(1) as f64,
            slope_pref: pref(ws, gs),
            height_pref: pref(wh, gh),
            material: pairs.get(layer).filter(|p| p.share > 0.5).map(|p| p.material.clone()),
        });
    }
    out
}

/// A seeded value noise of four octaves (128, 64, 32 and 16 m, amplitudes 1, 1/2, 1/4, 1/8), about unit spread.
fn noise(seed: u64, stream: u64, x: f64, z: f64) -> f64 {
    let s = bc2_terragen::rng::Stream::new(seed, stream);
    let mut v = 0.0;
    for (o, (scale, amp)) in [(128.0, 1.0), (64.0, 0.5), (32.0, 0.25), (16.0, 0.125)]
        .into_iter()
        .enumerate()
    {
        let (u, w) = (x / scale, z / scale);
        let (i, j) = (u.floor(), w.floor());
        let (fx, fz) = (u - i, w - j);
        let (fx, fz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
        let at = |a: f64, b: f64| {
            let key = ((a as i64 as u64) << 32) ^ (b as i64 as u64 & 0xFFFF_FFFF) ^ ((o as u64) << 60);
            2.0 * s.unit(key) - 1.0
        };
        let top = at(i, j) * (1.0 - fx) + at(i + 1.0, j) * fx;
        let bottom = at(i, j + 1.0) * (1.0 - fx) + at(i + 1.0, j + 1.0) * fx;
        v += amp * (top * (1.0 - fz) + bottom * fz);
    }
    v * 1.6
}

/// A tiled layer's weights on new ground (TGEN-MAT-3): a seeded noise plus the log of the layer's slope and height
/// preferences, its edge 0.5 wide, its threshold found by bisection so its mean weight over the dry ground is the
/// donor's coverage times `scale`; 0 under the water.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "sample and cell counts, far below 2^52")]
pub fn lay_layer(
    lay: &LayerLay,
    g: &bc2_terragen::vegetation::Ground,
    seed: u64,
    stream: u64,
    scale: f64,
) -> crate::delta::Raster {
    use bc2_terragen::vegetation::{HEIGHT_BANDS, SLOPE_BANDS};
    let band = |b: &[f64], v: f64| b.iter().position(|&x| v < x).unwrap_or(b.len() - 1);
    let n = g.n;
    let mut score = vec![f64::NEG_INFINITY; n * n];
    for j in 0..n {
        for i in 0..n {
            let (x, z) = (g.x0 + i as f64 * g.cell, g.z0 + j as f64 * g.cell);
            let h = f64::from(g.heights.get(j * n + i).copied().unwrap_or(0.0));
            if g.water.is_some_and(|w| h <= w) {
                continue;
            }
            let ps = lay
                .slope_pref
                .get(band(&SLOPE_BANDS, g.slope(x, z)))
                .copied()
                .unwrap_or(1.0);
            let ph = lay
                .height_pref
                .get(band(&HEIGHT_BANDS, g.over_water(x, z)))
                .copied()
                .unwrap_or(1.0);
            if let Some(s) = score.get_mut(j * n + i) {
                *s = noise(seed, stream, x, z) + ps.max(1e-3).ln() + ph.max(1e-3).ln();
            }
        }
    }
    let want = (lay.coverage * scale).clamp(0.0, 1.0);
    let mean_at = |thr: f64| {
        let (mut sum, mut k) = (0.0, 0usize);
        for s in score.iter().step_by(7).filter(|s| s.is_finite()) {
            sum += ((s - thr) / 0.5).clamp(0.0, 1.0);
            k += 1;
        }
        sum / k.max(1) as f64
    };
    let (mut lo, mut hi) = (-20.0, 20.0);
    for _ in 0..40 {
        let mid = f64::midpoint(lo, hi);
        if mean_at(mid) > want {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let thr = f64::midpoint(lo, hi);
    let mut r = crate::delta::Raster::new(n as u32, n as u32, 0);
    if want <= 0.0 {
        return r;
    }
    for j in 0..n {
        for i in 0..n {
            let s = score.get(j * n + i).copied().unwrap_or(f64::NEG_INFINITY);
            let w = ((s - thr) / 0.5).clamp(0.0, 1.0);
            if w > 0.0 {
                r.set(i as u32, j as u32, (w * 255.0).round() as u16);
            }
        }
    }
    r
}

/// What [`paint_tiled`] did.
#[derive(Debug, Clone, Default)]
pub struct Tiled {
    /// Patch shaders the paint added, to be cloned into every database.
    pub clones: Vec<crate::terrainpaint::NewShader>,
    /// The layers painted: (layer, the donor's coverage, the new terrain's, its paired material).
    pub layers: Vec<(String, f64, f64, Option<String>)>,
    /// Lines for the report.
    pub notes: Vec<String>,
}

/// The donor's tiled layers painted on a new terrain (round TG4, TGEN-MAT-3), through E5-rest's paint
/// ([`crate::terrainpaint::paint`]: each reached cell's shader the donor's own combination or a clone, its tiles in
/// free atlas slots): each layer laid as the donor lays it on its own ground ([`layer_lays`], [`lay_layer`]), its
/// paired material painted under its strong mask (weight 200 or more) in the new material map, its name added when
/// the map lacks it (TEDIT-MM-2, MAT-GRID-2: the donor's names are its level grid's). A layer the paint refuses (no
/// shader to clone: TEDIT-SH-2) is left out; when the atlas has no room, every layer's coverage is cut by a third
/// until it fits. A donor whose layers are not one kind each (TEDIT-VT-2) paints nothing: its rule layers draw alone
/// (TGEN-VIS-1).
#[allow(
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    reason = "sample counts far below 2^52; the paint's steps in order: measure, lay, paint, fit the atlas, pair the materials"
)]
pub fn paint_tiled(
    built: &mut Built,
    t: &Terrain,
    name: &str,
    donor: &DonorFiles,
    textures: Option<&crate::terrainpaint::EntryTextures>,
    seed: u64,
) -> Tiled {
    let mut out = Tiled::default();
    let (Some(atlas), Some(hb)) = (donor.atlas.as_deref(), donor.heights.as_deref()) else {
        out.notes
            .push("tiled layers: the donor has no mask atlas or height field; rule layers only (TGEN-VIS-1)".into());
        return out;
    };
    let Ok(dh) = bc2_formats::terrain::parse(hb) else {
        out.notes
            .push("tiled layers: the donor's height field does not read".into());
        return out;
    };
    let weights = match crate::terrainpaint::layer_weights(&donor.visual, atlas, dh.size) {
        Ok(w) if !w.is_empty() => w,
        Ok(_) => {
            out.notes
                .push("tiled layers: the donor draws none; rule layers only (TGEN-VIS-1)".into());
            return out;
        }
        Err(e) => {
            out.notes.push(format!(
                "tiled layers: not painted, the donor's are not one kind each (TEDIT-VT-2: {e}); rule layers only"
            ));
            return out;
        }
    };
    let pairs = donor
        .materials
        .as_deref()
        .and_then(|m| bc2_formats::terrain_codec::MaterialMap::decode(m).ok())
        .and_then(|m| crate::terrainpaint::pairing(&donor.visual, atlas, dh.size, &m).ok())
        .unwrap_or_default();
    let dg = crate::vegetation::ground_of(&dh, donor.water_level);
    let lays = layer_lays(&weights, &dg, &pairs);
    let g = bc2_terragen::vegetation::Ground::of_generated(&t.height);
    let n = t.height.size;
    let low = |e: &NewEntry, end: &str| e.name.to_ascii_lowercase().ends_with(end);
    let (Some(vi), Some(ai)) = (
        built.terrain.iter().position(|e| low(e, ".visual.res")),
        built.terrain.iter().position(|e| low(e, ".masktileatlas.res")),
    ) else {
        out.notes
            .push("tiled layers: the new terrain has no visual terrain or atlas".into());
        return out;
    };
    let (Some(visual), Some(atl)) = (
        built.terrain.get(vi).map(|e| e.data.clone()),
        built.terrain.get(ai).map(|e| e.data.clone()),
    ) else {
        return out;
    };
    let rasters: Vec<(LayerLay, crate::delta::Raster)> = lays
        .iter()
        .enumerate()
        .filter(|(_, l)| l.coverage > 0.0)
        .map(|(k, l)| (l.clone(), lay_layer(l, &g, seed, 0x7A1E_0000 + k as u64, 1.0)))
        .collect();
    // the layers the paint takes on their own (a combination it cannot make is left out)
    let mut ok: Vec<usize> = Vec::new();
    for (k, (l, r)) in rasters.iter().enumerate() {
        let one = [crate::terrainpaint::PaintLayer {
            layer: &l.layer,
            weights: r,
        }];
        match crate::terrainpaint::paint(&visual, &atl, n, &one, textures) {
            Ok(_) => ok.push(k),
            Err(e) => out.notes.push(format!("tiled layers: {} left out ({e})", l.layer)),
        }
    }
    let mut scale = 1.0;
    let mut cur: Vec<(LayerLay, crate::delta::Raster)> = ok.iter().filter_map(|&k| rasters.get(k).cloned()).collect();
    for round in 0..8 {
        let layers: Vec<crate::terrainpaint::PaintLayer<'_>> = cur
            .iter()
            .map(|(l, r)| crate::terrainpaint::PaintLayer {
                layer: &l.layer,
                weights: r,
            })
            .collect();
        match crate::terrainpaint::paint(&visual, &atl, n, &layers, textures) {
            Ok(p) => {
                out.notes.extend(p.notes.iter().map(|x| format!("tiled layers: {x}")));
                out.notes.push(format!(
                    "tiled layers: {} painted ({} cells, {} tiles, {} shader(s) cloned){}",
                    cur.iter().map(|x| x.0.layer.as_str()).collect::<Vec<_>>().join(", "),
                    p.cells,
                    p.tiles,
                    p.added.len(),
                    if round > 0 {
                        format!(", coverage x {scale:.2} for the atlas's room")
                    } else {
                        String::new()
                    }
                ));
                if let Some(e) = built.terrain.get_mut(vi) {
                    e.data = p.visual;
                }
                if let Some(e) = built.terrain.get_mut(ai) {
                    e.data = p.atlas;
                }
                out.clones = p.added;
                break;
            }
            Err(e) if round < 7 && !cur.is_empty() => {
                scale *= 2.0 / 3.0;
                out.notes
                    .push(format!("tiled layers: {e}; coverage cut to x {scale:.2}"));
                cur = cur
                    .iter()
                    .enumerate()
                    .map(|(k, (l, _))| (l.clone(), lay_layer(l, &g, seed, 0x7A1E_0000 + k as u64, scale)))
                    .collect();
            }
            Err(e) => {
                out.notes.push(format!("tiled layers: not painted ({e})"));
                cur.clear();
                break;
            }
        }
    }
    // the coverage laid, and the paired materials under the strong masks (TEDIT-MM-2)
    let Some(mi) = built.terrain.iter().position(|e| low(e, "-materialmap.res")) else {
        return out;
    };
    let mut added: Vec<String> = Vec::new();
    let Some(base_map) = built.terrain.get(mi).map(|e| e.data.clone()) else {
        return out;
    };
    let Ok(m) = bc2_formats::terrain_codec::MaterialMap::decode(&base_map) else {
        return out;
    };
    let mut raster = crate::terrainedit::material_raster(&m);
    let mut names = m.names.clone();
    let per = (m.width / n.max(1)).max(1);
    for (l, r) in &cur {
        let dry: Vec<u16> = (0..n)
            .flat_map(|j| (0..n).map(move |i| (i, j)))
            .filter(|&(i, j)| {
                g.water
                    .is_none_or(|w| f64::from(g.heights.get((j * n + i) as usize).copied().unwrap_or(0.0)) > w)
            })
            .map(|(i, j)| r.get(i, j))
            .collect();
        let laid = dry.iter().map(|&v| f64::from(v) / 255.0).sum::<f64>() / dry.len().max(1) as f64;
        out.layers.push((l.layer.clone(), l.coverage, laid, l.material.clone()));
        let Some(mat) = &l.material else { continue };
        let idx = if let Some(k) = names.iter().position(|x| x.eq_ignore_ascii_case(mat)) {
            k
        } else if names.len() < crate::terrainedit::MAX_MATERIALS {
            names.push(mat.clone());
            added.push(mat.clone());
            names.len() - 1
        } else {
            continue;
        };
        for j in 0..n {
            for i in 0..n {
                if r.get(i, j) >= 200 {
                    for b in 0..per {
                        for a in 0..per {
                            raster.set(i * per + a, j * per + b, idx as u16);
                        }
                    }
                }
            }
        }
    }
    if let Ok(written) = crate::terrainedit::write_materials_named(&base_map, &raster, &added) {
        if let Some(e) = built.terrain.get_mut(mi) {
            e.data = written;
        }
        if !added.is_empty() {
            out.notes.push(format!(
                "tiled layers: the material map names {} added for the layers' paired materials (TEDIT-MM-2)",
                added.join(", ")
            ));
        }
    }
    let _ = name;
    out
}

// ------------------------------------------------------------------------------------------------ the editor's view

/// A Create Level project's new terrain as the editor shows and sculpts it (round TG4): the resources the export
/// builds (the terrain, its visual side, the donor's tiled layers painted), each under the **base's** name (the
/// level's partition names the base's terrain until the export renames it, so the strokes' targets are the base's
/// files and the export lays them on the new terrain's: `export`'s `mapped_name`).
#[derive(Debug, Clone, Default)]
pub struct View {
    /// The base terrain's stem (`terrains/mp_005/mp_005`).
    pub stem: String,
    /// The resources by the base's lower-case names (`terrains/mp_005/mp_005.heightfield-000.res` ...).
    pub files: BTreeMap<String, Vec<u8>>,
    /// The colour / occlusion map under the base's name (`terrains/mp_005/textures/mp05_ambientocclusion.res`).
    pub colour: Option<(String, Vec<u8>)>,
    /// Lines for the editor's notes.
    pub notes: Vec<String>,
}

/// The new terrain of a project on a base level, built as the export builds it ([`View`]).
///
/// # Errors
/// The base's terrain does not read, or the terrain does not build.
pub fn view(game: &Game, base: &str, t: &TerrainImport, project_file: &Path) -> Result<View> {
    let dx9 = crate::shaders::level_database(game, base, "dx9_single.res")?;
    let donor = DonorFiles::read(game, base, dx9.as_deref())?;
    let (terrain, mut notes) = terrain_of(t, project_file, donor.water_level)?;
    // the name is the export's to give; any name builds the same bytes but for the names inside
    let tname = "NEWTERRAIN";
    let mut built = build(&terrain, tname, &donor)?;
    let textures = dx9
        .as_deref()
        .and_then(|db| crate::terrainpaint::entry_textures(db).ok());
    let tiled = paint_tiled(
        &mut built,
        &terrain,
        tname,
        &donor,
        textures.as_ref(),
        seed_of_import(t),
    );
    notes.extend(tiled.notes);
    let to = format!("terrains/{0}/{0}", tname.to_ascii_lowercase());
    let mut files = BTreeMap::new();
    for e in &built.terrain {
        let low = e.name.to_ascii_lowercase();
        if let Some(rest) = low.strip_prefix(&to) {
            files.insert(format!("{}{rest}", donor.stem), e.data.clone());
        }
    }
    let colour = donor.occlusion.as_ref().and_then(|(old, _)| {
        let (_, new) = built.textures.first()?;
        let data = built
            .level
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&format!("{new}.res")) || e.name.eq_ignore_ascii_case(new))?
            .data
            .clone();
        Some((format!("{}.res", old.to_ascii_lowercase()), data))
    });
    Ok(View {
        stem: donor.stem,
        files,
        colour,
        notes,
    })
}
