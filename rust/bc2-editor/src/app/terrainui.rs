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

    /// A stroke under way: its region redrawn in the simplified view at once.
    pub(super) fn terrain_preview(
        &mut self,
        gpu: &GpuCtx,
        target: &FileTarget,
        region: bc2_formats::terrain_codec::Region,
    ) {
        if !matches!(target, FileTarget::Terrain { .. }) {
            self.message = "Painting the material (let go to keep the stroke)".into();
            return;
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
        self.redraw();
    }

    /// A stroke finished: committed as one operation (the tiles it changed kept beside the project).
    pub(super) fn terrain_stroke(
        &mut self,
        gpu: &GpuCtx,
        palette: &Palette,
        target: &FileTarget,
        raster: &bc2_levelwrite::delta::Raster,
        label: &str,
        clamped: bool,
    ) {
        let Some(d) = self.doc.as_mut() else { return };
        self.message = match d.stroke(target, raster, label) {
            Ok(Some(_)) => format!(
                "{label}: done (Ctrl+Z undoes it){}",
                if clamped {
                    "; the ground was held to the level's height range (TEDIT-HF-2)"
                } else {
                    ""
                }
            ),
            Ok(None) => "The stroke changed nothing".into(),
            Err(e) => format!("Not done: {e}"),
        };
        self.refresh(gpu, palette);
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
        let names = self.view.doc.as_ref().map_or_else(Vec::new, Doc::material_names);
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
                    }
                });
            });
        if let Some(b) = chosen {
            self.view.choose_brush(b);
        }
    }
}
