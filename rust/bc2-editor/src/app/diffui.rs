//! The Diff panel, the differences in the 3D view and the export preview (round EX1; `EDITOR_ARCHITECTURE.md` 9.5
//! and 3.12 "Preview").
//!
//! - **Compare with base**: the project (B) against its base level (A), LevelDiff's comparison on a worker
//!   ([`crate::compare`]); again by itself while the Diff panel shows it and the project changes.
//! - **Compare with level**: the open level or project (B) against another level of the game copy (A).
//! - **Preview export**: the staged export opened read only as a level (its stage folder in front of the bundles its
//!   registration names) beside the original base level, with the same camera; T switches between them, Close
//!   preview gives the project back. Both views draw the differences.
//!
//! In the 3D view the objects of the side shown are tinted by kind (`view-diff-added`, `-removed`, `-moved`,
//! `-changed`; never orange, which is the selection), and a box marks where an object of the other side stands (a
//! removed object on B, an added one on A, a moved object's old place) with a line from a moved object's old place to
//! its new one. The panel lists the differences with filters (kind, group, text, placed objects only); a click flies
//! to it and selects it, and its fields are listed as LevelDiff prints them (Inspect opens Properties > All fields).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use bc2_levelwrite::export::{self, StageRecord};
use bc2_scene::scene::{self, Scene};
use eframe::egui::{self, Color32, Margin, RichText, Sense, Vec2};

use super::{EditorApp, GpuCtx, Mode, Tabs, Then, ViewState};
use crate::actions::{Action, PanelKind};
use crate::camera;
use crate::compare::{self, Against, DiffView, Group, Kind, Side, SideOf};
use crate::render::Look;
use crate::theme::Palette;

/// A level as the scene loader gives it: the scene, its notes, the seconds it took, its bundles' folders.
type LoadedOk = (Scene, Vec<String>, f64, Vec<PathBuf>);

/// The export preview: the side not shown, the project's view to give back, the stage.
pub struct Preview {
    /// The side not shown now.
    pub other: Box<ViewState>,
    /// The project's view, given back by Close preview.
    pub project: Option<Box<ViewState>>,
    /// The stage folder.
    pub stage: PathBuf,
    /// Its record.
    pub rec: StageRecord,
}

/// A preview being opened: the stage prepared and loaded, the base loaded, the two compared, on a worker.
pub struct PreviewJob {
    rx: Receiver<Result<Box<PreviewLoaded>, String>>,
    /// Since when.
    pub since: Instant,
    /// The stage.
    pub stage: PathBuf,
}

/// What the preview's worker gives.
pub struct PreviewLoaded {
    base_lv: String,
    base: LoadedOk,
    stage_lv: String,
    stage: LoadedOk,
    rec: StageRecord,
    diff: Result<DiffView, String>,
}

/// The colour of a kind of difference (the design's `view-diff-*` tokens).
#[must_use]
pub fn kind_colour(p: &Palette, k: Kind) -> Color32 {
    match k {
        Kind::Added => p.view_diff_added,
        Kind::Removed => p.view_diff_removed,
        Kind::Moved => p.view_diff_moved,
        Kind::Changed => p.view_diff_changed,
    }
}

/// What the preview says about the export besides its objects: its era, mode and donor (the kits, factions and
/// announcer are the donor's, not objects of the level), the campaign content it stripped, what its closure copied.
fn preview_note(rec: &StageRecord) -> String {
    let mut froms: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for c in &rec.closure {
        let b = c.from.replace('\\', "/");
        let short = b.split('/').rev().nth(1).unwrap_or(b.as_str()).to_owned();
        *froms.entry(short).or_default() += 1;
    }
    let copied = if froms.is_empty() {
        "nothing copied from other sets".to_owned()
    } else {
        let mut v: Vec<(String, usize)> = froms.into_iter().collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        format!(
            "{} partitions copied from other sets ({})",
            rec.closure.len(),
            v.iter()
                .take(4)
                .map(|(b, n)| format!("{b} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "The export: {} {} from {}'s layer, {} kits, teams and announcer (the donor's, not objects of the level); \
         {copied}; campaign content: {} instances stripped.",
        rec.profile.word(),
        rec.mode,
        rec.donor.to_ascii_uppercase(),
        rec.profile.word(),
        rec.converted.removed.iter().map(|x| x.1).sum::<usize>()
    )
}

/// Loads the stage (prepared like a Package level, into the editor's cache) and the base, and compares them.
fn load_preview(root: &Path, stage: &Path) -> Result<Box<PreviewLoaded>, String> {
    let game = root.join("game");
    let (level, rec) = compare::stage_level(&game, stage)?;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    crate::package::prepare(root, &level, &mut |_| {}, &cancel)?;
    let t = Instant::now();
    let found = crate::package::load_options(root, &level)?;
    let s = scene::load(&found.options).map_err(|e| format!("the export could not be read: {e}"))?;
    let mut notes = found.notes;
    notes.extend(s.notes.iter().cloned());
    let stage_loaded = (s, notes, t.elapsed().as_secs_f64(), found.options.bases);
    let base = super::load(root, &rec.base)?;
    let a = base.0.file(0).map_or_default(|(_, b)| b.to_vec());
    let b = stage_loaded.0.file(0).map_or_default(|(_, b)| b.to_vec());
    let diff = DiffView::compute(
        &a,
        &b,
        &format!("{} (the original)", rec.base.to_ascii_uppercase()),
        &format!("{} (the export)", rec.level),
        &compare::closure_of(&rec),
    );
    Ok(Box::new(PreviewLoaded {
        base_lv: rec.base.clone(),
        base,
        stage_lv: level.lv.clone(),
        stage: stage_loaded,
        rec,
        diff,
    }))
}

impl EditorApp {
    /// Why a compare action cannot run now (`None`: it can, or it is not one).
    #[must_use]
    pub(super) fn compare_why_not(&self, action: Action) -> Option<&'static str> {
        match action {
            Action::PreviewExport if self.opfor.is_err() => Some("No game copy found"),
            Action::PreviewExport if self.last_stage().is_none() && self.preview.is_none() => {
                Some("Export the level first: the preview opens its stage")
            }
            Action::PreviewExport if self.preview_job.is_some() => Some("The preview is being opened"),
            Action::SwitchCompare | Action::ClosePreview if self.preview.is_none() => {
                Some("Open the export's preview first (Preview export)")
            }
            _ => None,
        }
    }

    /// The stage of the open project's last export (its folder, when it holds `stage.json`).
    #[must_use]
    pub fn last_stage(&self) -> Option<PathBuf> {
        let root = self.opfor.as_ref().ok()?;
        let lv = self.export.spec.as_ref()?.level_id().ok()?;
        let stage = root
            .join("work")
            .join("mapedit")
            .join(lv.to_ascii_lowercase())
            .join("stage");
        stage.join("stage.json").is_file().then_some(stage)
    }

    /// The compare actions; `false` for actions that are not theirs.
    pub(super) fn compare_action(&mut self, gpu: &GpuCtx, action: Action) -> bool {
        let palette = self.palette();
        match action {
            Action::PreviewExport => {
                if let Some(s) = self.last_stage() {
                    self.start_preview(&s);
                }
            }
            Action::SwitchCompare => self.switch_preview(),
            Action::ClosePreview => self.close_preview(),
            Action::CompareWithBase => {
                if let Mode::View(v) = &mut self.mode {
                    v.compare_with_base();
                }
                self.show_diff_panel();
            }
            Action::ToggleDiffMarks => {
                if let Mode::View(v) = &mut self.mode {
                    v.diff.show = !v.diff.show;
                    v.apply_look(&palette);
                    v.refresh(gpu, &palette);
                }
            }
            _ => return false,
        }
        true
    }

    fn show_diff_panel(&mut self) {
        let tab = super::Tab::Diff;
        if let Some(path) = self.dock.find_tab(&tab) {
            let _ = self.dock.set_active_tab(path);
        } else {
            self.dock.push_to_focused_leaf(tab);
        }
    }

    /// Opens an export's preview (its stage beside the original base level) on a worker.
    pub fn start_preview(&mut self, stage: &Path) {
        let Ok(root) = self.opfor.clone() else { return };
        let (tx, rx) = std::sync::mpsc::channel();
        let st = stage.to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(load_preview(&root, &st));
        });
        self.preview_job = Some(PreviewJob {
            rx,
            since: Instant::now(),
            stage: stage.to_path_buf(),
        });
        if let Mode::View(v) = &mut self.mode {
            v.message = "Opening the export's preview: the stage read like a level, beside the original...".into();
        }
    }

    /// The preview's worker, when it is done: the views shown (the export first), the project kept aside.
    pub(super) fn poll_preview(&mut self, gpu: &GpuCtx, ctx: &egui::Context) {
        let Some(job) = &self.preview_job else { return };
        let r = match job.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            Err(TryRecvError::Disconnected) => Err("the preview's worker stopped unexpectedly".into()),
        };
        let stage = job.stage.clone();
        self.preview_job = None;
        match r {
            Ok(loaded) => self.show_preview(gpu, *loaded, stage),
            Err(e) => {
                if let Mode::View(v) = &mut self.mode {
                    v.message = format!("The preview did not open: {e}");
                }
                self.export.log.push(format!("Preview: {e}"));
            }
        }
    }

    /// Opens an export's preview and waits for it (headless runs).
    ///
    /// # Errors
    /// No game copy, or the stage or the base does not load.
    pub fn open_preview_blocking(&mut self, gpu: &GpuCtx, stage: &Path) -> Result<(), String> {
        let root = self.opfor.clone()?;
        let loaded = load_preview(&root, stage)?;
        self.show_preview(gpu, *loaded, stage.to_path_buf());
        let palette = self.palette();
        if let Mode::View(v) = &mut self.mode {
            v.take_visuals(gpu, &palette, true);
        }
        if let Some(p) = self.preview.as_mut() {
            p.other.take_visuals(gpu, &palette, true);
        }
        Ok(())
    }

    fn show_preview(&mut self, gpu: &GpuCtx, l: PreviewLoaded, stage: PathBuf) {
        let palette = self.palette();
        // the project's view kept aside (an earlier preview's sides dropped)
        let project = match self.preview.take() {
            Some(p) => p.project,
            None => match std::mem::replace(&mut self.mode, Mode::Start) {
                Mode::View(v) => Some(v),
                other => {
                    self.mode = other;
                    None
                }
            },
        };
        let camera = project
            .as_ref()
            .and_then(|v| v.viewports.get(&camera::Projection::Perspective).map(|vp| vp.camera));
        let diff = l.diff.map(Arc::new);
        let note = preview_note(&l.rec);
        let side = |app: &mut Self, lv: &str, loaded: LoadedOk, s: Side| -> Option<Box<ViewState>> {
            app.show_scene(gpu, lv, loaded, Then::View);
            let Mode::View(mut v) = std::mem::replace(&mut app.mode, Mode::Start) else {
                return None;
            };
            if let Some(c) = camera
                && let Some(vp) = v.viewports.get_mut(&camera::Projection::Perspective)
            {
                vp.camera = c;
                vp.dirty = true;
            }
            v.diff.view = Some(diff.clone());
            v.diff.side = SideOf(s);
            v.diff.against = Against::Stage;
            v.diff.show = true;
            v.diff.note.clone_from(&note);
            v.apply_look(&palette);
            v.refresh(gpu, &palette);
            Some(v)
        };
        let Some(base) = side(self, &l.base_lv, l.base, Side::A) else {
            return;
        };
        let Some(mut shown) = side(self, &l.stage_lv, l.stage, Side::B) else {
            return;
        };
        shown.message = format!(
            "Preview: the export {} (B). T shows the original {} (A); Close preview goes back to the project.",
            l.rec.level,
            l.base_lv.to_ascii_uppercase()
        );
        self.mode = Mode::View(shown);
        self.preview = Some(Box::new(Preview {
            other: base,
            project,
            stage,
            rec: l.rec,
        }));
        self.show_diff_panel();
    }

    /// Shows the preview's other side, with this side's camera and filters.
    pub fn switch_preview(&mut self) {
        let Some(p) = self.preview.as_mut() else { return };
        let Mode::View(v) = &mut self.mode else { return };
        for (proj, vp) in &v.viewports {
            if let Some(o) = p.other.viewports.get_mut(proj) {
                o.camera = vp.camera;
                o.dirty = true;
            }
        }
        p.other.diff.filters = v.diff.filters.clone();
        p.other.diff.show = v.diff.show;
        std::mem::swap(v, &mut p.other);
        let which = match v.diff.side.0 {
            Side::A => format!("the original {} (A)", v.lv.to_ascii_uppercase()),
            Side::B => format!("the export {} (B)", p.rec.level),
        };
        v.message = format!("Preview: {which}. T switches; Close preview goes back to the project.");
        v.redraw();
    }

    /// Closes the preview: the project's view again.
    pub fn close_preview(&mut self) {
        if let Some(p) = self.preview.take()
            && let Some(project) = p.project
        {
            self.mode = Mode::View(project);
        }
    }

    /// The stage the preview shows, or the one being opened.
    #[must_use]
    pub fn preview_stage(&self) -> Option<&Path> {
        self.preview
            .as_ref()
            .map(|p| p.stage.as_path())
            .or_else(|| self.preview_job.as_ref().map(|j| j.stage.as_path()))
    }

    /// How long the preview has been opening, seconds (`None`: it is not).
    #[must_use]
    pub fn preview_opening(&self) -> Option<f64> {
        self.preview_job.as_ref().map(|j| j.since.elapsed().as_secs_f64())
    }

    /// Which side the preview shows (`None`: no preview).
    #[must_use]
    pub fn preview_side(&self) -> Option<Side> {
        self.preview.as_ref()?;
        self.view().map(|v| v.diff.side.0)
    }
}

impl ViewState {
    /// Compares the project (B) with its base level (A) on a worker.
    pub fn compare_with_base(&mut self) {
        let Some(d) = &self.doc else { return };
        let (Some(a), Some(b)) = (d.base_bytes().map(<[u8]>::to_vec), d.partition()) else {
            self.diff.view = Some(Err("This scene has no level partition to compare".into()));
            return;
        };
        let labels = [
            format!("{} (the base)", d.project.base.level.to_ascii_uppercase()),
            format!("{} (the project)", d.project.name),
        ];
        self.diff.revision = d.revision;
        self.diff.against = Against::Base;
        self.diff.side = SideOf(Side::B);
        self.diff.start(a, b, labels, std::collections::HashSet::new());
    }

    /// Compares the open level or project (B) with another level of the game copy (A) on a worker.
    pub fn compare_with_level(&mut self, other: &str) {
        let other = other.trim().to_ascii_lowercase();
        let Some(root) = self.root.clone() else {
            self.diff.view = Some(Err("No game copy found".into()));
            return;
        };
        let a = export::Game::open(&root.join("game"))
            .and_then(|g| export::level_partition(&g, &other))
            .map_err(|e| format!("{}: {e}", other.to_ascii_uppercase()));
        let b = match &self.doc {
            Some(d) => d.partition(),
            None => self.base.file(0).map(|(_, b)| b.to_vec()),
        };
        match (a, b) {
            (Ok(a), Some(b)) => {
                let labels = [other.to_ascii_uppercase(), self.lv.to_ascii_uppercase()];
                self.diff.against = Against::Level(other);
                self.diff.side = SideOf(Side::B);
                self.diff.revision = self.doc.as_ref().map_or(0, |d| d.revision);
                self.diff.start(a, b, labels, std::collections::HashSet::new());
            }
            (Err(e), _) => self.diff.view = Some(Err(e)),
            (_, None) => self.diff.view = Some(Err("This scene has no level partition to compare".into())),
        }
    }

    /// The comparison's answer, when it came; a project compared with its base compares again after an edit.
    pub(super) fn diff_poll(&mut self, gpu: &GpuCtx, palette: &Palette) {
        if self.diff.poll() {
            self.apply_look(palette);
            self.refresh(gpu, palette);
        }
        let stale = self.diff.against == Against::Base
            && self.diff.job.is_none()
            && self.doc.as_ref().is_some_and(|d| d.revision != self.diff.revision);
        if stale {
            self.compare_with_base();
        }
    }

    /// Waits for a comparison and shows it (headless runs).
    pub fn diff_settle(&mut self, gpu: &GpuCtx, palette: &Palette) {
        self.diff.settle();
        self.apply_look(palette);
        self.refresh(gpu, palette);
    }

    /// The differences drawn in the 3D view: tints, markers, the moves' lines.
    pub(super) fn diff_look(&self, look: &mut Look, palette: &Palette) {
        let Some(view) = self.diff.current().filter(|_| self.diff.show) else {
            return;
        };
        let marks = view.marks(self.scene(), self.diff.side.0, &self.diff.filters);
        for (k, kind) in &marks.tint {
            look.tint.insert(*k, kind_colour(palette, *kind));
        }
        for (p, kind) in &marks.markers {
            let c = kind_colour(palette, *kind);
            look.overlay
                .extend(compare::box_lines(*p, 1.5).into_iter().map(|l| (l, c)));
        }
        for (a, b) in &marks.moves {
            look.overlay.push(([*a, *b], palette.view_diff_moved));
        }
        // the row chosen in the panel: a larger box
        if let Some(p) = self
            .diff
            .picked
            .and_then(|k| view.rows.get(k))
            .and_then(|r| r.position(self.diff.side.0))
        {
            let c = palette.view_status_info;
            look.overlay
                .extend(compare::box_lines(p, 4.0).into_iter().map(|l| (l, c)));
        }
    }

    /// Goes to a row: selects its object on the side shown (and flies to it), else flies to where it stands.
    pub fn go_to_row(&mut self, gpu: &GpuCtx, k: usize) {
        let Some(view) = self.diff.current() else { return };
        let Some(r) = view.rows.get(k).cloned() else { return };
        self.diff.picked = Some(k);
        let side = self.diff.side.0;
        let entity = r.guid(side).and_then(|g| {
            self.scene()
                .entities
                .iter()
                .find(|e| e.guid.eq_ignore_ascii_case(g))
                .map(|e| e.index)
        });
        match entity {
            Some(e) => {
                self.select(gpu, Some(e), true);
                self.focus_selected();
            }
            None => {
                if let Some(p) = r.position(side) {
                    for vp in self.viewports.values_mut() {
                        vp.camera.focus(
                            [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])],
                            Some(camera::focus_distance(6.0)),
                        );
                        vp.dirty = true;
                    }
                }
            }
        }
        self.redraw();
    }
}

fn swatch(ui: &mut egui::Ui, c: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().rect_filled(r, 2.0, c);
}

impl Tabs<'_> {
    /// The Diff panel.
    #[allow(clippy::too_many_lines, reason = "the panel's parts in order")]
    pub(super) fn diff(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        let right = match &self.view.diff.view {
            Some(Ok(v)) => format!("{} differences", v.rows.len()),
            _ => String::new(),
        };
        self.header(ui, "Diff", &right);
        let gpu = self.gpu;
        let mut go: Option<usize> = None;
        let mut again = false;
        let mut with_level = false;
        let mut with_base = false;
        let mut inspect = false;
        let mut look_changed = false;
        egui::Frame::new()
            .inner_margin(Margin::same(m.space_3 as i8))
            .show(ui, |ui| {
                let v = &mut *self.view;
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(v.doc.is_some(), egui::Button::new("Compare with base"))
                        .on_hover_text("The project against its base level: what the edits added, removed, moved and changed")
                        .on_disabled_hover_text("Open a project first")
                        .clicked()
                    {
                        with_base = true;
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut v.diff.other_level)
                            .hint_text("a level, e.g. mp_005gr")
                            .desired_width(130.0),
                    );
                    if ui
                        .add_enabled(!v.diff.other_level.trim().is_empty(), egui::Button::new("Compare with level"))
                        .on_hover_text("Another level of the game copy (A) against this one (B)")
                        .clicked()
                    {
                        with_level = true;
                    }
                    if v.diff.view.is_some()
                        && ui
                            .add_enabled(v.diff.job.is_none(), egui::Button::new("Refresh"))
                            .clicked()
                    {
                        again = true;
                    }
                });
                if v.diff.job.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Comparing (LevelDiff's comparison)...");
                    });
                }
                let view = match &v.diff.view {
                    None => {
                        ui.label(
                            RichText::new(
                                "Nothing compared yet. Compare with base lists what the project changed; Compare with \
                                 level compares this level with another; the Export window's Preview opens the export \
                                 beside the original.",
                            )
                            .color(p.ink_muted),
                        );
                        return;
                    }
                    Some(Err(e)) => {
                        super::status_line(ui, &p, "Error", p.status_error, e);
                        return;
                    }
                    Some(Ok(view)) => Arc::clone(view),
                };
                let side = v.diff.side.0;
                ui.label(format!(
                    "A: {}  ({} instances)    B: {}  ({} instances)",
                    view.label_a, view.counts[0], view.label_b, view.counts[1]
                ));
                ui.label(
                    RichText::new(format!(
                        "This view shows {}: its differing objects are tinted; a box marks where an object of {} stands. \
                         {} instances are the same. Compared in {:.2} s.",
                        if side == Side::A { "A" } else { "B" },
                        if side == Side::A { "B" } else { "A" },
                        view.same,
                        view.seconds
                    ))
                    .color(p.ink_muted),
                );
                if !v.diff.note.is_empty() {
                    ui.label(RichText::new(&v.diff.note).color(p.ink_muted));
                }
                let counts = view.counts_by_kind();
                ui.horizontal_wrapped(|ui| {
                    for k in Kind::ALL {
                        swatch(ui, kind_colour(&p, k));
                        let mut on = v.diff.filters.kinds.get(k.index()).copied().unwrap_or(true);
                        if ui
                            .checkbox(&mut on, format!("{} {}", k.word(), counts.get(k.index()).copied().unwrap_or(0)))
                            .changed()
                        {
                            if let Some(x) = v.diff.filters.kinds.get_mut(k.index()) {
                                *x = on;
                            }
                            look_changed = true;
                        }
                    }
                    if ui
                        .checkbox(&mut v.diff.show, "Show in the view")
                        .on_hover_text("Tints and markers in the 3D view")
                        .changed()
                    {
                        look_changed = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let groups = view.counts_by_group(&v.diff.filters);
                    egui::ComboBox::from_id_salt("diff-group")
                        .selected_text(v.diff.filters.group.map_or("Every group", Group::word))
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(v.diff.filters.group.is_none(), "Every group").clicked() {
                                v.diff.filters.group = None;
                                look_changed = true;
                            }
                            for (g, n) in groups {
                                if ui
                                    .selectable_label(v.diff.filters.group == Some(g), format!("{} ({n})", g.word()))
                                    .clicked()
                                {
                                    v.diff.filters.group = Some(g);
                                    look_changed = true;
                                }
                            }
                        });
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut v.diff.filters.text)
                                .hint_text("type, id, blueprint or GUID")
                                .desired_width(170.0),
                        )
                        .changed()
                    {
                        look_changed = true;
                    }
                    if ui.checkbox(&mut v.diff.filters.placed_only, "Placed objects only").changed() {
                        look_changed = true;
                    }
                });
                let rows = view.visible(&v.diff.filters);
                ui.separator();
                let row_h = m.row.max(20.0);
                v.diff.row_rects.clear();
                let picked = v.diff.picked;
                egui::ScrollArea::vertical()
                    .id_salt("diff-rows")
                    .max_height((ui.available_height() - if picked.is_some() { 190.0 } else { 12.0 }).max(120.0))
                    .auto_shrink([false, true])
                    .show_rows(ui, row_h, rows.len(), |ui, range| {
                        for &k in rows.get(range).unwrap_or_default() {
                            let Some(r) = view.rows.get(k) else { continue };
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                            if picked == Some(k) {
                                ui.painter().rect_filled(rect, 0.0, p.accent_subtle);
                            } else if resp.hovered() {
                                ui.painter().rect_filled(rect, 0.0, p.surface_2);
                            }
                            let c = kind_colour(&p, r.kind);
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(
                                    rect.min + Vec2::new(4.0, row_h / 2.0 - 5.0),
                                    Vec2::splat(10.0),
                                ),
                                2.0,
                                c,
                            );
                            ui.painter().text(
                                rect.min + Vec2::new(20.0, row_h / 2.0),
                                egui::Align2::LEFT_CENTER,
                                format!("{:<8} {}", r.kind.word(), r.label()),
                                egui::FontId::proportional(13.0),
                                p.ink,
                            );
                            ui.painter().text(
                                rect.right_center() - Vec2::new(6.0, 0.0),
                                egui::Align2::RIGHT_CENTER,
                                r.group.word(),
                                egui::FontId::proportional(11.0),
                                p.ink_muted,
                            );
                            v.diff.row_rects.push((k, rect));
                            if resp.clicked() {
                                go = Some(k);
                            }
                        }
                    });
                ui.separator();
                if let Some(r) = picked.and_then(|k| view.rows.get(k)) {
                    ui.label(RichText::new(format!("{}  {}", r.kind.word(), r.label())).strong());
                    let g = |x: &Option<String>| x.clone().unwrap_or_else(|| "-".into());
                    ui.label(format!("GUID in A: {}    in B: {}", g(&r.guid_a), g(&r.guid_b)));
                    let at = |x: Option<[f32; 3]>| {
                        x.map_or_else(|| "-".into(), |p| format!("{:.1}, {:.1}, {:.1}", p[0], p[1], p[2]))
                    };
                    ui.label(format!("At in A: {}    in B: {}", at(r.pos_a), at(r.pos_b)));
                    egui::ScrollArea::vertical()
                        .id_salt("diff-fields")
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for c in &r.changes {
                                let clip = |s: &Option<String>| {
                                    let s = s.as_deref().unwrap_or("(none)");
                                    if s.chars().count() > 90 {
                                        format!("{}...", s.chars().take(90).collect::<String>())
                                    } else {
                                        s.to_owned()
                                    }
                                };
                                ui.monospace(format!("{}: {}  ->  {}", c.path, clip(&c.a), clip(&c.b)));
                            }
                            for x in &r.removed {
                                ui.monospace(format!("- {x}"));
                            }
                            for x in &r.added {
                                ui.monospace(format!("+ {x}"));
                            }
                        });
                    if ui
                        .add_enabled(r.guid(side).is_some(), egui::Button::new("Inspect all fields"))
                        .on_hover_text("Selects it and opens Properties > All fields")
                        .on_disabled_hover_text("It is not in the level this view shows")
                        .clicked()
                    {
                        inspect = true;
                    }
                }
            });
        let palette = self.palette;
        if with_base {
            self.view.compare_with_base();
        }
        if with_level {
            let other = self.view.diff.other_level.clone();
            self.view.compare_with_level(&other);
        }
        if again {
            match self.view.diff.against.clone() {
                Against::Base => self.view.compare_with_base(),
                Against::Level(l) => self.view.compare_with_level(&l),
                Against::Nothing | Against::Stage => {}
            }
        }
        if let Some(k) = go {
            self.view.go_to_row(gpu, k);
            look_changed = true;
        }
        if inspect && let Some(k) = self.view.diff.picked {
            self.view.go_to_row(gpu, k);
            self.view.pending.push(Action::ShowPanel(PanelKind::Properties));
        }
        if look_changed {
            self.view.apply_look(&palette);
            self.view.refresh(gpu, &palette);
        }
    }
}
