//! Round E5-rest: the document's terrain layers besides the height field and the material map (spec
//! `transforms/terrain_edit.md`): the visual paint layers ([`Target::Paint`], TEDIT-VT-1), the colour / occlusion map
//! ([`Target::Colour`], TEDIT-OCC-1) and its re-shade after a sculpt, the sub-terrain ring ([`Target::Ring`],
//! TERR-HF-12), the height range widened ([`Op::TerrainRange`], TEDIT-HF-2), a material added to the map from the
//! level's grid ([`Op::MaterialAdd`], MAT-GRID-2) and the visual / material pairing (TEDIT-MM-2).
//!
//! Each is a raster or an operation of the project's log, written by the export (`bc2_levelwrite::export`), so a
//! second tool (round RD1's roads) paints the same layers through [`Doc::stroke_parts`], [`Doc::widen_range`] and
//! [`Doc::add_material`] without the Terrain workspace.

use std::collections::BTreeMap;

use bc2_levelwrite::delta::Raster;
use bc2_levelwrite::files::Target;
use bc2_levelwrite::terrainpaint::{self, LayerModel, Pair, Reshaded};
use bc2_levelwrite::{Op, terrainedit};
use bc2_scene::scene::TerrainExtra;

use crate::edit::Doc;
use crate::error::EditorError;

impl Doc {
    /// The terrain's other resources as the level's files hold them (`None` for a level without them: the demo).
    #[must_use]
    pub fn terrain_extra(&self) -> Option<&TerrainExtra> {
        self.base.terrain_extra.as_deref()
    }

    /// The colour / occlusion map's target, when the level's folder has a DXT1 or L8 map.
    #[must_use]
    pub fn colour_target(&self) -> Option<Target> {
        let (file, bytes) = self.terrain_extra()?.colour.as_ref()?;
        let h = bc2_formats::itexture::Header::read(bytes)?;
        bc2_formats::itexture::Format::from_code(h.format)?;
        Some(Target::Colour { file: file.clone() })
    }

    /// Whether the colour / occlusion map is grey (L8: its raster holds grey levels).
    #[must_use]
    pub fn colour_is_grey(&self) -> bool {
        self.terrain_extra()
            .and_then(|x| x.colour.as_ref())
            .is_some_and(|(_, b)| terrainpaint::colour_is_grey(b))
    }

    /// The sub-terrain ring's target, when the terrain has DICE's single ring of 12 blocks (TERR-HF-12).
    #[must_use]
    pub fn ring_target(&self) -> Option<Target> {
        if self.terrain_extra()?.ring.len() != terrainedit::RING_BLOCKS {
            return None;
        }
        let Some(Target::Terrain { file }) = self.terrain_target() else {
            return None;
        };
        Some(Target::Ring { file })
    }

    /// The terrain's paint layers ([`LayerModel`]); why not, in words, when its patch shaders' names do not give
    /// one kind per layer (TEDIT-VT-2) or the level has no visual terrain.
    ///
    /// # Errors
    /// No visual terrain, or its layers are not one kind each.
    pub fn layer_model(&self) -> Result<LayerModel, String> {
        let v = self
            .terrain_extra()
            .and_then(|x| x.visual.as_deref())
            .ok_or("this level's terrain has no visual terrain to paint")?;
        let vt = bc2_formats::visual_terrain::VisualTerrain::decode(v).map_err(|e| e.to_string())?;
        LayerModel::of(&vt)
    }

    /// The visual layers a paint can lay (the terrain's tiled layers), in DICE's order; empty when none.
    #[must_use]
    pub fn paint_layers(&self) -> Vec<String> {
        self.layer_model()
            .map(|m| m.paintable().into_iter().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// The height range of the main height field: the base's max y and the max y the log leaves, metres.
    #[must_use]
    pub fn terrain_range(&self) -> Option<(f32, f32)> {
        let hf = self.base.heightfield.as_deref()?;
        let base = hf.max[1] - hf.min[1];
        let Some(Target::Terrain { file }) = self.terrain_target() else {
            return Some((base, base));
        };
        Some((base, self.files.ranges.get(&file).map_or(hf.max[1], |r| r[1]) - hf.min[1]))
    }

    /// Widens the main height field's range to `to` metres (TEDIT-HF-2): one operation; every stored height scaled so
    /// the ground stays where it is. `None` when the range is already that wide.
    ///
    /// # Errors
    /// No height field, or a range narrower than the current one.
    pub fn widen_range(&mut self, to: f32) -> Result<Option<u64>, EditorError> {
        let Some(Target::Terrain { file }) = self.terrain_target() else {
            return Err(EditorError::Project("this level has no height field".into()));
        };
        let (_, now) = self.terrain_range().unwrap_or((0.0, 0.0));
        if to <= now + 0.5 {
            return Ok(None);
        }
        let op = Op::TerrainRange { file, from: now, to };
        let label = op.describe();
        Ok(Some(self.commit(op, &label)))
    }

    /// The material names the material brush paints: the map's own, then those the log added.
    #[must_use]
    pub fn material_names(&self) -> Vec<String> {
        let mut out = self
            .base
            .material_map
            .as_ref()
            .map_or_else(Vec::new, |m| m.names.clone());
        if let Some(Target::Material { file }) = self.material_target()
            && let Some(added) = self.files.material_names.get(&file)
        {
            out.extend(added.iter().cloned());
        }
        out
    }

    /// The level grid's terrain materials the map lacks (MAT-GRID-2: the names a material brush may add), read from
    /// the level's `Materials_Win32` partition as the scene read it.
    #[must_use]
    pub fn materials_to_add(&self) -> Vec<String> {
        let have: Vec<String> = self.material_names().iter().map(|n| n.to_ascii_lowercase()).collect();
        let mut out: Vec<String> = Vec::new();
        for n in self.grid_from_game() {
            if !have.contains(&n.to_ascii_lowercase()) && !out.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                out.push(n);
            }
        }
        out
    }

    /// The level grid's terrain materials read from the game copy (when the scene did not read the grid).
    fn grid_from_game(&self) -> Vec<String> {
        let Some(g) = self.game.as_ref() else {
            return Vec::new();
        };
        let lv = self.project.base.level.to_ascii_lowercase();
        bc2_levelwrite::export::base_terrain_file(g, &lv, &format!("levels/{lv}/materials_win32.dbx"))
            .map(|b| terrainedit::grid_terrain_materials(&b))
            .unwrap_or_default()
    }

    /// Adds a material to the terrain's material map (TEDIT-MM-1: one of [`Doc::materials_to_add`]); its index for
    /// the material brush.
    ///
    /// # Errors
    /// No material map, a name the level's grid does not hold, or a full map (16 names).
    pub fn add_material(&mut self, name: &str) -> Result<u8, EditorError> {
        let Some(Target::Material { file }) = self.material_target() else {
            return Err(EditorError::Project("this level's terrain has no material map".into()));
        };
        if let Some(k) = self.material_names().iter().position(|n| n.eq_ignore_ascii_case(name)) {
            return Ok(k as u8);
        }
        if !self.materials_to_add().iter().any(|n| n.eq_ignore_ascii_case(name)) {
            return Err(EditorError::Project(format!(
                "{name} is not a terrain material of this level's grid (MAT-GRID-2)"
            )));
        }
        if self.material_names().len() >= terrainedit::MAX_MATERIALS {
            return Err(EditorError::Project(format!(
                "the material map holds {} names already (TERR-MM-2)",
                terrainedit::MAX_MATERIALS
            )));
        }
        let op = Op::MaterialAdd {
            file,
            name: name.to_owned(),
        };
        let label = op.describe();
        self.commit(op, &label);
        Ok((self.material_names().len() - 1) as u8)
    }

    /// Each paint layer's paired material, measured on the terrain's own data (TEDIT-MM-2); empty when the level has
    /// no visual terrain, atlas or material map.
    #[must_use]
    pub fn pairing(&self) -> BTreeMap<String, Pair> {
        let (Some(x), Some(m), Some(hf)) = (
            self.terrain_extra(),
            self.base.material_map.as_deref(),
            self.base.heightfield.as_deref(),
        ) else {
            return BTreeMap::new();
        };
        let (Some(v), Some(a)) = (x.visual.as_deref(), x.atlas.as_deref()) else {
            return BTreeMap::new();
        };
        terrainpaint::pairing(v, a, hf.size, m).unwrap_or_default()
    }

    /// A stroke over several rasters (round E5-rest: a sculpt reaching the ring, a paint with its paired material):
    /// one operation, one undo step; only the tiles that changed are kept. `None` when nothing changed.
    ///
    /// # Errors
    /// A raster not in this level, or a tile not written.
    pub fn stroke_parts(&mut self, parts: &[(Target, &Raster)], label: &str) -> Result<Option<u64>, EditorError> {
        let mut made: Vec<bc2_levelwrite::ops::StrokePart> = Vec::new();
        for (t, after) in parts {
            let before = match (self.raster(t), t) {
                (Some(r), _) => r,
                (None, Target::Paint { .. }) => Raster::new(after.width(), after.height(), 0),
                (None, other) => {
                    return Err(EditorError::Project(format!(
                        "{} is not in this level",
                        other.describe()
                    )));
                }
            };
            let tiles = bc2_levelwrite::delta::stroke(&before, after, &self.tiles)
                .map_err(|e| EditorError::Project(e.to_string()))?;
            if !tiles.is_empty() {
                made.push(bc2_levelwrite::ops::StrokePart {
                    target: t.clone(),
                    size: [after.width(), after.height()],
                    tiles,
                });
            }
        }
        if made.is_empty() {
            return Ok(None);
        }
        let first = made.remove(0);
        let n = self.commit(
            Op::Stroke {
                target: first.target,
                size: first.size,
                tiles: first.tiles,
                also: made,
            },
            label,
        );
        Ok(Some(n))
    }

    /// The height of the main height field at world (`x`, `z`) as the base has it (`old`) and as the log leaves it,
    /// for [`Doc::reshade`] (the main block's nearest sample; outside it, its nearest edge sample).
    fn ground(&self) -> Option<(Ground, Vec<u16>, Vec<u16>)> {
        let b = self.base.heightfield.as_deref()?;
        let now = self.scene.heightfield.as_deref()?;
        Some((Ground::of(now), b.raw.clone(), now.raw.clone()))
    }

    /// Re-shades the colour / occlusion map over the area the sculpt changed (TEDIT-OCC-1, LIGHT-BAKE-6): DICE's map
    /// scaled texel by texel by the ratio of sky visibility on the sculpted ground to DICE's ground, as one stroke on
    /// the colour raster (painted texels elsewhere kept). `None` when nothing was sculpted or nothing changed.
    ///
    /// # Errors
    /// No colour / occlusion map, or the stroke is not written.
    pub fn reshade(&mut self) -> Result<Option<(u64, Reshaded)>, EditorError> {
        let t = self
            .colour_target()
            .ok_or_else(|| EditorError::Project("this level has no colour / occlusion map to re-shade".into()))?;
        let Some((g, old, new)) = self.ground() else {
            return Ok(None);
        };
        let n = g.n as usize;
        let Some(region) = terrainedit::changed_region(&old, &new, n) else {
            return Ok(None);
        };
        let base_scale = self.base.heightfield.as_deref().map_or(g.scale, |b| {
            (f64::from(b.max[1]) - f64::from(b.min[1])) / 65535.0
        });
        let height = |raw: Vec<u16>, scale: f64| {
            move |x: f64, z: f64| {
                let (gi, gj) = g.grid(x, z);
                let top = (n - 1) as f64;
                let (i, j) = (gi.round().clamp(0.0, top) as usize, gj.round().clamp(0.0, top) as usize);
                g.lo + f64::from(raw.get(j * n + i).copied().unwrap_or(0)) * scale
            }
        };
        let (h_old, h_new) = (height(old, base_scale), height(new, g.scale));
        let (a, b) = (g.world(region.x0 as u32, region.z0 as u32), g.world(region.x1 as u32, region.z1 as u32));
        let base_colour = self
            .terrain_extra()
            .and_then(|x| x.colour.as_ref())
            .and_then(|(_, bytes)| terrainpaint::colour_raster(bytes))
            .ok_or_else(|| EditorError::Project("the colour / occlusion map does not decode".into()))?;
        let grey = self.colour_is_grey();
        let (shaded, done) = terrainpaint::reshade(&base_colour, grey, &h_old, &h_new, [a.0, a.1, b.0, b.1]);
        // the re-shaded texels over the colour raster as the log leaves it (the painted texels elsewhere kept)
        let mut after = self.raster(&t).unwrap_or_else(|| base_colour.clone());
        let mut changed = 0usize;
        for y in 0..shaded.height() {
            for x in 0..shaded.width() {
                let v = shaded.get(x, y);
                if v != base_colour.get(x, y) && after.get(x, y) != v {
                    after.set(x, y, v);
                    changed += 1;
                }
            }
        }
        if changed == 0 {
            return Ok(None);
        }
        let label = format!(
            "Re-shade the sculpted area ({} texels, mean ratio {:.3})",
            done.texels, done.mean_ratio
        );
        Ok(self.stroke_parts(&[(t, &after)], &label)?.map(|n| (n, done)))
    }
}

use crate::sculpt::Ground;
