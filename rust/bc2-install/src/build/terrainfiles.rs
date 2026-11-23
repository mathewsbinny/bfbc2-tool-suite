//! The level's terrain bundle (`wake_install.terrain_files`, `undergrowth_scales`, `undergrowth_blocks`,
//! `_undergrowth_uses`): BF1943's terrain resources converted (the suite's terrain and texture converters), the
//! VisualTerrain's stand-in undergrowth given their DICE donors' 12 parameters and the strips of a kill switch
//! emptied. The VisualTerrain's undergrowth and strip records are walked here (the walk `terrain._Walk` made in
//! Python, retired with it).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use super::closure::{Src, Take};
use super::index::StockIndex;
use super::{OMap, Result, bundle_one, pjoin, write};

/// A road vertex: 5 floats and 4 half floats.
const ROAD_VERTEX: usize = 28;

struct Walk<'a> {
    x: &'a [u8],
    o: usize,
}

impl Walk<'_> {
    fn need(&self, n: usize) -> Result<()> {
        if self.o + n > self.x.len() {
            return Err(format!(
                "TerrainError: visual terrain: runs past the end at {:#x} (+{n} > {} bytes)",
                self.o,
                self.x.len()
            ));
        }
        Ok(())
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        self.need(n)?;
        self.o += n;
        Ok(())
    }

    fn u32(&mut self) -> Result<usize> {
        self.need(4)?;
        let v = self
            .x
            .get(self.o..self.o + 4)
            .and_then(|b| <[u8; 4]>::try_from(b).ok())
            .map_or(0, u32::from_le_bytes);
        self.o += 4;
        Ok(v as usize)
    }

    fn string(&mut self) -> Result<String> {
        let rest = self.x.get(self.o..).unwrap_or_default();
        let Some(e) = rest.iter().position(|&c| c == 0) else {
            return Err(format!(
                "TerrainError: visual terrain: a name at {:#x} has no end",
                self.o
            ));
        };
        let s: String = rest
            .get(..e)
            .unwrap_or_default()
            .iter()
            .map(|&c| char::from(c))
            .collect();
        self.o += e + 1;
        Ok(s)
    }
}

/// `_undergrowth_uses(pc)`: (mesh, offset of its 12 floats) of a PC VisualTerrain's undergrowth (its patch groups),
/// and (shader, offset of the triangle count) of each road strip.
///
/// # Errors
/// The walk runs past the end, or a name has no end.
#[allow(clippy::type_complexity, reason = "the Python's two lists")]
pub fn undergrowth_uses(pc: &[u8]) -> Result<(Vec<(String, usize)>, Vec<(String, usize)>)> {
    let mut w = Walk { x: pc, o: 0 };
    let (mut uses, mut strips) = (Vec::new(), Vec::new());
    let n = w.u32()?;
    for _ in 0..2 * n {
        for _ in 0..w.u32()? {
            let shader = w.string()?;
            w.string()?;
            w.string()?;
            let at = w.o;
            w.skip(8)?;
            w.skip(24)?;
            w.skip(4)?;
            strips.push((shader, at + 4));
        }
        let v = w.u32()?;
        let _ = w.u32()?;
        let i = w.u32()?;
        w.skip(v * ROAD_VERTEX)?;
        w.skip(i * 4)?;
    }
    for _ in 0..w.u32()? {
        w.string()?;
        w.string()?;
        w.skip(8)?;
        for _ in 0..w.u32()? {
            for _ in 0..w.u32()? {
                let mesh = w.string()?;
                w.skip(1)?;
                w.string()?;
                uses.push((mesh, w.o));
                w.skip(48)?;
                w.skip(2)?;
            }
        }
    }
    Ok((uses, strips))
}

static BLOCKS: LazyLock<Mutex<Option<Vec<(String, Vec<u8>)>>>> = LazyLock::new(|| Mutex::new(None));

/// `undergrowth_blocks()`: {stand-in mesh: the 12 parameters (bytes) its donor level's VisualTerrain uses most for the
/// donor mesh} (the first such block met on a tie), kept for the process.
///
/// # Errors
/// A donor in no VisualTerrain of its folder, or a VisualTerrain that cannot be read.
pub fn undergrowth_blocks(standins: &[(String, String)], index: &StockIndex) -> Result<Vec<(String, Vec<u8>)>> {
    if let Some(b) = BLOCKS.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
        return Ok(b.clone());
    }
    let vts: Vec<(&str, &str)> = index
        .bundles
        .iter()
        .flat_map(|b| {
            b.names
                .iter()
                .filter(|(_, _, t)| t == "VisualTerrain")
                .map(move |(_, n, _)| (b.path.as_str(), n.as_str()))
        })
        .collect();
    let mut out = Vec::new();
    for (new, donor) in standins {
        let dl = donor.to_lowercase();
        let folder = format!("{}/", dl.split('/').take(2).collect::<Vec<_>>().join("/"));
        let mut got: Vec<(Vec<u8>, usize)> = Vec::new();
        for (b, n) in &vts {
            if !n.to_lowercase().starts_with(&folder) {
                continue;
            }
            let pc = bundle_one(b, n)?;
            let (uses, _) = undergrowth_uses(&pc)?;
            for (mesh, at) in uses {
                if mesh.to_lowercase() == dl {
                    let block = pc.get(at..at + 48).unwrap_or_default().to_vec();
                    match got.iter_mut().find(|(k, _)| *k == block) {
                        Some((_, c)) => *c += 1,
                        None => got.push((block, 1)),
                    }
                }
            }
        }
        // Counter.most_common(1): the highest count, the first met of equal counts
        let best = got
            .iter()
            .enumerate()
            .max_by(|(i, a), (j, b)| a.1.cmp(&b.1).then(j.cmp(i)))
            .map(|(_, (k, _))| k.clone())
            .ok_or_else(|| format!("undergrowth: {donor} is in no VisualTerrain of {folder}"))?;
        out.push((new.clone(), best));
    }
    *BLOCKS.lock().unwrap_or_else(PoisonError::into_inner) = Some(out.clone());
    Ok(out)
}

/// Forgets the kept blocks (another stock index: a test's).
pub fn forget_blocks() {
    *BLOCKS.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

fn floats_shown(b: &[u8]) -> String {
    let v: Vec<String> = b
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| {
            let x = f64::from(f32::from_le_bytes(*c));
            bc2_pyrt::fmt::float_repr(bc2_pyrt::fmt::round_to(x, 3))
        })
        .collect();
    format!("[{}]", v.join(", "))
}

/// `undergrowth_scales(pc, report)`: the VisualTerrain with the stand-ins' 12 parameters set to their donors' and the
/// strips whose shader is in `strips_off` emptied.
///
/// # Errors
/// The walk fails, or a donor block cannot be found.
pub fn undergrowth_scales(
    pc: &[u8],
    standins: &[(String, String)],
    strips_off: &[String],
    index: &StockIndex,
    report: &mut Vec<String>,
) -> Result<Vec<u8>> {
    let mut out = pc.to_vec();
    let (uses, strips) = undergrowth_uses(pc)?;
    let mut off = 0;
    for (shader, at) in &strips {
        if strips_off.contains(shader)
            && let Some(s) = out.get_mut(*at..*at + 4)
        {
            s.copy_from_slice(&[0; 4]);
            off += 1;
        }
    }
    report.push(format!(
        "terrain strips: {} of {} drawn, {off} switched off (TERRAIN_STRIPS_OFF)",
        strips.len() - off,
        strips.len()
    ));
    let blocks = undergrowth_blocks(standins, index)?;
    let by: HashMap<&str, &Vec<u8>> = blocks.iter().map(|(k, v)| (k.as_str(), v)).collect();
    let mut done = 0;
    for (mesh, at) in &uses {
        if let Some(b) = by.get(mesh.as_str())
            && let Some(s) = out.get_mut(*at..*at + 48)
        {
            s.copy_from_slice(b);
            done += 1;
        }
    }
    let mut sorted: Vec<&(String, Vec<u8>)> = blocks.iter().collect();
    sorted.sort();
    let shown: Vec<String> = sorted
        .iter()
        .map(|(k, v)| format!("{}: {}", k.rsplit('/').next().unwrap_or_default(), floats_shown(v)))
        .collect();
    report.push(format!(
        "undergrowth: {done} stand-in mesh uses given their DICE levels' 12 parameters ({})",
        shown.join(", ")
    ));
    Ok(out)
}

/// What `terrain_files` converts and where (the level's bindings and OPFOR's stand-ins).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct TerrainSpec {
    /// The terrain's staging folder.
    pub stage: String,
    /// `UG_STANDIN`, in order.
    pub standins: Vec<(String, String)>,
    /// `TERRAIN_STRIPS_OFF`.
    #[serde(default)]
    pub strips_off: Vec<String>,
}

/// `terrain_files(stage, src, report)`: BF1943's terrain resources (`items`: (lower key, name, type, bytes) of
/// `terrains/<lv>/*.res`, sorted by key) converted and written into the stage: {lower name: (name, type, path)}.
///
/// # Errors
/// A resource of a type with no converter, or a conversion that fails.
pub fn terrain_files(
    spec: &TerrainSpec,
    items: &[(String, String, String, Vec<u8>)],
    index: &StockIndex,
    report: &mut Vec<String>,
) -> Result<OMap<Take>> {
    use bc2_convert::terrain::{self, Refusal};
    let terr = |r: std::result::Result<Vec<u8>, Refusal>| -> Result<Vec<u8>> {
        r.map_err(|e| match e {
            Refusal::Error(m) => format!("TerrainError: {m}"),
            Refusal::Reference => "TerrainError: cut short or damaged (a read past the data)".to_owned(),
        })
    };
    let mut out = OMap::new();
    for (k, name, t, data) in items {
        let b = match t.as_str() {
            "TerrainHeightfield" => terr(terrain::heightfield(data, false))?,
            "WaterMesh" => terr(terrain::water_mesh(data))?,
            "TerrainMaterialMap" => terr(terrain::material_map(data))?,
            "VisualTerrain" => {
                let vt = terr(terrain::visual_terrain(data))?;
                undergrowth_scales(&vt, &spec.standins, &spec.strips_off, index, report)?
            }
            "VisualWater" => terr(terrain::visual_water(data))?,
            "XenonTexture" => bc2_convert::texture::convert(data).map_err(|e| format!("ConvertError: {e}"))?,
            "Terrain" => data.clone(),
            _ => return Err(format!("{k}: a terrain resource of type {t} - no converter")),
        };
        let p = pjoin(&spec.stage, name);
        write(&p, &b)?;
        let kind = if t == "XenonTexture" { "ITexture" } else { t.as_str() };
        out.insert(
            &name.to_lowercase(),
            Take {
                name: name.clone(),
                kind: Some(kind.to_owned()),
                src: Src::Path(p),
            },
        );
    }
    let mut kinds: Vec<&str> = out.iter().filter_map(|(_, t)| t.kind.as_deref()).collect();
    kinds.sort_unstable();
    report.push(format!(
        "terrain: {} resources converted from BF1943 ({})",
        out.len(),
        kinds.join(", ")
    ));
    Ok(out)
}
