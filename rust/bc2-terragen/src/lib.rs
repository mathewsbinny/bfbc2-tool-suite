//! A new BFBC2 terrain from a seed (`spec/transforms/terrain_gen.md`, TGEN-*): a main height field shaped to one of
//! DICE's five biomes, its material map painted from slope and height, and its water.
//!
//! The library the map editor's Create Level and sculpt tools and `bfbc2 terrain generate` call:
//!
//! | Step | Here |
//! |---|---|
//! | settings: biome, size (1,024 / 2,048), seed, border, water share, objective pads, erosion | [`Settings`], [`Pad`], [`Biome`] |
//! | the height field (pure function of the settings, TGEN-GEN-1) | [`generate()`] -> [`Generated`] |
//! | the material cells from any height field (an edited one too) | [`paint::paint`], [`Biome::materials`] |
//! | the water mesh | [`water::water_mesh`] |
//! | DICE's statistics of a height field (the probe's `shape()`) | [`shape::shape`] |
//! | everything at once, the resources' bytes | [`Terrain::new`], [`Terrain::resources`] |
//!
//! ```
//! use bc2_terragen::{Biome, Settings, Terrain};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let t = Terrain::new(&Settings::new(Biome::Arid, 1024, 7))?;
//! assert_eq!(t.height.raw.len(), 1024 * 1024);
//! assert_eq!(t.cells.len(), 2048 * 2048); // 0.5 m material cells
//! assert_eq!(t.names[0], "defaultmaterial");
//! # Ok(())
//! # }
//! ```

#![allow(
    clippy::many_single_char_names,
    reason = "grid code: i, j along x and z, n the side, h the heights, as the spec's probes name them"
)]
#![allow(
    clippy::cast_precision_loss,
    reason = "grid indices and counts are at most 2048 x 2048 (and seeds' 53 random bits): exact in f64 and f32"
)]

pub mod biome;
pub mod fft;
pub mod generate;
pub mod paint;
pub mod rng;
pub mod shape;
pub mod water;

use bc2_formats::terrain_codec::{CodecError, MainBlock, MaterialMap};

pub use biome::{Biome, Targets};
pub use generate::{GENERATOR_VERSION, GenError, Generated, Pad, Settings, generate};

/// A resource of the terrain bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// The bundle entry's name (`Terrains/<T>/<T>.Heightfield-000.res`).
    pub name: String,
    /// The resource type (`TerrainHeightfield`, `TerrainMaterialMap`, `WaterMesh`, `Terrain`).
    pub type_name: &'static str,
    /// The data.
    pub data: Vec<u8>,
}

impl Resource {
    const fn new(name: String, type_name: &'static str, data: Vec<u8>) -> Self {
        Self { name, type_name, data }
    }
}

/// A generated terrain: the height field, its material cells and names, and the water.
#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    /// What it was generated from.
    pub settings: Settings,
    /// The height field.
    pub height: Generated,
    /// The material map's side in cells (TERR-MM-3: 0.5 m cells, twice the samples).
    pub width: usize,
    /// The material cells, `width` x `width`, rows along z (indices into `names`).
    pub cells: Vec<u8>,
    /// The material names (TGEN-MAT-1).
    pub names: Vec<String>,
}

impl Terrain {
    /// Generates the height field and paints its materials.
    ///
    /// # Errors
    ///
    /// [`GenError`] as [`generate()`].
    pub fn new(settings: &Settings) -> Result<Self, GenError> {
        let height = generate(settings)?;
        let n = height.size as usize;
        let width = 2 * n;
        let cells = paint::paint(&height.heights(), n, 1.0, height.water_y, width);
        Ok(Self {
            settings: settings.clone(),
            height,
            width,
            cells,
            names: settings.biome.materials().iter().map(|s| (*s).to_owned()).collect(),
        })
    }

    /// The water mesh (`<T>.Water.Mesh.res`): a quad at the water level, or none when no ground is under water.
    #[must_use]
    pub fn water_mesh(&self) -> Option<Vec<u8>> {
        self.height
            .water_y
            .map(|y| water::water_mesh(f64::from(self.height.size), y))
    }

    /// The terrain bundle's resources (TGEN-REQ-1, TERR-NAME-1), named for terrain `name`
    /// (`Terrains/<name>/<name>...`): the main block (`.Heightfield-000.res`: the heights, a depth map of 255, the
    /// min / max tree and a lower envelope built under the heights by `bc2_formats::terrain_codec`), the material map
    /// (`-MaterialMap.res`), the water mesh (`.Water.Mesh.res`) when ground lies under water, and the 0-byte
    /// `Terrain` entry (`<name>.res`, TERR-NAME-2); each as (name, resource type, data), in that order.
    ///
    /// # Errors
    ///
    /// The codec's refusal (none for a generated terrain: its shapes are the codec's).
    pub fn resources(&self, name: &str) -> Result<Vec<Resource>, CodecError> {
        let base = format!("Terrains/{name}/{name}");
        let block = MainBlock::new(self.height.bounds(), self.height.raw.clone(), None)?;
        let map = MaterialMap {
            width: self.width as u32,
            cells: self.cells.clone(),
            names: self.names.clone(),
        };
        let mut out = vec![
            Resource::new(
                format!("{base}.Heightfield-000.res"),
                "TerrainHeightfield",
                block.encode(),
            ),
            Resource::new(format!("{base}-MaterialMap.res"), "TerrainMaterialMap", map.encode()),
        ];
        if let Some(w) = self.water_mesh() {
            out.push(Resource::new(format!("{base}.Water.Mesh.res"), "WaterMesh", w));
        }
        out.push(Resource::new(format!("{base}.res"), "Terrain", Vec::new()));
        Ok(out)
    }

    /// DICE's statistics of the generated heights (as the game reads them).
    #[must_use]
    pub fn shape(&self) -> shape::Shape {
        let h: Vec<f64> = self.height.heights().iter().map(|v| f64::from(*v)).collect();
        shape::shape(&h, self.height.size as usize, 1.0, self.height.water_y)
    }
}
