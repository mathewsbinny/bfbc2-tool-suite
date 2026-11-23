//! The five biomes of DICE's terrains (TGEN-BIOME-1) and the shape each is generated to: the medians of its DICE
//! terrains measured by `spec/probes/terrain_shape.py` (TGEN-SHAPE-1 / -2 / -4, 2026-11-23), and the material names a
//! generated material map gives it (TGEN-MAT-1).

use std::fmt;
use std::str::FromStr;

/// A biome (TGEN-BIOME-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Biome {
    /// Desert and dry levels (`MP_005`, `MP_006`, `SP_006`, `SP_009`, `BC1_Oasis`).
    Arid,
    /// DICE's `jungle` vegetation set.
    Jungle,
    /// DICE's `snow` set.
    Snow,
    /// DICE's `autumn` set.
    Temperate,
    /// DICE's `nam` set.
    Vietnam,
}

/// What a biome's DICE terrains measure (medians over the biome's terrains).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Targets {
    /// The radial power spectrum's slope (TGEN-SHAPE-3).
    pub beta: f64,
    /// Slope percentiles 10, 50, 90, 99 in degrees (TGEN-SHAPE-2).
    pub slope: [f64; 4],
    /// Hypsometric integral (TGEN-SHAPE-1).
    pub hi: f64,
    /// Share of the ground under the water level (TGEN-SHAPE-4).
    pub wet: f64,
    /// Height range in metres (the main block's max y; TGEN-REQ-2).
    pub range_m: f64,
    /// The border ring's median height minus the centre's in metres (TGEN-SHAPE-4).
    pub edge_minus_centre: f64,
    /// RMS relief against a 66 m mean in metres (TGEN-SHAPE-3; reported, not driven).
    pub rough64: f64,
}

impl Biome {
    /// Every biome.
    pub const ALL: [Self; 5] = [Self::Arid, Self::Jungle, Self::Snow, Self::Temperate, Self::Vietnam];

    /// Its name as the spec writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Arid => "arid",
            Self::Jungle => "jungle",
            Self::Snow => "snow",
            Self::Temperate => "temperate",
            Self::Vietnam => "vietnam",
        }
    }

    /// DICE's medians for the biome (`spec/probes/terrain_shape.py`, 2026-11-23).
    #[must_use]
    pub const fn targets(self) -> Targets {
        match self {
            Self::Arid => Targets {
                beta: 4.02,
                slope: [1.2, 8.0, 32.0, 52.8],
                hi: 0.27,
                wet: 0.12,
                range_m: 500.0,
                edge_minus_centre: 35.1,
                rough64: 1.98,
            },
            Self::Jungle => Targets {
                beta: 4.07,
                slope: [0.6, 22.2, 54.9, 75.0],
                hi: 0.41,
                wet: 0.20,
                range_m: 750.0,
                edge_minus_centre: 14.8,
                rough64: 4.67,
            },
            Self::Snow => Targets {
                beta: 4.38,
                slope: [6.55, 30.5, 58.0, 71.1],
                hi: 0.41,
                wet: 0.42,
                range_m: 1000.0,
                edge_minus_centre: 9.75,
                rough64: 4.91,
            },
            Self::Temperate => Targets {
                beta: 3.71,
                slope: [6.2, 26.0, 55.7, 75.1],
                hi: 0.36,
                wet: 0.03,
                range_m: 500.0,
                edge_minus_centre: 37.2,
                rough64: 13.16,
            },
            Self::Vietnam => Targets {
                beta: 3.94,
                slope: [0.0, 14.2, 29.4, 64.6],
                hi: 0.21,
                wet: 0.18,
                range_m: 500.0,
                edge_minus_centre: 6.9,
                rough64: 3.35,
            },
        }
    }

    /// The rough relief's strength (TGEN-GEN-6): temperate's terrains are the rocky ones (their 10 m and 66 m relief
    /// three to four times the other biomes': TGEN-SHAPE-3), so temperate terraces at full strength by default.
    #[must_use]
    pub const fn rough(self) -> f64 {
        match self {
            Self::Temperate => 1.0,
            _ => 0.0,
        }
    }

    /// The material map's names (TGEN-MAT-1, TERR-MM-4): `defaultmaterial` first, then the biome's ground, its
    /// steep material, what lies under the water and its slope material - names DICE's terrains of the biome use
    /// (TGEN-BIOME-2), resolved against the donor level's material grid when the level loads (MAT-GRID-2).
    #[must_use]
    pub const fn materials(self) -> [&'static str; 5] {
        match self {
            Self::Arid => ["defaultmaterial", "sand", "stone", "seabed", "gravel"],
            Self::Jungle => ["defaultmaterial", "grass", "stone", "seabed", "dirt"],
            Self::Snow => ["defaultmaterial", "snow", "stone", "seabed", "gravel"],
            Self::Temperate => ["defaultmaterial", "forest", "rock", "seabed", "gravel"],
            Self::Vietnam => ["defaultmaterial", "grass", "stone", "water_solid", "dirt"],
        }
    }
}

impl fmt::Display for Biome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Biome {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|b| b.name().eq_ignore_ascii_case(s))
            .ok_or_else(|| format!("no biome {s:?} (arid, jungle, snow, temperate, vietnam)"))
    }
}
