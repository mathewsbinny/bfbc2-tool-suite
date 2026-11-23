//! Round E5: the Terrain workspace on the views ([`crate::sculpt`]): its toolbar (the brushes, radius, strength,
//! falloff, the noise's seed, the material to paint), a brush's stroke drawn at once while it is made, committed as one
//! operation when the button is let go, and the game-accurate view following the sculpted heights.

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use bc2_levelwrite::files::Target as FileTarget;

use crate::actions::Brush;
use crate::sculpt::{Falloff, Ground};

/// How often the game-accurate view takes a stroke's heights while it is made (round E5-rest).
const LIVE_EVERY: std::time::Duration = std::time::Duration::from_millis(150);

impl ViewState {
    /// A brush chosen (the toolbar, the actions): the Terrain tool with it.
    pub(super) fn choose_brush(&mut self, b: Brush) {
        if let Some(why) = b.coming() {
            self.message = why.into();
            return;
        }
        if b == Brush::Material && self.doc.as_ref().is_none_or(|d| d.material_target().is_none()) {
            self.message = "This level's terrain has no material map to paint".into();
            return;
        }
        // round E5-rest: the visual paint needs the terrain's layers; the shade brush its colour / occlusion map
        if b == Brush::Paint {
            let Some(d) = self.doc.as_ref() else { return };
            match d.layer_model() {
                Err(why) => {
                    self.message = format!("This terrain's visual layers cannot be painted: {why}");
                    return;
                }
                Ok(m) => {
                    let layers = m.paintable();
                    let s = &mut self.tools.terrain.settings;
                    if !layers.iter().any(|l| *l == s.layer) {
                        s.layer = layers.first().map_or_else(String::new, |l| (*l).to_owned());
                        s.pair = None;
                    }
                }
            }
        }
        if b == Brush::Shade && self.doc.as_ref().is_none_or(|d| d.colour_target().is_none()) {
            self.message = "This level has no colour / occlusion map to shade".into();
            return;
        }
        self.tools.terrain.brush = Some(b);
        self.tools.terrain.ramp_from = None;
        self.tool = Tool::Terrain;
        self.tools.choose(Tool::Terrain);
        self.message = format!(
            "{} brush{}",
            b.name(),
            if b == Brush::Ramp {
                ": click where the ramp starts, then where it ends"
            } else {
                ": hold the left button on the ground (Ctrl+Z undoes a stroke)"
            }
        );
    }

    /// A stroke under way: its region redrawn in the simplified view at once, and (round E5-rest) the game-accurate
    /// view following the drag a few times a second.
    pub(super) fn terrain_preview(
        &mut self,
        gpu: &GpuCtx,
        target: &FileTarget,
        region: bc2_formats::terrain_codec::Region,
    ) {
        match target {
            FileTarget::Terrain { .. } => {}
            FileTarget::Material { .. } => {
                self.message = "Painting the material (let go to keep the stroke)".into();
                return;
            }
            FileTarget::Paint { layer } => {
                self.message = format!("Painting {layer} (let go to keep the stroke)");
                return;
            }
            FileTarget::Colour { .. } => {
                self.message = "Shading the colour / occlusion map (let go to keep the stroke)".into();
                return;
            }
            _ => return,
        }
        let Some(d) = self.doc.as_ref() else { return };
        let Some(hf) = d.scene.heightfield.as_deref() else {
            return;
        };
        let g = Ground::of(hf);
        if let Some(w) = self.tools.terrain.stroke.as_ref() {
            self.gpu_scene
                .preview_terrain(&gpu.gpu, &d.scene, &w.raster, &g, region);
        }
        // the game-accurate view takes the stroke's heights while it is made (at most every LIVE_EVERY)
        let due = self
            .tools
            .terrain
            .live_at
            .is_none_or(|t| t.elapsed() >= LIVE_EVERY);
        if due
            && let (Some(game), Some(w)) = (self.game_view.game.as_mut(), self.tools.terrain.stroke.as_ref())
            && game.set_terrain_heights(&w.raster.samples())
        {
            self.tools.terrain.live_at = Some(Instant::now());
        }
        self.redraw();
    }

    /// A stroke finished: committed as one operation (the tiles it changed kept beside the project; round E5-rest: the
    /// ring or the paired material it reached in the same operation).
    pub(super) fn terrain_stroke(
        &mut self,
        gpu: &GpuCtx,
        palette: &Palette,
        parts: &[(FileTarget, bc2_levelwrite::delta::Raster)],
        label: &str,
        clamped: bool,
    ) {
        let Some(d) = self.doc.as_mut() else { return };
        let refs: Vec<(FileTarget, &bc2_levelwrite::delta::Raster)> =
            parts.iter().map(|(t, r)| (t.clone(), r)).collect();
        self.message = match d.stroke_parts(&refs, label) {
            Ok(Some(_)) => format!(
                "{label}: done (Ctrl+Z undoes it){}",
                if clamped {
                    "; the ground was held to the level's height range (TEDIT-HF-2: Widen range raises it)"
                } else {
                    ""
                }
            ),
            Ok(None) => "The stroke changed nothing".into(),
            Err(e) => format!("Not done: {e}"),
        };
        self.refresh(gpu, palette);
    }

    /// A Terrain toolbar request (round E5-rest): a material added, the pairing turned on or off, the re-shade, the
    /// range widened.
    pub(super) fn terrain_ask(&mut self, a: TerrainAsk) {
        let Some(d) = self.doc.as_mut() else { return };
        self.message = match a {
            TerrainAsk::AddMaterial(name) => match d.add_material(&name) {
                Ok(k) => {
                    self.tools.terrain.settings.material = k;
                    format!("Material {name} added to the map (Ctrl+Z undoes it)")
                }
                Err(e) => format!("Not added: {e}"),
            },
            TerrainAsk::Pair(on) => {
                let layer = self.tools.terrain.settings.layer.clone();
                if on {
                    let pairs = d.pairing();
                    match pairs.get(&layer) {
                        Some(p) => match d.material_names().iter().position(|n| n.eq_ignore_ascii_case(&p.material)) {
                            Some(k) => {
                                self.tools.terrain.settings.pair = Some(k as u8);
                                format!(
                                    "{layer} paints {} under it ({:.0} % of DICE's cells under {layer} carry it)",
                                    p.material,
                                    p.share * 100.0
                                )
                            }
                            None => format!("{layer}'s material {} is not in the map", p.material),
                        },
                        None => format!("{layer} has no material measured under it"),
                    }
                } else {
                    self.tools.terrain.settings.pair = None;
                    format!("{layer} paints the visual layer only")
                }
            }
            TerrainAsk::Reshade => match d.reshade() {
                Ok(Some((_, r))) => format!(
                    "Re-shaded {} texels of the colour / occlusion map (mean ratio {:.3}; Ctrl+Z undoes it)",
                    r.texels, r.mean_ratio
                ),
                Ok(None) => "Nothing sculpted to re-shade".into(),
                Err(e) => format!("Not re-shaded: {e}"),
            },
            TerrainAsk::Widen(to) => match d.widen_range(to) {
                Ok(Some(_)) => format!("The height range is {to:.0} m now (Ctrl+Z undoes it)"),
                Ok(None) => "The range is that wide already".into(),
                Err(e) => format!("Not widened: {e}"),
            },
        };
        self.sync_game_terrain();
        self.redraw();
    }

    /// The game-accurate view's terrain follows the document's (a stroke, an undo).
    pub(super) fn sync_game_terrain(&mut self) {
        let Some(raw) = self
            .doc
            .as_ref()
            .and_then(|d| d.scene.heightfield.as_ref())
            .map(|h| h.raw.clone())
        else {
            return;
        };
        if let Some(g) = self.game_view.game.as_mut()
            && g.set_terrain_heights(&raw)
        {
            self.redraw();
        }
    }
}

/// A Terrain toolbar request (round E5-rest).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum TerrainAsk {
    /// A material of the level's grid added to the map.
    AddMaterial(String),
    /// The paint brush's paired material on or off.
    Pair(bool),
    /// The colour / occlusion map re-shaded over the sculpt.
    Reshade,
    /// The height range widened to this many metres.
    Widen(f32),
}

impl Tabs<'_> {
    /// The Terrain workspace's toolbar over a view: the brushes, and the chosen one's settings.
    pub(super) fn terrain_bar(&mut self, ui: &mut egui::Ui, rect: Rect, p: &Palette) {
        let bar = Rect::from_min_size(rect.min + Vec2::splat(6.0), Vec2::new(rect.width() - 12.0, 66.0));
        let mut ui2 = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar)
                .layout(Layout::top_down(Align::Min)),
        );
        let mut chosen = None;
        let mut asked: Option<TerrainAsk> = None;
        let names = self.view.doc.as_ref().map_or_else(Vec::new, Doc::material_names);
        let to_add = if self.view.tools.terrain.brush == Some(Brush::Material) {
            self.view.doc.as_ref().map_or_else(Vec::new, Doc::materials_to_add)
        } else {
            Vec::new()
        };
        let layers = if self.view.tools.terrain.brush == Some(Brush::Paint) {
            self.view.doc.as_ref().map_or_else(Vec::new, Doc::paint_layers)
        } else {
            Vec::new()
        };
        let range = self.view.doc.as_ref().and_then(Doc::terrain_range);
        egui::Frame::new()
            .fill(p.surface_2.gamma_multiply(0.94))
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(6, 4))
            .show(&mut ui2, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Terrain").color(p.ink));
                    ui.separator();
                    let t = &mut self.view.tools.terrain;
                    for b in Brush::ALL.iter().filter(|b| b.workspace() == Workspace::Terrain) {
                        let on = t.brush == Some(*b) && self.view.tool == Tool::Terrain;
                        match b.coming() {
                            Some(why) => {
                                ui.add_enabled(false, egui::Button::new(b.name()))
                                    .on_disabled_hover_text(why);
                            }
                            None => {
                                if ui.add(egui::Button::new(b.name()).selected(on)).clicked() {
                                    chosen = Some(*b);
                                }
                            }
                        }
                    }
                });
                // the chosen brush's settings on a row of their own (a narrow view keeps them all)
                ui.horizontal(|ui| {
                    let t = &mut self.view.tools.terrain;
                    let s = &mut t.settings;
                    ui.label("Radius");
                    ui.add(
                        egui::DragValue::new(&mut s.radius)
                            .range(1.0..=200.0)
                            .suffix(" m")
                            .speed(0.5),
                    );
                    ui.label("Strength");
                    ui.add(egui::DragValue::new(&mut s.strength).range(0.01..=1.0).speed(0.01));
                    egui::ComboBox::from_id_salt("terrain-falloff")
                        .selected_text(s.falloff.name())
                        .show_ui(ui, |ui| {
                            for f in Falloff::ALL {
                                ui.selectable_value(&mut s.falloff, f, f.name());
                            }
                        });
                    if t.brush == Some(Brush::Noise) {
                        ui.label("Seed");
                        ui.add(egui::DragValue::new(&mut s.seed));
                    }
                    if t.brush == Some(Brush::Material) && !names.is_empty() {
                        egui::ComboBox::from_id_salt("terrain-material")
                            .selected_text(names.get(usize::from(s.material)).map_or("", String::as_str))
                            .show_ui(ui, |ui| {
                                for (k, n) in names.iter().enumerate() {
                                    ui.selectable_value(&mut s.material, k as u8, n);
                                }
                            });
                        // round E5-rest: a terrain material of the level's grid the map lacks (MAT-GRID-2)
                        if !to_add.is_empty() {
                            egui::ComboBox::from_id_salt("terrain-material-add")
                                .selected_text("Add material")
                                .show_ui(ui, |ui| {
                                    for n in &to_add {
                                        if ui.selectable_label(false, n).clicked() {
                                            asked = Some(TerrainAsk::AddMaterial(n.clone()));
                                        }
                                    }
                                });
                        }
                    }
                    // round E5-rest: the visual layer, and its paired material (TEDIT-MM-2)
                    if t.brush == Some(Brush::Paint) && !layers.is_empty() {
                        egui::ComboBox::from_id_salt("terrain-layer")
                            .selected_text(s.layer.clone())
                            .show_ui(ui, |ui| {
                                for l in &layers {
                                    ui.selectable_value(&mut s.layer, l.clone(), l);
                                }
                            });
                        let mut pair = s.pair.is_some();
                        if ui
                            .checkbox(&mut pair, "Paired material")
                            .on_hover_text(
                                "Paint the material DICE's own cells carry under this layer (measured on this terrain)",
                            )
                            .changed()
                        {
                            asked = Some(TerrainAsk::Pair(pair));
                        }
                    }
                    if t.brush == Some(Brush::Shade) {
                        ui.checkbox(&mut s.lighten, "Lighten");
                    }
                    if matches!(
                        t.brush,
                        Some(Brush::Raise | Brush::Lower | Brush::Smooth | Brush::Flatten | Brush::Noise | Brush::Ramp)
                    ) {
                        if ui
                            .button("Re-shade sculpted area")
                            .on_hover_text(
                                "Scales the colour / occlusion map under the sculpt by the change in sky visibility \
                                 (TEDIT-OCC-1)",
                            )
                            .clicked()
                        {
                            asked = Some(TerrainAsk::Reshade);
                        }
                        if let Some((_, now)) = range
                            && ui
                                .button(format!("Widen range ({now:.0} m)"))
                                .on_hover_text("Raises the height range by a quarter (TEDIT-HF-2): the ground stays")
                                .clicked()
                        {
                            asked = Some(TerrainAsk::Widen((now * 1.25).ceil()));
                        }
                    }
                });
            });
        if let Some(b) = chosen {
            self.view.choose_brush(b);
        }
        if let Some(a) = asked {
            self.view.terrain_ask(a);
        }
    }
}
