//! How the AI workspace draws a level's navigation data (round GR2: the Navigation Studio's view in the editor): the
//! colourings of the walkable ground, the layers of each file and what goes on the GPU.
//!
//! - [`ColourMode`] and [`classes`]: the ground coloured by island (the main network in `view-nav-main`, every other
//!   island its own hue), by sector, by pitch (detailed 0.2 m zones in `view-nav-extra`), by difference (NavKit's
//!   ground DICE also has, NavKit's extra ground, ladder ground) or by anomaly (the main network, islands of 25 nodes
//!   or more, smaller ones) - the same partition of the cells as the Studio's `cell_colors`.
//! - [`Layer`]: each file's ground, outlines, portals, border portals, route graph, ladder ground and ladder links,
//!   DICE's ground NavKit lacks, the anomaly flags, a reference level's AI layer, the edits and the path test.
//! - [`ground_meshes`]: the cells as quads (two triangles each; on the terrain they follow it corner by corner, other
//!   cells are flat at their height), one mesh per colour, for the renderer's overlay meshes.

use std::collections::HashMap;
use std::sync::Arc;

use bc2_formats::terrain::Heightfield;
use eframe::egui::Color32;

use crate::navdata::{GRID, NavData};
use crate::theme::Palette;

/// Ground cells are drawn this far above the surface (m).
pub const LIFT: f32 = 0.06;

/// What the ground's colour says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColourMode {
    /// Connected parts of the route graph: the main network, every other island its own hue.
    #[default]
    Island,
    /// The 100 m sectors.
    Sector,
    /// Detailed (0.2 m) zones against the normal pitch.
    Pitch,
    /// NavKit's build against DICE's file: the same, extra, missing, ladder ground.
    Difference,
    /// The main network, islands of 25 nodes or more, smaller islands.
    Anomaly,
}

impl ColourMode {
    /// Every mode, in the Studio's order.
    pub const ALL: [Self; 5] = [Self::Island, Self::Sector, Self::Pitch, Self::Difference, Self::Anomaly];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Island => "Island",
            Self::Sector => "Sector",
            Self::Pitch => "Pitch",
            Self::Difference => "Difference",
            Self::Anomaly => "Anomaly",
        }
    }

    /// What it shows, in a line.
    #[must_use]
    pub fn help(self) -> &'static str {
        match self {
            Self::Island => {
                "Connected parts of the route graph: the main network green, every other island its own colour"
            }
            Self::Sector => "The 100 m sectors, each its own colour",
            Self::Pitch => "Detailed zones (DICE's 0.2 m areas) against the normal 0.8 m pitch",
            Self::Difference => {
                "NavKit's ground DICE also has, NavKit's extra ground, ladder ground; DICE's ground NavKit lacks is a \
                 layer of its own"
            }
            Self::Anomaly => "The main network, islands of 25 route nodes or more, islands under 25 nodes",
        }
    }
}

/// Each cell's colour class in a mode (the same cells share a class exactly where the Studio gave them one colour).
/// `same`: per cell, whether the other file has the same ground (difference; `None` with one file); `dice`: the file
/// is DICE's.
#[must_use]
pub fn classes(nav: &NavData, mode: ColourMode, same: Option<&[bool]>, dice: bool) -> Vec<u32> {
    let area = |a: u32| nav.areas.get(a as usize);
    let island_of = |a: u32| area(a).map_or(-1, |x| x.island);
    match mode {
        ColourMode::Island => {
            let top = nav.islands.len().max(1) as i64 - 1;
            nav.cell_area
                .iter()
                .map(|&a| island_of(a).clamp(0, top) as u32)
                .collect()
        }
        ColourMode::Sector => nav.cell_area.iter().map(|&a| area(a).map_or(0, |x| x.sector)).collect(),
        ColourMode::Pitch => nav
            .cell_area
            .iter()
            .map(|&a| u32::from(area(a).is_some_and(crate::navdata::NavArea::detailed)))
            .collect(),
        ColourMode::Anomaly => {
            let top = nav.islands.len().max(1) as i64 - 1;
            nav.cell_area
                .iter()
                .map(|&a| {
                    let isl = island_of(a);
                    let n = nav.islands.get(isl.clamp(0, top) as usize).map_or(0, |i| i.nodes);
                    if isl == 0 {
                        0
                    } else if n >= crate::navdata::SMALL_ISLAND {
                        1
                    } else {
                        2
                    }
                })
                .collect()
        }
        ColourMode::Difference => {
            let Some(same) = same else {
                return vec![0; nav.cell_area.len()];
            };
            let _ = dice;
            nav.cell_area
                .iter()
                .zip(same)
                .map(|(&a, &s)| {
                    if s {
                        0
                    } else if area(a).is_some_and(|x| x.ladder) {
                        2
                    } else {
                        1
                    }
                })
                .collect()
        }
    }
}

/// A class's own hue for the island and sector colourings: golden-ratio steps round the wheel (the Studio's `hue`),
/// light enough to read on the terrain.
#[must_use]
pub fn hue(k: u32) -> Color32 {
    let h = (f64::from(k) * 0.618_034).fract() as f32;
    Color32::from(eframe::egui::ecolor::Hsva::new(h, 0.5, 0.95, 1.0))
}

/// The colour of a class in a mode. `sector_rank`: a sector's place among the file's sectors (the sector colouring).
#[must_use]
#[allow(
    clippy::implicit_hasher,
    clippy::match_same_arms,
    reason = "one colour can stand for two classes of different colourings, as the Studio's did"
)]
pub fn class_colour(p: &Palette, mode: ColourMode, class: u32, dice: bool, sector_rank: &HashMap<u32, u32>) -> Color32 {
    match (mode, class) {
        (ColourMode::Island | ColourMode::Anomaly, 0) => p.view_nav_main,
        (ColourMode::Island, k) => hue(k),
        (ColourMode::Sector, s) => hue(sector_rank.get(&s).copied().unwrap_or(0) * 7 + 3),
        (ColourMode::Pitch, 0) => p.view_nav_normal,
        (ColourMode::Pitch, _) => p.view_nav_extra,
        (ColourMode::Anomaly, 1) => p.view_nav_dimmed,
        (ColourMode::Anomaly, _) => p.view_nav_small,
        (ColourMode::Difference, 0) if dice => p.view_nav_ground_dice,
        (ColourMode::Difference, 0) => p.view_nav_ground,
        (ColourMode::Difference, 1) if dice => p.view_nav_missing,
        (ColourMode::Difference, 1) => p.view_nav_extra,
        (ColourMode::Difference, _) => p.view_nav_ladder,
    }
}

/// Classes numbered by first appearance (the parity test's canonical form: two colourings that split the cells the
/// same way give the same numbers).
#[must_use]
pub fn canonical(classes: &[u32]) -> Vec<u32> {
    let mut seen: HashMap<u32, u32> = HashMap::new();
    classes
        .iter()
        .map(|c| {
            let n = seen.len() as u32;
            *seen.entry(*c).or_insert(n)
        })
        .collect()
}

/// A mesh of the overlay: triangles in world space, one colour, in a layer (the renderer's group, shown and hidden as
/// one). `key` names its contents: the same key is the same mesh (not uploaded again).
#[derive(Debug, Clone)]
pub struct OverlayMesh {
    /// What it is (the same key, the same contents).
    pub key: u64,
    /// Its layer (the renderer's group).
    pub group: String,
    /// Its colour.
    pub colour: Color32,
    /// The vertices.
    pub vertices: Arc<Vec<[f32; 3]>>,
    /// The triangles.
    pub triangles: Arc<Vec<[u32; 3]>>,
}

/// The quads of some cells (`navdata`'s cells by index): each 0.8 m cell two triangles, `lift` above its ground; a
/// cell on the terrain (its height within 5 cm of the height field's) follows the terrain at its corners.
#[must_use]
pub fn cell_quads(
    nav: &NavData,
    cells: &[usize],
    hf: Option<&Heightfield>,
    lift: f32,
) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let h = GRID / 2.0;
    let mut v = Vec::with_capacity(cells.len() * 4);
    let mut t = Vec::with_capacity(cells.len() * 2);
    for &k in cells {
        let [x, z] = nav.cell_xz(k);
        let y = nav.cell_y.get(k).copied().unwrap_or(0.0);
        let on_terrain = hf.is_some_and(|hf| (hf.height(x, z) - y).abs() < 0.05);
        let base = v.len() as u32;
        for (dx, dz) in [(-h, -h), (h, -h), (h, h), (-h, h)] {
            let cy = match hf {
                Some(hf) if on_terrain => hf.height(x + dx, z + dz),
                _ => y,
            };
            v.push([(x + dx) as f32, cy as f32 + lift, (z + dz) as f32]);
        }
        t.push([base, base + 1, base + 2]);
        t.push([base, base + 2, base + 3]);
    }
    (v, t)
}

/// A stable key from some words.
#[must_use]
pub fn key_of(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.bytes().chain([0xff]) {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// The ground of a file coloured by a mode: one mesh per colour class, in the layer `group`. `tag` names the file
/// and its data (part of each mesh's key).
#[allow(
    clippy::too_many_arguments,
    reason = "the colouring's inputs, each a different thing"
)]
#[must_use]
pub fn ground_meshes(
    nav: &NavData,
    tag: &str,
    group: &str,
    mode: ColourMode,
    same: Option<&[bool]>,
    dice: bool,
    hf: Option<&Heightfield>,
    p: &Palette,
) -> Vec<OverlayMesh> {
    let cls = classes(nav, mode, same, dice);
    let mut by: HashMap<u32, Vec<usize>> = HashMap::new();
    for (k, c) in cls.iter().enumerate() {
        by.entry(*c).or_default().push(k);
    }
    let sectors = nav.sectors();
    let rank: HashMap<u32, u32> = sectors.iter().enumerate().map(|(k, s)| (*s, k as u32)).collect();
    let mut keys: Vec<u32> = by.keys().copied().collect();
    keys.sort_unstable();
    keys.into_iter()
        .filter_map(|c| {
            let cells = by.get(&c)?;
            let colour = class_colour(p, mode, c, dice, &rank);
            let (vertices, triangles) = cell_quads(nav, cells, hf, LIFT);
            Some(OverlayMesh {
                key: key_of(&[tag, group, mode.name(), &c.to_string(), &format!("{colour:?}")]),
                group: group.to_owned(),
                colour,
                vertices: Arc::new(vertices),
                triangles: Arc::new(triangles),
            })
        })
        .collect()
}

/// Line segments in world space, each with its colour.
pub type Lines = Vec<([[f32; 3]; 2], Color32)>;

fn seg(a: [f64; 3], b: [f64; 3], drop: f64) -> [[f32; 3]; 2] {
    [
        [a[0] as f32, (a[1] - drop) as f32, a[2] as f32],
        [b[0] as f32, (b[1] - drop) as f32, b[2] as f32],
    ]
}

/// The lines of a file's layer: outlines, portals, border portals (drawn on the ground: the stored 0.95 m less a
/// little), the route graph (0.2 m above the ground) and the ladder links (0.3 m above).
#[must_use]
pub fn file_lines(nav: &NavData, which: FileLayer, colour: Color32) -> Lines {
    use crate::navdata::Y_OFFSET;
    let drop = Y_OFFSET - 0.12;
    let mut out = Vec::new();
    match which {
        FileLayer::Outlines => {
            for a in &nav.areas {
                for l in &a.outline {
                    for w in l.windows(2) {
                        if let [p, q] = w {
                            out.push((seg(p.map(f64::from), q.map(f64::from), drop), colour));
                        }
                    }
                }
            }
        }
        FileLayer::Portals => {
            for a in &nav.areas {
                out.extend(a.portals.iter().map(|(p, q, _)| (seg(*p, *q, drop), colour)));
            }
        }
        FileLayer::Borders => {
            for a in &nav.areas {
                out.extend(a.borders.iter().map(|(p, q)| (seg(*p, *q, drop), colour)));
            }
        }
        FileLayer::Graph | FileLayer::LadderLinks => {
            let ladder = which == FileLayer::LadderLinks;
            let lift = Y_OFFSET - if ladder { 0.3 } else { 0.2 };
            for (e, l) in nav.edges.iter().zip(&nav.ladder_edge) {
                if ladder && !*l {
                    continue;
                }
                let (Some(p), Some(q)) = (nav.node_pos.get(e[0]), nav.node_pos.get(e[1])) else {
                    continue;
                };
                out.push((seg(*p, *q, lift), colour));
            }
        }
        FileLayer::Ground | FileLayer::LadderGround => {}
    }
    out
}

/// A layer of one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileLayer {
    /// Its walkable cells.
    Ground,
    /// Its areas' outlines.
    Outlines,
    /// Portals between areas.
    Portals,
    /// Border portals between sectors.
    Borders,
    /// The route graph.
    Graph,
    /// Ground reached only through a ladder link.
    LadderGround,
    /// The ladder links.
    LadderLinks,
}

impl FileLayer {
    /// Every layer of a file, in the panel's order.
    pub const ALL: [Self; 7] = [
        Self::Ground,
        Self::Outlines,
        Self::Portals,
        Self::Borders,
        Self::Graph,
        Self::LadderGround,
        Self::LadderLinks,
    ];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Ground => "ground",
            Self::Outlines => "outlines",
            Self::Portals => "portals",
            Self::Borders => "border portals",
            Self::Graph => "route graph",
            Self::LadderGround => "ladder ground",
            Self::LadderLinks => "ladder links",
        }
    }

    /// Its colour (the ground's is the colouring's; this is its swatch).
    #[must_use]
    pub fn colour(self, p: &Palette) -> Color32 {
        match self {
            Self::Ground => p.view_nav_main,
            Self::Outlines => p.view_nav_outline,
            Self::Portals => p.view_nav_portal,
            Self::Borders => p.view_nav_border,
            Self::Graph => p.view_nav_graph,
            Self::LadderGround | Self::LadderLinks => p.view_nav_ladder,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::navdata::{Island, NavArea};

    fn area(island: i64, pitch: f64, ladder: bool) -> NavArea {
        NavArea {
            sector: if island == 1 { 7 } else { 3 },
            index: 0,
            tile: (0, 0),
            layer: 0,
            pitch,
            min_y: 0.0,
            max_y: 0.0,
            outline: vec![vec![[0.0, 0.95, 0.0], [1.0, 0.95, 0.0]]],
            portals: Vec::new(),
            borders: Vec::new(),
            island,
            cells: 1,
            ladder,
        }
    }

    #[test]
    fn colourings_split_the_cells_as_the_studio_did() {
        let nav = NavData {
            areas: vec![area(0, 0.8, false), area(1, 0.2, false), area(-1, 0.8, true)],
            cell_ij: vec![[0, 0], [1, 0], [2, 0], [3, 0]],
            cell_y: vec![0.0; 4],
            cell_area: vec![0, 1, 2, 0],
            islands: vec![
                Island {
                    nodes: 30,
                    cells: 2,
                    centre: [0.0; 3],
                },
                Island {
                    nodes: 4,
                    cells: 1,
                    centre: [0.0; 3],
                },
            ],
            ..NavData::default()
        };
        assert_eq!(classes(&nav, ColourMode::Island, None, false), [0, 1, 0, 0]);
        assert_eq!(classes(&nav, ColourMode::Sector, None, false), [3, 7, 3, 3]);
        assert_eq!(classes(&nav, ColourMode::Pitch, None, false), [0, 1, 0, 0]);
        // an area without an island (-1) counts the main network's size: 25 or more, dimmed
        assert_eq!(classes(&nav, ColourMode::Anomaly, None, false), [0, 2, 1, 0]);
        let same = [true, false, false, true];
        assert_eq!(classes(&nav, ColourMode::Difference, Some(&same), false), [0, 1, 2, 0]);
        assert_eq!(classes(&nav, ColourMode::Difference, None, false), [0, 0, 0, 0]);
        assert_eq!(canonical(&[7, 3, 7, 9]), [0, 1, 0, 2]);
        let (v, t) = cell_quads(&nav, &[0, 1], None, 0.5);
        assert_eq!((v.len(), t.len()), (8, 4));
        assert!((v[0][1] - 0.5).abs() < 1e-6 && (v[4][0] - 0.4).abs() < 1e-6);
        assert_ne!(hue(1), hue(2));
        assert_eq!(file_lines(&nav, FileLayer::Outlines, Color32::WHITE).len(), 3);
    }
}
