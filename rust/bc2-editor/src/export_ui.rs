//! The Export window (`EDITOR_ARCHITECTURE.md` 3.12, "The editor"; round E3b): the project exported as a NEW
//! multiplayer level, staged in OPFOR's `work\mapedit\<level>\stage` and checked offline (`bc2_levelwrite::export`).
//! The window never writes the game copy: installing is Project OPFOR's `mapedit_install.py --apply`, shown with its
//! command.
//!
//! The form:
//! - **the name** (what the launcher, the server browser, the menus and the loading screen show), with a description
//!   and a picture (PNG or JPEG; empty: the base level's loading picture) and a preview of the loading screen and the
//!   map-list picture as the export writes them (round E3b);
//! - the level id and the game mode (Conquest; the other modes need E4's mode definitions);
//! - **the content profile** (BFBC2, BFBC1, Vietnam, WW2) and its **donor** level: both read from the game copy's
//!   stock levels (`bc2_levelwrite::profile`), with the teams (the donor's factions through the profile, `factions`),
//!   kits and common bundles they give;
//! - **the campaign content** kept ([`crate::strip_ui`], a choice in the project's log);
//! - the bases' and flags' places (E3's first guesses; each can take the selected object's place) and the combat
//!   area kept.
//!
//! The settings are kept beside the project as `<project>.export.json` (data only). The checks before an export run
//! as the form changes; Export stays disabled while one fails, with the reason.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use bc2_levelwrite::awards::Rgb;
use bc2_levelwrite::export::{self, ExportSpec, Game, Identity};
use bc2_levelwrite::mpconvert::{Anchor, LayerSpec};
use bc2_levelwrite::profile::{ContentProfile, Donor, Profiles};
use bc2_levelwrite::{MapProject, Profile};
use eframe::egui;

use crate::edit::Doc;
use crate::strip_ui::StripWindow;

/// The modes the export builds now, and the stock donor of each one's layer when the profiles are not read yet.
pub const MODES: [(&str, &str, &str); 1] = [("conquest", "Conquest", "mp_009cq")];

/// What the preview shows: the two pictures as the game will (decoded from what the export writes).
pub struct PreviewData {
    /// The loading screen.
    pub loading: Rgb,
    /// The map-list picture.
    pub level: Rgb,
    /// Each one's loss against what was encoded, dB.
    pub psnr: [f64; 2],
    /// Where the picture came from.
    pub from: String,
    /// What it was made for (picture, donor): a change asks for a new preview.
    pub key: String,
}

struct Preview {
    data: PreviewData,
    tex: [egui::TextureHandle; 2],
}

/// The window's state.
#[derive(Default)]
pub struct ExportWindow {
    /// Shown.
    pub open: bool,
    /// The project file.
    pub project: Option<PathBuf>,
    /// The game folder (OPFOR's `game\`) and OPFOR's folder.
    pub opfor: Option<PathBuf>,
    /// The form.
    pub spec: Option<ExportSpec>,
    /// The last export's lines (report, then the checks).
    pub log: Vec<String>,
    /// The campaign-content checklist.
    pub strip: StripWindow,
    /// The game copy's content profiles for the form's mode (read on a worker).
    pub profiles: Option<Result<Profiles, String>>,
    running: Option<Receiver<Result<Vec<String>, String>>>,
    scanning: Option<Receiver<Result<Profiles, String>>>,
    previewing: Option<Receiver<Result<PreviewData, String>>>,
    preview: Option<Preview>,
    preview_error: Option<String>,
}

/// The settings file beside a project.
#[must_use]
pub fn spec_path(project: &Path) -> PathBuf {
    let mut s = project.as_os_str().to_owned();
    s.push(".export.json");
    PathBuf::from(s)
}

/// The anchors of a donor's layer: its two bases and its first two flags, at `places` (by order; the origin past
/// them).
fn anchors_of(d: &Donor, places: &[[f32; 3]]) -> (Vec<Anchor>, Vec<Anchor>) {
    let (bases, flags) = d.bases_and_flags();
    let at = |k: usize| places.get(k).copied().unwrap_or([0.0; 3]);
    let b = bases
        .iter()
        .take(2)
        .enumerate()
        .map(|(k, c)| Anchor {
            from: c.name.clone(),
            at: at(k),
        })
        .collect();
    let f = flags
        .iter()
        .take(2)
        .enumerate()
        .map(|(k, c)| Anchor {
            from: c.name.clone(),
            at: at(2 + k),
        })
        .collect();
    (b, f)
}

/// A first form for a project: its name as the level's and the map's, its own name and the base's picture,
/// Conquest in the project's profile with the profile's default donor (`MP_009CQ`'s layer until the profiles are
/// read), the bases and flags at the origin (to be placed), no combat area chosen yet.
#[must_use]
pub fn default_spec(project: &MapProject, profiles: Option<&Profiles>) -> ExportSpec {
    let id: String = project
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    let a = |from: &str| Anchor {
        from: from.into(),
        at: [0.0; 3],
    };
    let mut layer = LayerSpec {
        donor: "mp_009cq".into(),
        bases: vec![a("US Base"), a("RUS Base")],
        flags: vec![a("A: Crash Site"), a("B: Hill")],
        combat_area: String::new(),
    };
    let profile = project.base.profile;
    if let Some(d) = profiles
        .and_then(|p| p.get(profile).ok())
        .and_then(ContentProfile::default_donor)
    {
        layer.donor.clone_from(&d.level);
        (layer.bases, layer.flags) = anchors_of(d, &[]);
    }
    ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: format!("MAPEDIT_{}_CQ", id.trim_matches('_')),
        title: format!("{} (Conquest)", project.name),
        mode: "conquest".into(),
        layer,
        profile,
        identity: Some(Identity::default()),
    }
}

/// The form made for another profile: its default donor, the bases and flags renamed to the donor's (their places
/// kept, in order).
pub fn set_profile(spec: &mut ExportSpec, profiles: &Profiles, p: Profile) {
    spec.profile = p;
    if let Some(d) = profiles.get(p).ok().and_then(ContentProfile::default_donor) {
        set_donor(spec, d);
    }
}

/// The form made for another donor: its bases and flags, their places kept in order.
pub fn set_donor(spec: &mut ExportSpec, d: &Donor) {
    let places: Vec<[f32; 3]> = spec
        .layer
        .bases
        .iter()
        .chain(&spec.layer.flags)
        .map(|a| a.at)
        .chain(std::iter::repeat([0.0; 3]))
        .take(4)
        .collect();
    spec.layer.donor.clone_from(&d.level);
    (spec.layer.bases, spec.layer.flags) = anchors_of(d, &places);
}

/// The checks before an export: each failing one's reason (empty: Export can run).
#[must_use]
pub fn checks(spec: &ExportSpec, game: Option<&Game>, saved: bool, profiles: Option<&Profiles>) -> Vec<String> {
    let mut out = Vec::new();
    if !saved {
        out.push("Save the project first (the export builds the saved project)".into());
    }
    match spec.level_id() {
        Err(e) => out.push(e.to_string()),
        Ok(id) => {
            if let Some(g) = game {
                let low = id.to_ascii_lowercase();
                let exists = !g.level_bundles(&low, "level").is_empty();
                if exists && !export::is_custom(g, &low) {
                    out.push(format!("{id} is a level of the game copy: choose another name"));
                }
            }
        }
    }
    if spec.title.trim().is_empty() {
        out.push("Give the map a name".into());
    }
    if let Some(id) = &spec.identity {
        if let Err(e) = export::loc_text("The map name", spec.title.trim(), 64) {
            out.push(e.to_string());
        }
        if !id.description.trim().is_empty()
            && let Err(e) = export::loc_text("The description", id.description.trim(), 400)
        {
            out.push(e.to_string());
        }
        let pic = id.picture.trim();
        if !pic.is_empty() {
            let ext = Path::new(pic)
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase());
            if !matches!(ext.as_deref(), Some("png" | "jpg" | "jpeg")) {
                out.push("The picture must be a PNG or JPEG file".into());
            } else if !Path::new(pic).is_file() {
                out.push(format!("The picture {pic} is not there"));
            }
        }
    }
    if !MODES.iter().any(|m| m.0 == spec.mode) {
        out.push(format!("The {} mode is not built yet (Conquest is)", spec.mode));
    }
    if spec.layer.bases.len() != 2 {
        out.push("Conquest needs two bases, one per team".into());
    }
    if spec.layer.flags.is_empty() {
        out.push("Conquest needs at least one flag".into());
    }
    if spec.layer.combat_area.trim().len() != 36 {
        out.push("Choose the combat area the map keeps (its guid)".into());
    }
    match profiles.map(|p| p.get(spec.profile)) {
        None => out.push("Reading the game copy's content profiles...".into()),
        Some(Err(e)) => out.push(e.to_string()),
        Some(Ok(p)) => match p.donor(&spec.layer.donor) {
            None => out.push(format!(
                "{} is not a {} level of the mode: choose a donor",
                spec.layer.donor,
                spec.profile.word()
            )),
            Some(d) => {
                for a in spec.layer.bases.iter().chain(&spec.layer.flags) {
                    if !d.capture_points.iter().any(|c| c.name == a.from) {
                        out.push(format!("{} has no capture point {:?}", d.level, a.from));
                    }
                }
            }
        },
    }
    if game.is_none() {
        out.push("No game copy found (OPFOR's game folder)".into());
    }
    out
}

/// The teams of a donor in words (`US / RU`), its factions read through the profile (`factions::faction_side`).
#[must_use]
pub fn teams(p: Profile, d: &Donor) -> String {
    let fp = match p {
        Profile::Vietnam => crate::factions::Profile::Vietnam,
        Profile::Ww2 => crate::factions::Profile::Ww2,
        Profile::Bfbc2 | Profile::Bfbc1 => crate::factions::Profile::Bfbc2,
    };
    let side = |f: &str| crate::factions::faction_side(fp, f).map_or_else(|| f.to_owned(), |s| s.0.to_owned());
    format!("{} / {}", side(&d.factions[0]), side(&d.factions[1]))
}

/// The donor's announcer data in words (round E3c): where its banks and trigger sets are, its voice-over bundles.
#[must_use]
pub fn announcer_text(d: &Donor) -> String {
    let a = &d.announcer;
    let own_banks = a
        .partitions
        .iter()
        .filter(|n| n.to_ascii_lowercase().starts_with("sound/vo/"))
        .count();
    let banks = if own_banks == 0 {
        "the common bundles' line banks".to_owned()
    } else {
        format!("{own_banks} line banks of its own")
    };
    let mut targets: Vec<&str> = a
        .blocks
        .iter()
        .flat_map(|(_, t)| t.iter().map(|x| x.rsplit('/').nth(2).unwrap_or(x)))
        .collect();
    targets.dedup();
    let mut seen = std::collections::HashSet::new();
    targets.retain(|t| seen.insert(*t));
    format!(
        "{}'s: {banks}; {} voice-over languages ({}){}",
        d.level.to_ascii_uppercase(),
        a.blocks.len(),
        targets.join(", "),
        if a.bundles.is_empty() {
            String::new()
        } else {
            format!(", {} bundle(s) of its own copied", a.bundles.len())
        }
    )
}

fn to_image(p: &Rgb) -> egui::ColorImage {
    egui::ColorImage::from_rgb([p.w, p.h], &p.px)
}

impl ExportWindow {
    /// Opens the window for a project (its settings read from beside it, else a first form).
    pub fn open_for(&mut self, project: &Path, opfor: Option<&Path>) {
        self.open = true;
        self.project = Some(project.to_path_buf());
        self.opfor = opfor.map(Path::to_path_buf);
        let from_file = std::fs::read_to_string(spec_path(project))
            .ok()
            .and_then(|t| ExportSpec::from_json(&t).ok());
        let profiles = self.profiles.as_ref().and_then(|p| p.as_ref().ok());
        self.spec = from_file.or_else(|| MapProject::load(project).ok().map(|p| default_spec(&p, profiles)));
        if self.profiles.is_none() && self.scanning.is_none() {
            self.scan();
        }
    }

    fn game(&self) -> Option<Game> {
        Game::open(&self.opfor.as_ref()?.join("game")).ok()
    }

    /// Reads the content profiles on a worker.
    fn scan(&mut self) {
        let Some(opfor) = self.opfor.clone() else { return };
        let mode = self.spec.as_ref().map_or_else(|| "conquest".into(), |s| s.mode.clone());
        let (tx, rx) = channel();
        self.scanning = Some(rx);
        std::thread::spawn(move || {
            let r = Game::open(&opfor.join("game"))
                .and_then(|g| bc2_levelwrite::profile::scan(&g, &mode))
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
    }

    /// Makes the preview pictures on a worker (the export's own code: `export::pictures`).
    fn start_preview(&mut self, base: &str) {
        let (Some(opfor), Some(spec)) = (self.opfor.clone(), self.spec.clone()) else {
            return;
        };
        let picture = spec
            .identity
            .as_ref()
            .map_or_else(String::new, |i| i.picture.trim().to_owned());
        let base = base.to_owned();
        let (tx, rx) = channel();
        self.previewing = Some(rx);
        self.preview_error = None;
        std::thread::spawn(move || {
            let run = || -> Result<PreviewData, String> {
                let game = Game::open(&opfor.join("game")).map_err(|e| e.to_string())?;
                let donor = spec.layer.donor.to_ascii_lowercase();
                let dk = export::description_key(&game, &donor).map_err(|e| e.to_string())?;
                let bk = export::description_key(&game, &base).map_err(|e| e.to_string())?;
                let bp = bk.strip_suffix("_NAME").unwrap_or(&bk).to_ascii_lowercase();
                let mut p = export::pictures(&game, &donor, &dk, &bp, &picture).map_err(|e| e.to_string())?;
                let level = p.pop().ok_or("no picture")?;
                let loading = p.pop().ok_or("no picture")?;
                Ok(PreviewData {
                    from: loading.from.clone(),
                    psnr: [loading.made.psnr, level.made.psnr],
                    loading: loading.made.decoded,
                    level: level.made.decoded,
                    key: format!("{picture}|{donor}"),
                })
            };
            let _ = tx.send(run());
        });
    }

    fn start(&mut self) {
        let (Some(project), Some(opfor), Some(spec)) = (self.project.clone(), self.opfor.clone(), self.spec.clone())
        else {
            return;
        };
        if let Ok(json) = serde_json::to_string_pretty(&spec) {
            let _ = std::fs::write(spec_path(&project), json + "\n");
        }
        let (tx, rx) = channel();
        self.running = Some(rx);
        self.log = vec!["Exporting...".into()];
        std::thread::spawn(move || {
            let run = || -> Result<Vec<String>, String> {
                let game = Game::open(&opfor.join("game")).map_err(|e| e.to_string())?;
                let p = MapProject::load(&project).map_err(|e| e.to_string())?;
                let lv = spec.level_id().map_err(|e| e.to_string())?.to_ascii_lowercase();
                let stage = opfor.join("work").join("mapedit").join(&lv).join("stage");
                let rec = export::build(&game, &p, &project.display().to_string(), &spec, &stage)
                    .map_err(|e| e.to_string())?;
                let r = export::check(&game, &stage).map_err(|e| e.to_string())?;
                let mut out = rec.report;
                out.extend(
                    r.checks
                        .iter()
                        .map(|(ok, w)| format!("{} {w}", if *ok { "ok" } else { "FAILED" })),
                );
                out.push(if r.ok() {
                    format!(
                        "Staged in {} - nothing in the game copy changed. To install it (the game closed): python \
                         src\\tools\\mapedit\\mapedit_install.py --apply {}",
                        stage.display(),
                        stage.display()
                    )
                } else {
                    "A check failed: the stage is not ready to install".into()
                });
                Ok(out)
            };
            let _ = tx.send(run());
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.running
            && let Ok(r) = rx.try_recv()
        {
            self.log = match r {
                Ok(lines) => lines,
                Err(e) => vec![format!("Not exported: {e}")],
            };
            self.running = None;
        }
        if let Some(rx) = &self.scanning
            && let Ok(r) = rx.try_recv()
        {
            self.scanning = None;
            // a first form made before the profiles were read takes the profile's default donor
            if let (Ok(p), Some(spec), Some(project)) = (&r, self.spec.as_mut(), self.project.as_ref())
                && !spec_path(project).is_file()
            {
                let profile = spec.profile;
                set_profile(spec, p, profile);
            }
            self.profiles = Some(r);
        }
        if let Some(rx) = &self.previewing
            && let Ok(r) = rx.try_recv()
        {
            self.previewing = None;
            match r {
                Ok(data) => {
                    let opts = egui::TextureOptions::LINEAR;
                    let tex = [
                        ctx.load_texture("export-preview-loading", to_image(&data.loading), opts),
                        ctx.load_texture("export-preview-level", to_image(&data.level), opts),
                    ];
                    self.preview = Some(Preview { data, tex });
                }
                Err(e) => self.preview_error = Some(e),
            }
        }
        if self.running.is_some() || self.scanning.is_some() || self.previewing.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    /// Opens the checklist on a project when it is a campaign level with no choice yet (a project just opened).
    pub fn offer_strip(&mut self, doc: &Doc) {
        self.strip.offer(doc);
    }

    /// Waits for the workers (the headless screenshots): the profiles and the preview read.
    pub fn settle(&mut self, ctx: &egui::Context, base: &str) {
        if let Some(rx) = self.scanning.take() {
            let r = rx.recv().unwrap_or_else(|e| Err(e.to_string()));
            self.profiles = Some(r);
        }
        self.start_preview(base);
        if let Some(rx) = self.previewing.take() {
            let r = rx.recv().unwrap_or_else(|e| Err(e.to_string()));
            let (tx, rx2) = channel();
            let _ = tx.send(r);
            self.previewing = Some(rx2);
            self.poll(ctx);
        }
    }
}

fn anchor_row(ui: &mut egui::Ui, a: &mut Anchor, pick: Option<[f32; 3]>) {
    ui.horizontal(|ui| {
        ui.label(&a.from);
        ui.label("x");
        ui.add(egui::DragValue::new(&mut a.at[0]).speed(1.0));
        ui.label("z");
        ui.add(egui::DragValue::new(&mut a.at[2]).speed(1.0));
        if ui
            .add_enabled(pick.is_some(), egui::Button::new("Use selection"))
            .on_hover_text("Puts it where the selected object stands (move that object in the view first)")
            .on_disabled_hover_text("Select an object in the view: its place is taken")
            .clicked()
            && let Some(p) = pick
        {
            a.at = p;
        }
    });
}

/// Draws the window and the campaign checklist (when open). `doc` is the open project (the checklist commits its
/// choice to it), `selected` its primary selected entity (an anchor can take its place).
#[allow(clippy::too_many_lines, reason = "the form's rows in order")]
pub fn window(ctx: &egui::Context, w: &mut ExportWindow, mut doc: Option<&mut Doc>, selected: Option<usize>) {
    crate::strip_ui::window(ctx, &mut w.strip, doc.as_deref_mut());
    if !w.open {
        return;
    }
    w.poll(ctx);
    let game = w.game();
    let saved = w
        .project
        .as_ref()
        .is_some_and(|p| !bc2_levelwrite::project::autosave_path(p).exists());
    let pick = doc.as_deref().zip(selected).and_then(|(d, k)| {
        let m = d.scene.entities.get(k)?.matrix?;
        Some([m[12] as f32, m[13] as f32, m[14] as f32])
    });
    let strip_text = doc.as_deref().map(|d| {
        let chosen = d.project.strip_chosen();
        (
            d.project.strip().describe(),
            chosen.is_none(),
            d.project.base.level.clone(),
        )
    });
    let mut open = w.open;
    let mut go = false;
    let mut open_strip = false;
    let mut want_preview = false;
    let profiles = match &w.profiles {
        Some(Ok(p)) => Some(p.clone()),
        _ => None,
    };
    let scan_error = match &w.profiles {
        Some(Err(e)) => Some(e.clone()),
        _ => None,
    };
    let busy = w.running.is_some();
    let previewing = w.previewing.is_some();
    egui::Window::new("Export as a multiplayer level")
        .id(egui::Id::new("export-window"))
        .open(&mut open)
        .resizable(true)
        .default_width(660.0)
        .default_height(880.0)
        .show(ctx, |ui| {
            let Some(spec) = w.spec.as_mut() else {
                ui.label("Open a project first");
                return;
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .max_height((ctx.content_rect().height() - 120.0).max(300.0))
                .show(ui, |ui| {
                    ui.heading("Name and picture");
                    egui::Grid::new("export-name").num_columns(2).show(ui, |ui| {
                        ui.label("Map name");
                        ui.text_edit_singleline(&mut spec.title)
                            .on_hover_text("The launcher, the server browser, the menus and the loading screen");
                        ui.end_row();
                        let mut own = spec.identity.is_some();
                        ui.label("");
                        if ui
                            .checkbox(&mut own, "Its own name and picture in the game")
                            .on_hover_text(
                                "Off: the menus and the loading screen show the base level's name and picture",
                            )
                            .changed()
                        {
                            spec.identity = own.then(Identity::default);
                        }
                        ui.end_row();
                        if let Some(id) = spec.identity.as_mut() {
                            ui.label("Description");
                            ui.add(egui::TextEdit::multiline(&mut id.description).desired_rows(2));
                            ui.end_row();
                            ui.label("Picture");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut id.picture)
                                        .hint_text("a PNG or JPEG file; empty: the base level's loading picture")
                                        .desired_width(360.0),
                                );
                                if ui
                                    .add_enabled(!previewing, egui::Button::new("Preview"))
                                    .on_hover_text(
                                        "The loading screen and the map list's picture as the export writes them",
                                    )
                                    .clicked()
                                {
                                    want_preview = true;
                                }
                            });
                            ui.end_row();
                        }
                    });
                    if spec.identity.is_some() {
                        if previewing {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Making the preview...");
                            });
                        }
                        if let Some(e) = &w.preview_error {
                            ui.colored_label(ui.visuals().warn_fg_color, format!("Preview: {e}"));
                        }
                        if let Some(p) = &w.preview {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.image((p.tex[0].id(), egui::vec2(320.0, 180.0)));
                                    ui.weak(format!(
                                        "Loading screen {} x {} ({:.1} dB)",
                                        p.data.loading.w, p.data.loading.h, p.data.psnr[0]
                                    ));
                                });
                                ui.vertical(|ui| {
                                    ui.image((p.tex[1].id(), egui::vec2(256.0, 128.0)));
                                    ui.weak(format!(
                                        "Map list {} x {} ({:.1} dB)",
                                        p.data.level.w, p.data.level.h, p.data.psnr[1]
                                    ));
                                });
                            });
                            ui.weak(format!("From {}", p.data.from));
                        }
                    }
                    ui.separator();
                    ui.heading("Level");
                    egui::Grid::new("export-form").num_columns(2).show(ui, |ui| {
                        ui.label("Level id");
                        ui.text_edit_singleline(&mut spec.level);
                        ui.end_row();
                        ui.label("Game mode");
                        egui::ComboBox::from_id_salt("export-mode")
                            .selected_text(MODES.iter().find(|m| m.0 == spec.mode).map_or("?", |m| m.1))
                            .show_ui(ui, |ui| {
                                for (id, name, donor) in MODES {
                                    if ui.selectable_label(spec.mode == id, name).clicked() {
                                        spec.mode = id.into();
                                        spec.layer.donor = donor.into();
                                    }
                                }
                            });
                        ui.end_row();
                        ui.label("Content profile");
                        let label = |p: Profile| {
                            profiles.as_ref().and_then(|ps| ps.get(p).ok()).map_or_else(
                                || p.word().to_owned(),
                                |s| format!("{} ({} levels)", p.word(), s.levels.len()),
                            )
                        };
                        egui::ComboBox::from_id_salt("export-profile")
                            .selected_text(label(spec.profile))
                            .show_ui(ui, |ui| {
                                for p in Profile::ALL {
                                    let there = profiles.as_ref().is_some_and(|ps| ps.get(p).is_ok());
                                    if ui
                                        .add_enabled(there, egui::Button::selectable(spec.profile == p, label(p)))
                                        .on_disabled_hover_text("The game copy has no level of this set in the mode")
                                        .clicked()
                                        && let Some(ps) = &profiles
                                    {
                                        set_profile(spec, ps, p);
                                    }
                                }
                            });
                        ui.end_row();
                        let set = profiles.as_ref().and_then(|ps| ps.get(spec.profile).ok());
                        ui.label("Donor level");
                        egui::ComboBox::from_id_salt("export-donor")
                            .selected_text(spec.layer.donor.to_ascii_uppercase())
                            .show_ui(ui, |ui| {
                                for d in set.map_or(&[][..], |s| s.donors.as_slice()) {
                                    let mut text = d.level.to_ascii_uppercase();
                                    if !d.outside.is_empty() {
                                        let _ = write!(text, " (+{} partitions of its own)", d.outside.len());
                                    }
                                    if ui.selectable_label(spec.layer.donor == d.level, text).clicked() {
                                        set_donor(spec, d);
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "The stock level whose game-mode layer, kits and description the export takes",
                            );
                        ui.end_row();
                        if let Some(d) = set.and_then(|s| s.donor(&spec.layer.donor)) {
                            ui.label("Teams");
                            ui.label(format!("{} (the donor's factions)", teams(spec.profile, d)));
                            ui.end_row();
                            ui.label("Kits");
                            ui.label(format!(
                                "{} kit pictures; game mode {}",
                                d.kits.len(),
                                d.game_mode.split('/').nth(1).unwrap_or("?")
                            ));
                            ui.end_row();
                            ui.label("Announcer");
                            ui.label(announcer_text(d))
                                .on_hover_text("The announcer's voice-over, as the donor's set has it (round E3c)");
                            ui.end_row();
                        }
                        if let Some(s) = set {
                            ui.label("Common bundles");
                            ui.label(
                                s.commons("level")
                                    .iter()
                                    .map(|t| t.rsplit('/').nth(1).unwrap_or(t))
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            );
                            ui.end_row();
                        }
                        ui.label("Campaign content");
                        ui.horizontal(|ui| {
                            match &strip_text {
                                Some((text, unchosen, _)) => {
                                    ui.label(if *unchosen {
                                        format!("{text} (not chosen yet)")
                                    } else {
                                        text.clone()
                                    });
                                }
                                None => {
                                    ui.label("open the project");
                                }
                            }
                            if ui
                                .add_enabled(strip_text.is_some(), egui::Button::new("Choose..."))
                                .clicked()
                            {
                                open_strip = true;
                            }
                        });
                        ui.end_row();
                        ui.label("Combat area kept");
                        ui.text_edit_singleline(&mut spec.layer.combat_area);
                        ui.end_row();
                    });
                    if let Some(e) = &scan_error {
                        ui.colored_label(ui.visuals().warn_fg_color, format!("Content profiles: {e}"));
                    }
                    ui.separator();
                    ui.label("Bases (first guesses: move them, or take the selected object's place)");
                    for a in &mut spec.layer.bases {
                        anchor_row(ui, a, pick);
                    }
                    ui.label("Flags");
                    for a in &mut spec.layer.flags {
                        anchor_row(ui, a, pick);
                    }
                    ui.separator();
                    let problems = checks(spec, game.as_ref(), saved, profiles.as_ref());
                    for p in &problems {
                        ui.colored_label(ui.visuals().warn_fg_color, p);
                    }
                    let tip = problems
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "Builds and checks the new level; the game copy is not changed".into());
                    if ui
                        .add_enabled(problems.is_empty() && !busy, egui::Button::new("Export"))
                        .on_hover_text(tip)
                        .on_disabled_hover_text(problems.first().cloned().unwrap_or_else(|| "Exporting...".into()))
                        .clicked()
                    {
                        go = true;
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("export-log")
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for l in &w.log {
                                ui.monospace(l);
                            }
                        });
                });
        });
    w.open = open;
    if open_strip && let Some(d) = doc.as_deref() {
        w.strip.open_for(d);
    }
    if want_preview && let Some((_, _, base)) = &strip_text {
        let base = base.clone();
        w.start_preview(&base);
    }
    if go {
        w.start();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use bc2_levelwrite::mpconvert::CapturePoint;
    use bc2_levelwrite::profile::Donor;

    fn donor() -> Donor {
        let cp = |name: &str, base: bool, team: &str| CapturePoint {
            name: name.into(),
            base,
            team: team.into(),
            at: [0.0; 3],
        };
        Donor {
            level: "nam_mp_002cq".into(),
            game_mode: "gamemodes/namconquest/x".into(),
            factions: ["FactionUS".into(), "FactionRUS".into()],
            kits: Vec::new(),
            capture_points: vec![
                cp("B: Camp", false, "TeamNeutral"),
                cp("VC Base", true, "Team2"),
                cp("A: Village", false, "TeamNeutral"),
                cp("US Base", true, "Team1"),
            ],
            outside: Vec::new(),
            fields: std::collections::BTreeMap::new(),
            announcer: bc2_levelwrite::profile::Announcer::default(),
        }
    }

    #[test]
    fn checks_name_mode_and_layer() {
        let p = MapProject::new("Aurora: north", "sp_01", "levels/SP_01", b"x", Profile::Bfbc2);
        let mut s = default_spec(&p, None);
        assert_eq!(s.level, "MAPEDIT_AURORA__NORTH_CQ");
        assert!(s.identity.is_some());
        let c = checks(&s, None, true, None);
        assert!(c.iter().any(|x| x.contains("combat area")));
        assert!(c.iter().any(|x| x.contains("game copy")));
        assert!(c.iter().any(|x| x.contains("content profiles")));
        s.layer.combat_area = "E4C7B1C2-BB9F-481A-B3CC-F67E4E5CB088".into();
        s.level = "bad name".into();
        s.mode = "rush".into();
        s.title = "Op\u{e9}ration".into();
        s.identity = Some(Identity {
            description: String::new(),
            picture: "x.gif".into(),
        });
        let c = checks(&s, None, false, None);
        assert!(c.iter().any(|x| x.starts_with("Save the project")));
        assert!(c.iter().any(|x| x.contains("3 to 40")));
        assert!(c.iter().any(|x| x.contains("not built yet")));
        assert!(c.iter().any(|x| x.contains("plain ASCII")));
        assert!(c.iter().any(|x| x.contains("PNG or JPEG")));
        assert_eq!(
            spec_path(Path::new("a/b.mapedit.json")),
            PathBuf::from("a/b.mapedit.json.export.json")
        );
    }

    #[test]
    fn a_donor_renames_the_anchors_and_keeps_their_places() {
        let p = MapProject::new("A", "sp_01", "levels/SP_01", b"x", Profile::Bfbc2);
        let mut s = default_spec(&p, None);
        s.layer.bases[0].at = [1.0, 0.0, 2.0];
        s.layer.flags[1].at = [5.0, 0.0, 6.0];
        let d = donor();
        set_donor(&mut s, &d);
        assert_eq!(s.layer.donor, "nam_mp_002cq");
        let names: Vec<&str> = s
            .layer
            .bases
            .iter()
            .chain(&s.layer.flags)
            .map(|a| a.from.as_str())
            .collect();
        assert_eq!(names, ["US Base", "VC Base", "A: Village", "B: Camp"]);
        assert_eq!(s.layer.bases[0].at, [1.0, 0.0, 2.0]);
        assert_eq!(s.layer.flags[1].at, [5.0, 0.0, 6.0]);
        assert_eq!(teams(Profile::Vietnam, &d), "US / NVA");
        assert_eq!(teams(Profile::Ww2, &d), "US / JP");
        assert_eq!(teams(Profile::Bfbc1, &d), "US / RU");
    }
}
