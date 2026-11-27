//! "Create level - from a height map" (round G4, the user's design) and "Generate" beside it (round TG3): the height
//! map file checked, the import's settings checked, and a project saved on a **base level** that records them with
//! the height map (and the optional colour map) kept beside it, or the terrain generator's settings
//! (`bfbc2 terrain generate`'s: biome, size, seed, border, water share, pads). The project opens on its base level as
//! any project does; its export writes the new terrain as the level's own (round E7:
//! `bc2_levelwrite::newterrain`, spec `transforms/terrain_edit.md` TEDIT-NEW-*). The base gives the level everything
//! that is not terrain and its terrain's look (TGEN-BIOME-4: its patch shaders, atlas layout and colour map).
//!
//! **The sizes BFBC2's terrain takes** (`bc2_formats::terrain`): a level's main height field is a power of two
//! samples a side, and the reader knows the tail of the two sizes the game's levels use - 1024 and 2048 samples
//! (`terrain::FIXED`). A height map of 1024 or 2048 samples a side is taken as it is; 1025 or 2049 (a power of two
//! plus one, as terrain tools export) also, its last row and column dropped when the terrain is generated. Any other
//! size, or a map that is not square, is refused with the sizes it could be.
//!
//! **The files:** a 16-bit greyscale PNG, or RAW 16-bit samples row by row (no header) whose size and byte order the
//! user gives (the file's length must be exactly `2 x side x side`).

use std::fmt::Write as _;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use bc2_levelwrite::project::{HeightFormat, KeptFile, TerrainImport, sha1_hex};
use bc2_levelwrite::{MapProject, Profile};

use crate::error::EditorError;
use crate::levels::ContentSet;

/// The samples a side the import takes: BFBC2's two main terrain sizes, and each plus one.
pub const SIDES: [u32; 4] = [1024, 1025, 2048, 2049];

/// How the height map's samples are stored, as the form chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A 16-bit greyscale PNG.
    Png,
    /// RAW 16-bit samples, `side` x `side`, in this byte order.
    Raw {
        /// Samples per side.
        side: u32,
        /// Little-endian (PC) or big-endian.
        little_endian: bool,
    },
}

impl Source {
    /// The project file's record of it.
    #[must_use]
    pub fn format(self) -> HeightFormat {
        match self {
            Self::Png => HeightFormat::Png16,
            Self::Raw { side, little_endian } => HeightFormat::Raw16 { side, little_endian },
        }
    }
}

/// A height map that passed the checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// Samples per side.
    pub samples: u32,
    /// Its lowest and highest sample.
    pub range: [u16; 2],
    /// The file's size in bytes.
    pub bytes: u64,
    /// The file's sha1.
    pub sha1: String,
}

impl Checked {
    /// What the form says of it.
    #[must_use]
    pub fn summary(&self) -> String {
        let n = self.samples;
        let mut s = format!(
            "{n} x {n} samples, 16-bit, values {} - {}",
            self.range[0], self.range[1]
        );
        if !n.is_power_of_two() {
            let _ = write!(
                s,
                " (its last row and column are dropped: BFBC2's terrain is {} a side)",
                n - 1
            );
        }
        if self.range[0] == self.range[1] {
            s.push_str(" - every sample is the same: the terrain would be flat");
        }
        s
    }
}

/// Whether `side` is a size the import takes, or why not.
///
/// # Errors
/// The size in words, with the sizes it could be.
pub fn side_ok(w: u32, h: u32) -> Result<(), EditorError> {
    if w != h {
        return Err(EditorError::Input(format!(
            "The height map is {w} x {h}: BFBC2's terrain is square - 1024 or 2048 samples a side (1025 or 2049 also \
             do: the last row and column are dropped)."
        )));
    }
    if !SIDES.contains(&w) {
        return Err(EditorError::Input(format!(
            "The height map is {w} x {h}: BFBC2's terrain is 1024 or 2048 samples a side (1025 or 2049 also do: the \
             last row and column are dropped). Resize it in your terrain tool."
        )));
    }
    Ok(())
}

fn range_of(samples: impl Iterator<Item = u16>) -> [u16; 2] {
    samples.fold([u16::MAX, 0], |[lo, hi], v| [lo.min(v), hi.max(v)])
}

/// Checks a height map file: it reads, it is a 16-bit PNG or a RAW of the size given, and its size is one BFBC2's
/// terrain takes ([`SIDES`]).
///
/// # Errors
/// Why it cannot be used, in words (it does not read; not a PNG; not 16-bit greyscale; the wrong size).
pub fn check(path: &Path, source: Source) -> Result<Checked, EditorError> {
    let shown = path.display();
    if path.as_os_str().is_empty() {
        return Err(EditorError::Input("Choose a height map file.".into()));
    }
    let data = fs::read(path).map_err(|e| EditorError::Input(format!("{shown} does not read: {e}")))?;
    let bytes = data.len() as u64;
    let (samples, range) = match source {
        Source::Png => png16(&data).map_err(|e| EditorError::Input(format!("{shown}: {e}")))?,
        Source::Raw { side, little_endian } => {
            let want = 2 * u64::from(side) * u64::from(side);
            if bytes != want {
                let guess = SIDES
                    .iter()
                    .find(|&&s| 2 * u64::from(s) * u64::from(s) == bytes)
                    .map_or_else(String::new, |s| {
                        format!(" It has the length of a {s} x {s} RAW: choose {s}.")
                    });
                return Err(EditorError::Input(format!(
                    "{shown} is {bytes} bytes; a {side} x {side} RAW of 16-bit samples is {want} bytes.{guess}"
                )));
            }
            side_ok(side, side)?;
            let range = range_of(data.as_chunks::<2>().0.iter().map(|&b| {
                if little_endian {
                    u16::from_le_bytes(b)
                } else {
                    u16::from_be_bytes(b)
                }
            }));
            (side, range)
        }
    };
    Ok(Checked {
        samples,
        range,
        bytes,
        sha1: sha1_hex(&data),
    })
}

/// A 16-bit greyscale PNG's side and sample range.
fn png16(data: &[u8]) -> Result<(u32, [u16; 2]), EditorError> {
    let mut d = png::Decoder::new(Cursor::new(data));
    d.set_transformations(png::Transformations::IDENTITY);
    let mut r = d
        .read_info()
        .map_err(|e| EditorError::Input(format!("not a PNG that reads ({e})")))?;
    let (w, h, colour, depth) = {
        let i = r.info();
        (i.width, i.height, i.color_type, i.bit_depth)
    };
    if colour != png::ColorType::Grayscale || depth != png::BitDepth::Sixteen {
        let what = match (colour, depth) {
            (png::ColorType::Grayscale, d) => format!("{}-bit greyscale", d as u8),
            (c, d) => format!("{c:?} at {} bits", d as u8),
        };
        return Err(EditorError::Input(format!(
            "the PNG is {what}; a height map must be 16-bit greyscale (8 bits give only 256 heights). Export it as \
             16-bit greyscale."
        )));
    }
    side_ok(w, h)?;
    let mut buf = vec![
        0;
        r.output_buffer_size()
            .ok_or(EditorError::Input("the PNG is too large".into()))?
    ];
    let info = r
        .next_frame(&mut buf)
        .map_err(|e| EditorError::Input(format!("the PNG's pixels do not read ({e})")))?;
    buf.truncate(info.buffer_size());
    // PNG stores 16-bit samples big-endian
    let range = range_of(buf.as_chunks::<2>().0.iter().map(|&b| u16::from_be_bytes(b)));
    Ok((w, range))
}

/// Checks the optional colour map: a PNG that reads; its size.
///
/// # Errors
/// It does not read, or is not a PNG.
pub fn check_colour(path: &Path) -> Result<[u32; 2], EditorError> {
    let f = fs::File::open(path).map_err(|e| EditorError::Input(format!("{} does not read: {e}", path.display())))?;
    let r = png::Decoder::new(std::io::BufReader::new(f))
        .read_info()
        .map_err(|e| EditorError::Input(format!("{}: not a PNG that reads ({e})", path.display())))?;
    let i = r.info();
    Ok([i.width, i.height])
}

/// The import's numbers, as the form has them.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The mode whose content the level is made from.
    pub content_set: ContentSet,
    /// The map's size along x and z in metres.
    pub map_size_m: f64,
    /// The height of the lowest sample value (0) in metres.
    pub height_min_m: f64,
    /// The height of the highest sample value (65535) in metres.
    pub height_max_m: f64,
    /// The water's height in metres.
    pub water_level_m: f64,
    /// The base level the new level is built on (round E7: a multiplayer map with a terrain of its own; its
    /// everything-but-terrain and its terrain's look).
    pub base: String,
    /// The biome whose materials a height map's material map is painted with (TGEN-MAT-1).
    pub biome: bc2_terragen::Biome,
    /// A sub-terrain ring around a height map's terrain (TGEN-RING-1).
    pub ring: bool,
    /// Round TG4: the base's forests, middlegrowth, rocks and natural props placed again on the new terrain
    /// (TGEN-VEG-1 - 3), for both forms.
    pub vegetation: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            content_set: ContentSet::Bfbc2,
            map_size_m: 2048.0,
            height_min_m: 0.0,
            height_max_m: 300.0,
            water_level_m: 0.0,
            base: DEFAULT_BASE.to_owned(),
            biome: bc2_terragen::Biome::Arid,
            ring: false,
            vegetation: true,
        }
    }
}

/// The base the forms start with: `MP_005` (Atacama Desert), TG2's donor.
pub const DEFAULT_BASE: &str = "mp_005";

/// Whether a level can be a new level's base (round E7): a multiplayer map of its own - `mp_001` ... `mp_012`,
/// `nam_mp_002r` ... - not a mode variant of one (`mp_005gr` loads `mp_005`'s terrain) nor a campaign level.
#[must_use]
pub fn is_base_candidate(lv: &str) -> bool {
    let low = lv.to_ascii_lowercase();
    let rest = low.strip_prefix("nam_").unwrap_or(&low);
    rest.strip_prefix("mp_").is_some_and(|n| {
        let d = n.trim_end_matches('r');
        d.len() == 3 && d.bytes().all(|b| b.is_ascii_digit())
    })
}

/// The largest map the form takes, in metres (BFBC2's largest levels are about 2 km; 16 km is far beyond).
pub const MAX_MAP_M: f64 = 16_384.0;

impl Settings {
    /// Whether the numbers make a terrain, or why not.
    ///
    /// # Errors
    /// The first number that does not, in words.
    pub fn check(&self) -> Result<(), EditorError> {
        let all = [
            self.map_size_m,
            self.height_min_m,
            self.height_max_m,
            self.water_level_m,
        ];
        if all.iter().any(|v| !v.is_finite()) {
            return Err(EditorError::Input("Every number must be a number.".into()));
        }
        if !(64.0..=MAX_MAP_M).contains(&self.map_size_m) {
            return Err(EditorError::Input(format!(
                "The map size must be 64 to {MAX_MAP_M} metres."
            )));
        }
        // TGEN-REQ-2: BFBC2's terrain has 1 m samples, 1024 or 2048 a side; no other spacing is known to load
        if !matches!(self.map_size_m.to_bits(), x if x == 1024f64.to_bits() || x == 2048f64.to_bits()) {
            return Err(EditorError::Input(
                "The map size must be 1024 or 2048 metres: BFBC2's terrain has 1 m samples, 1024 or 2048 a side \
                 (no other spacing is known to load)."
                    .into(),
            ));
        }
        if self.base.trim().is_empty() {
            return Err(EditorError::Input("Choose the base level.".into()));
        }
        if self.height_max_m <= self.height_min_m {
            return Err(EditorError::Input(
                "The highest height must be above the lowest.".into(),
            ));
        }
        if self.height_max_m - self.height_min_m > 4096.0 {
            return Err(EditorError::Input(
                "The height range must be at most 4096 metres.".into(),
            ));
        }
        Ok(())
    }

    /// A note on the water, when it is outside the height range (allowed, said).
    #[must_use]
    pub fn water_note(&self) -> Option<&'static str> {
        if self.water_level_m < self.height_min_m {
            Some("The water is below the lowest ground: no water shows.")
        } else if self.water_level_m > self.height_max_m {
            Some("The water is above the highest ground: the whole map is under water.")
        } else {
            None
        }
    }
}

/// The content profile of a content set (BFBC1's maps use BFBC2's assets and kits).
#[must_use]
pub fn profile_of(set: ContentSet) -> Profile {
    match set {
        ContentSet::Bfbc1 | ContentSet::Bfbc2 => Profile::Bfbc2,
        ContentSet::Vietnam => Profile::Vietnam,
        ContentSet::Ww2 => Profile::Ww2,
    }
}

/// A file copied beside the project as `<stem>.<what>.<its extension>`.
fn keep(src: &Path, folder: &Path, stem: &str, what: &str) -> Result<KeptFile, EditorError> {
    let ext = src
        .extension()
        .map_or_else(|| "bin".to_owned(), |e| e.to_string_lossy().to_lowercase());
    let file = format!("{stem}.{what}.{ext}");
    let data = fs::read(src).map_err(|e| EditorError::Input(format!("{} does not read: {e}", src.display())))?;
    let dest = folder.join(&file);
    fs::write(&dest, &data).map_err(|e| EditorError::Input(format!("{} could not be written: {e}", dest.display())))?;
    Ok(KeptFile {
        file,
        source: src.display().to_string(),
        size: data.len() as u64,
        sha1: sha1_hex(&data),
    })
}

/// What "Create" saves.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The new level's name.
    pub name: &'a str,
    /// The project's folder.
    pub folder: &'a Path,
    /// The height map and how it is stored.
    pub heightmap: &'a Path,
    /// How its samples are stored.
    pub source: Source,
    /// The optional colour map.
    pub colour_map: Option<&'a Path>,
    /// The numbers and the content set.
    pub settings: Settings,
    /// The base level's partition as the game copy has it (the project's base sha1).
    pub base_bytes: &'a [u8],
}

/// The generator's settings ("Generate", round TG3) as the form has them.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateForm {
    /// The biome.
    pub biome: bc2_terragen::Biome,
    /// 1,024 or 2,048 samples (and metres).
    pub size: u32,
    /// The seed.
    pub seed: u64,
    /// The border's median minus the centre's (`None`: the biome's).
    pub border_m: Option<f64>,
    /// The share of ground under water (`None`: the biome's).
    pub water_share: Option<f64>,
    /// The water's height (`None`: the base's, TGEN-VIS-2).
    pub water_level_m: Option<f64>,
    /// The escarpment's strength (`None`: the biome's).
    pub rough: Option<f64>,
    /// Objective pads (x, z, radius).
    pub pads: Vec<[f64; 3]>,
    /// The sub-terrain ring.
    pub ring: bool,
}

impl Default for GenerateForm {
    fn default() -> Self {
        Self {
            biome: bc2_terragen::Biome::Arid,
            size: 1024,
            seed: 7,
            border_m: None,
            water_share: None,
            water_level_m: None,
            rough: None,
            pads: Vec::new(),
            ring: true,
        }
    }
}

impl GenerateForm {
    /// Whether the generator takes these settings, or why not (its own checks: the size, the share, the pads).
    ///
    /// # Errors
    /// The generator's refusal in words.
    pub fn check(&self) -> Result<(), EditorError> {
        let mut s = bc2_terragen::Settings::new(self.biome, self.size, self.seed);
        s.water_share = self.water_share;
        s.pads = self
            .pads
            .iter()
            .map(|p| bc2_terragen::Pad {
                x: p[0],
                z: p[1],
                radius: p[2],
            })
            .collect();
        if !matches!(self.size, 1024 | 2048) {
            return Err(EditorError::Input(format!(
                "size {}: BFBC2's terrain is 1024 or 2048 samples a side",
                self.size
            )));
        }
        if self.water_share.is_some_and(|w| !(0.0..=1.0).contains(&w)) {
            return Err(EditorError::Input("The water share is 0 to 1.".into()));
        }
        if self.rough.is_some_and(|w| !(0.0..=1.0).contains(&w)) {
            return Err(EditorError::Input("The rough relief is 0 to 1.".into()));
        }
        let half = f64::from(self.size) / 2.0;
        if let Some([x, z, r]) = self
            .pads
            .iter()
            .find(|[x, z, r]| x.abs() >= half || z.abs() >= half || *r <= 0.0)
        {
            return Err(EditorError::Input(format!(
                "The pad at {x}, {z} (radius {r}) is outside the {} m square or has no radius.",
                self.size
            )));
        }
        Ok(())
    }

    /// The project's record of it.
    #[must_use]
    pub fn spec(&self) -> bc2_levelwrite::project::GenerateSpec {
        bc2_levelwrite::project::GenerateSpec {
            biome: self.biome.name().to_owned(),
            size: self.size,
            seed: self.seed,
            border_m: self.border_m,
            water_share: self.water_share,
            pads: self.pads.clone(),
            rough: self.rough,
            erosion: None,
            ring: self.ring,
            water_level_m: self.water_level_m,
        }
    }
}

/// Pads as the form types them: `x, z[, radius]` separated by `;` (radius 24 by default, as `--pad`).
///
/// # Errors
/// A pad that is not two or three numbers.
pub fn parse_pads(text: &str) -> Result<Vec<[f64; 3]>, EditorError> {
    text.split(';')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| {
            let v: Vec<f64> = t
                .split(',')
                .map(|x| x.trim().parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|_| EditorError::Input(format!("Pad {t:?}: x, z or x, z, radius")))?;
            match v.as_slice() {
                [x, z] => Ok([*x, *z, 24.0]),
                [x, z, r] => Ok([*x, *z, *r]),
                _ => Err(EditorError::Input(format!("Pad {t:?}: x, z or x, z, radius"))),
            }
        })
        .collect()
}

/// What "Create" of the generator saves (round TG3).
#[derive(Debug, Clone)]
pub struct GenerateRequest<'a> {
    /// The new level's name.
    pub name: &'a str,
    /// The project's folder.
    pub folder: &'a Path,
    /// The content set and the base level (the form's other numbers are the height map's).
    pub settings: Settings,
    /// The generator's settings.
    pub generate: GenerateForm,
    /// The base level's partition as the game copy has it.
    pub base_bytes: &'a [u8],
}

/// Saves a generated level's project (round TG3): no file is kept, the settings generate the terrain again
/// (TGEN-GEN-1); the project's path.
///
/// # Errors
/// A check fails, a project of that name is already there, or it cannot be written.
pub fn save_generated(r: &GenerateRequest<'_>) -> Result<PathBuf, EditorError> {
    let name = r.name.trim();
    if name.is_empty() {
        return Err(EditorError::Input("Give the new level a name.".into()));
    }
    if r.folder.as_os_str().is_empty() {
        return Err(EditorError::Input("Choose a folder for the project.".into()));
    }
    r.generate.check()?;
    if r.base_bytes.is_empty() {
        return Err(EditorError::Input(format!(
            "The base level {} does not read from the game copy.",
            r.settings.base
        )));
    }
    let size = f64::from(r.generate.size);
    let project = MapProject::new_level(
        name,
        &r.settings.base.to_ascii_lowercase(),
        r.base_bytes,
        profile_of(r.settings.content_set),
        TerrainImport {
            content_set: set_word(r.settings.content_set).into(),
            heightmap: KeptFile {
                file: String::new(),
                source: String::new(),
                size: 0,
                sha1: String::new(),
            },
            format: HeightFormat::Png16,
            samples: r.generate.size,
            sample_range: [0, 0],
            map_size_m: size,
            height_range_m: [0.0, 0.0],
            water_level_m: r.generate.water_level_m.unwrap_or(0.0),
            colour_map: None,
            generate: Some(r.generate.spec()),
            biome: Some(r.generate.biome.name().to_owned()),
            ring: r.generate.ring,
            vegetation: r
                .settings
                .vegetation
                .then(bc2_levelwrite::project::VegetationSpec::default),
            strokes_on_base: false,
        },
    );
    write_new(&project, r.folder)
}

/// Writes a new project into `folder`, refusing to overwrite one.
fn write_new(project: &MapProject, folder: &Path) -> Result<PathBuf, EditorError> {
    let path = folder.join(project.file_name());
    if path.exists() {
        return Err(EditorError::Input(format!(
            "{} is already in this folder. Open it with \"Open project\", or choose another name.",
            path.display()
        )));
    }
    fs::create_dir_all(folder)
        .map_err(|e| EditorError::Input(format!("{} could not be made: {e}", folder.display())))?;
    project
        .save(&path)
        .map_err(|e| EditorError::Input(format!("The project could not be written to {}: {e}", path.display())))?;
    Ok(path)
}

/// Checks everything again and saves the project (`<name>.mapedit.json` in the folder), the height map and the colour
/// map copied beside it; the project file's path.
///
/// # Errors
/// A check fails, a project of that name is already there, or a file cannot be written.
pub fn save(r: &Request<'_>) -> Result<PathBuf, EditorError> {
    let name = r.name.trim();
    if name.is_empty() {
        return Err(EditorError::Input("Give the new level a name.".into()));
    }
    if r.folder.as_os_str().is_empty() {
        return Err(EditorError::Input("Choose a folder for the project.".into()));
    }
    let checked = check(r.heightmap, r.source)?;
    r.settings.check()?;
    let side = if checked.samples.is_power_of_two() {
        checked.samples
    } else {
        checked.samples - 1
    };
    if (r.settings.map_size_m - f64::from(side)).abs() > 1e-9 {
        return Err(EditorError::Input(format!(
            "The height map is {side} samples a side, so the map is {side} m across (BFBC2's terrain has 1 m \
             samples): set the map size to {side}."
        )));
    }
    if let Some(c) = r.colour_map {
        check_colour(c)?;
    }
    if r.base_bytes.is_empty() {
        return Err(EditorError::Input(format!(
            "The base level {} does not read from the game copy.",
            r.settings.base
        )));
    }
    let profile = profile_of(r.settings.content_set);
    let mut project = MapProject::new_level(
        name,
        &r.settings.base.to_ascii_lowercase(),
        r.base_bytes,
        profile,
        TerrainImport {
            content_set: set_word(r.settings.content_set).into(),
            heightmap: KeptFile {
                file: String::new(),
                source: String::new(),
                size: 0,
                sha1: String::new(),
            },
            format: r.source.format(),
            samples: checked.samples,
            sample_range: checked.range,
            map_size_m: r.settings.map_size_m,
            height_range_m: [r.settings.height_min_m, r.settings.height_max_m],
            water_level_m: r.settings.water_level_m,
            colour_map: None,
            generate: None,
            biome: Some(r.settings.biome.name().to_owned()),
            ring: r.settings.ring,
            vegetation: r
                .settings
                .vegetation
                .then(bc2_levelwrite::project::VegetationSpec::default),
            strokes_on_base: false,
        },
    );
    let file = project.file_name();
    let path = r.folder.join(&file);
    if path.exists() {
        return Err(EditorError::Input(format!(
            "{} is already in this folder. Open it with \"Open project\", or choose another name.",
            path.display()
        )));
    }
    fs::create_dir_all(r.folder)
        .map_err(|e| EditorError::Input(format!("{} could not be made: {e}", r.folder.display())))?;
    let stem = file.strip_suffix(bc2_levelwrite::project::EXTENSION).unwrap_or(&file);
    let hm = keep(r.heightmap, r.folder, stem, "heightmap")?;
    if hm.sha1 != checked.sha1 {
        return Err(EditorError::Input(format!(
            "{} changed while it was copied: try again.",
            r.heightmap.display()
        )));
    }
    let colour = r.colour_map.map(|c| keep(c, r.folder, stem, "colour")).transpose()?;
    if let Some(t) = project.terrain_import.as_mut() {
        t.heightmap = hm;
        t.colour_map = colour;
    }
    project
        .save(&path)
        .map_err(|e| EditorError::Input(format!("The project could not be written to {}: {e}", path.display())))?;
    Ok(path)
}

/// A content set as the project file names it.
#[must_use]
pub fn set_word(set: ContentSet) -> &'static str {
    match set {
        ContentSet::Bfbc1 => "BFBC1",
        ContentSet::Bfbc2 => "BFBC2",
        ContentSet::Vietnam => "Vietnam",
        ContentSet::Ww2 => "WW2",
    }
}

/// Writes a 16-bit greyscale PNG (the tests' height maps).
///
/// # Errors
/// The file cannot be written.
pub fn write_png16(path: &Path, side: u32, sample: &dyn Fn(u32, u32) -> u16) -> Result<(), EditorError> {
    let f = std::io::BufWriter::new(fs::File::create(path).map_err(|e| EditorError::Input(e.to_string()))?);
    let mut e = png::Encoder::new(f, side, side);
    e.set_color(png::ColorType::Grayscale);
    e.set_depth(png::BitDepth::Sixteen);
    let mut w = e.write_header().map_err(|e| EditorError::Input(e.to_string()))?;
    let mut data = Vec::with_capacity((side * side * 2) as usize);
    for y in 0..side {
        for x in 0..side {
            data.extend_from_slice(&sample(x, y).to_be_bytes());
        }
    }
    w.write_image_data(&data).map_err(|e| EditorError::Input(e.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bc2_editor_heightmap_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sizes() {
        assert!(side_ok(1024, 1024).is_ok() && side_ok(2049, 2049).is_ok());
        assert!(side_ok(1024, 2048).unwrap_err().contains("square"));
        assert!(side_ok(512, 512).unwrap_err().contains("1024 or 2048"));
        assert!(side_ok(4097, 4097).is_err());
    }

    #[test]
    fn files_checked() {
        let d = temp("files");
        // a good PNG: 1025 a side, a ramp
        let good = d.join("good.png");
        write_png16(&good, 1025, &|x, _| (x * 60) as u16).unwrap();
        let c = check(&good, Source::Png).unwrap();
        assert_eq!((c.samples, c.range), (1025, [0, 61_440]));
        assert!(c.summary().contains("dropped"));
        // the wrong size
        let small = d.join("small.png");
        write_png16(&small, 300, &|_, _| 7).unwrap();
        assert!(check(&small, Source::Png).unwrap_err().contains("300 x 300"));
        // unreadable: missing, not a PNG, 8-bit
        assert!(
            check(&d.join("none.png"), Source::Png)
                .unwrap_err()
                .contains("does not read")
        );
        fs::write(d.join("text.png"), b"hello").unwrap();
        assert!(
            check(&d.join("text.png"), Source::Png)
                .unwrap_err()
                .contains("not a PNG")
        );
        let eight = d.join("eight.png");
        {
            let f = fs::File::create(&eight).unwrap();
            let mut e = png::Encoder::new(f, 4, 4);
            e.set_color(png::ColorType::Grayscale);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&[0; 16]).unwrap();
        }
        assert!(check(&eight, Source::Png).unwrap_err().contains("8-bit greyscale"));
        // RAW: the byte order chosen, the length checked
        let raw = d.join("h.raw");
        let mut bytes = vec![0u8; 2 * 1024 * 1024];
        bytes.splice(0..2, 0x0102u16.to_le_bytes());
        fs::write(&raw, &bytes).unwrap();
        let le = Source::Raw {
            side: 1024,
            little_endian: true,
        };
        assert_eq!(check(&raw, le).unwrap().range, [0, 0x0102]);
        let be = Source::Raw {
            side: 1024,
            little_endian: false,
        };
        assert_eq!(check(&raw, be).unwrap().range, [0, 0x0201]);
        let e = check(
            &raw,
            Source::Raw {
                side: 2048,
                little_endian: true,
            },
        )
        .unwrap_err();
        assert!(e.contains("choose 1024"), "{e}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn settings_checked() {
        let s = Settings::default();
        assert!(s.check().is_ok() && s.water_note().is_none());
        let flat = Settings {
            height_max_m: 0.0,
            ..s.clone()
        };
        assert!(flat.check().unwrap_err().contains("above the lowest"));
        assert!(
            Settings {
                map_size_m: 10.0,
                ..s.clone()
            }
            .check()
            .is_err()
        );
        // TGEN-REQ-2: 1 m samples, 1024 or 2048 a side
        assert!(
            Settings {
                map_size_m: 1500.0,
                ..s.clone()
            }
            .check()
            .unwrap_err()
            .contains("1024 or 2048")
        );
        assert!(
            Settings {
                base: String::new(),
                ..s.clone()
            }
            .check()
            .unwrap_err()
            .contains("base")
        );
        assert!(is_base_candidate("mp_005") && is_base_candidate("NAM_MP_002R") && is_base_candidate("mp_012"));
        assert!(!is_base_candidate("mp_005gr") && !is_base_candidate("sp_01") && !is_base_candidate("mp_sp_005"));
        // the generator's own checks
        let g = GenerateForm::default();
        assert!(g.check().is_ok());
        assert!(
            GenerateForm {
                size: 1500,
                ..g.clone()
            }
            .check()
            .is_err()
        );
        assert!(
            GenerateForm {
                water_share: Some(2.0),
                ..g.clone()
            }
            .check()
            .is_err()
        );
        assert!(
            GenerateForm {
                pads: vec![[600.0, 0.0, 10.0]],
                ..g.clone()
            }
            .check()
            .is_err()
        );
        assert_eq!(
            parse_pads("1, 2; 3,4,5 ;").unwrap(),
            vec![[1.0, 2.0, 24.0], [3.0, 4.0, 5.0]]
        );
        assert!(parse_pads("1").is_err() && parse_pads("a, b").is_err());
        assert!(
            Settings {
                map_size_m: f64::NAN,
                ..s.clone()
            }
            .check()
            .is_err()
        );
        assert!(
            Settings {
                water_level_m: -5.0,
                ..s.clone()
            }
            .water_note()
            .unwrap()
            .contains("below")
        );
    }

    #[test]
    fn a_project_saved() {
        let d = temp("save");
        let hm = d.join("in.png");
        write_png16(&hm, 1024, &|x, y| (x + y) as u16).unwrap();
        let colour = d.join("colour.png");
        {
            let f = fs::File::create(&colour).unwrap();
            let mut e = png::Encoder::new(f, 2, 2);
            e.set_color(png::ColorType::Rgba);
            e.write_header().unwrap().write_image_data(&[90; 16]).unwrap();
        }
        assert_eq!(check_colour(&colour).unwrap(), [2, 2]);
        let folder = d.join("projects");
        let r = Request {
            name: "Atoll",
            folder: &folder,
            heightmap: &hm,
            source: Source::Png,
            colour_map: Some(&colour),
            settings: Settings {
                content_set: ContentSet::Ww2,
                water_level_m: 12.5,
                ..Settings::default()
            },
            base_bytes: b"base partition",
        };
        // the map's size must be the height map's (1 m samples)
        assert!(save(&r).unwrap_err().contains("set the map size to 1024"));
        let r = Request {
            settings: Settings {
                map_size_m: 1024.0,
                ..r.settings.clone()
            },
            ..r
        };
        let path = save(&r).unwrap();
        assert_eq!(path, folder.join("Atoll.mapedit.json"));
        let p = MapProject::load(&path).unwrap();
        assert!(!p.is_import() && p.new_terrain().is_some());
        assert_eq!(p.base.level, "mp_005");
        assert_eq!(p.base.profile, Profile::Ww2);
        let t = p.terrain_import.unwrap();
        assert_eq!(t.content_set, "WW2");
        assert_eq!((t.samples, t.sample_range), (1024, [0, 2046]));
        assert!((t.water_level_m - 12.5).abs() < 1e-9);
        assert_eq!(
            fs::read(folder.join(&t.heightmap.file)).unwrap(),
            fs::read(&hm).unwrap()
        );
        assert_eq!(t.heightmap.file, "Atoll.heightmap.png");
        assert_eq!(t.colour_map.unwrap().file, "Atoll.colour.png");
        // the same name again: refused
        assert!(save(&r).unwrap_err().contains("already"));
        let _ = fs::remove_dir_all(&d);
    }
}
