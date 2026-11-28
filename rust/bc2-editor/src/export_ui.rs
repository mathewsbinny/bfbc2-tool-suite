//! The Export window (`EDITOR_ARCHITECTURE.md` 3.12, "The editor"; round E3b): the project exported as a NEW
//! multiplayer level, staged in OPFOR's `work\mapedit\<level>\stage` and checked offline (`bc2_levelwrite::export`).
//! The window never writes the game copy itself: Install runs the map export's install (`bc2_install::mapedit`, round
//! OR6: in process, what Project OPFOR's `mapedit_install.py` ran in Python), shown with the `bfbc2 install mapedit`
//! command that does the same.
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
use std::sync::mpsc::Receiver;

use bc2_levelwrite::awards::Rgb;
use bc2_levelwrite::export::{self, ExportSpec, Game, Identity};
use bc2_levelwrite::modelayer;
use bc2_levelwrite::mpconvert::{Anchor, LayerSpec};
use bc2_levelwrite::names::{self, Languages, Taken};
use bc2_levelwrite::profile::{ContentProfile, Donor, Profiles};
use bc2_levelwrite::{MapProject, Profile};
use eframe::egui;

use crate::edit::Doc;
use crate::error::EditorError;
use crate::strip_ui::StripWindow;
use crate::task::Task;

/// The mode an export file without a project target builds (E3's graft at export time), and its stock donor when the
/// profiles are not read yet. A project with a target (round E3d) takes any mode its era's stock levels have.
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
    /// Which project the window is for, and whether it has bound to that project's generation (round OP1: one state
    /// for DF1's `project` and `bound`).
    pub binding: ExportFor,
    /// The game folder (OPFOR's `game\`) and OPFOR's folder.
    pub opfor: Option<PathBuf>,
    /// The form.
    pub spec: Option<ExportSpec>,
    /// The last export's lines (report, then the checks).
    pub log: Vec<String>,
    /// The campaign-content checklist.
    pub strip: StripWindow,
    /// The game copy's content profiles for the form's mode (read on a worker).
    pub profiles: Option<Result<Profiles, EditorError>>,
    running: Option<Task<Vec<String>>>,
    scanning: Option<Task<Profiles>>,
    previewing: Option<Task<PreviewData>>,
    preview: Option<Preview>,
    preview_error: Option<String>,
    /// The target the form shows (round E3d): era, mode, donor; "Set target" makes it the project's.
    pub target: Option<(Profile, String, String)>,
    /// The game copy's modes per era (read once).
    pub era_modes: Option<Result<crate::app::EraModes, EditorError>>,
    /// The donor's own layer counted (the completeness rows): (donor, its signature).
    donor_sig: Option<(String, Result<modelayer::LayerSig, EditorError>)>,
    /// The Install dialog (round E3d).
    pub install: Install,
    /// The game copy's names in use and its languages (round EX1: the names' checks), read once.
    pub taken: Option<(Taken, Languages)>,
    /// Preview export asked for: the stage to open beside the original (the app opens it).
    pub want_preview: Option<PathBuf>,
    /// The project's layer counted, what it lacks, its anchors (kept until the log changes).
    #[allow(clippy::type_complexity, reason = "the four things the form shows of the layer")]
    layer: Option<(u64, modelayer::LayerSig, Vec<String>, Vec<(String, String, [f32; 3])>)>,
    /// The Weather section (round VW2).
    pub weather: crate::weather_ui::WeatherForm,
    /// Why Export cannot run now, as last drawn (`None`: it can; the tests read it).
    pub why_not: Option<String>,
}

/// The project the Export window is for (round OP1, audit AU4 CQ-7: an explicit state keyed by the project's path,
/// for DF1's two side fields).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExportFor {
    /// No project.
    #[default]
    None,
    /// Opened for this project file; not drawn with it yet.
    Opened(PathBuf),
    /// Drawn with its project in this project generation (round DF1, audit AU4-SM-2: the editor's
    /// [`crate::app::EditorApp::generation`]); a window of another generation is unbound before it acts.
    Bound {
        /// The project file.
        project: PathBuf,
        /// The generation.
        generation: u64,
    },
}

impl ExportWindow {
    /// The project file the window is for.
    #[must_use]
    pub fn project(&self) -> Option<&Path> {
        match &self.binding {
            ExportFor::None => None,
            ExportFor::Opened(p) | ExportFor::Bound { project: p, .. } => Some(p),
        }
    }

    /// The project generation it is bound to.
    #[must_use]
    pub fn bound(&self) -> Option<u64> {
        match &self.binding {
            ExportFor::Bound { generation, .. } => Some(*generation),
            _ => None,
        }
    }

    /// Binds the window to its project's generation (it draws with it; round DF1).
    pub fn bind(&mut self, generation: u64) {
        if let ExportFor::Opened(project) | ExportFor::Bound { project, .. } = std::mem::take(&mut self.binding) {
            self.binding = ExportFor::Bound { project, generation };
        }
    }
}

/// Why the Export button waits while an install or uninstall runs (round DF1, audit AU4-CON-1).
pub const WAIT_INSTALL: &str =
    "An install or uninstall is running: Export waits for it (it would rebuild the stage being copied)";
/// Why Install now / Uninstall now wait while an export runs.
pub const WAIT_EXPORT: &str = "An export is running: wait for it (it rebuilds the stage the install copies)";
/// Why Export cannot run while the project is not the open view (the export's preview shows the stage).
pub const NO_PROJECT: &str = "The project is not shown: close the preview first (the export builds the open project)";

/// Runs a worker's body, a panic turned into an error line (round DF1, audit AU4-BUG-4).
///
/// # Errors
/// The body's error, or its panic's text.
pub fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T, EditorError>) -> Result<T, EditorError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let text = p
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "no message".to_owned());
            Err(EditorError::Tool(format!("{what}: internal error ({text})")))
        }
    }
}

/// The stage folder of an export (`<opfor>\work\mapedit\<id>\stage`), from the level id as the export makes it
/// ([`ExportSpec::level_id`]: trimmed, checked) - the one place the editor computes it (round DF1, audit AU4-SM-3).
#[must_use]
pub fn stage_of(opfor: &Path, spec: &ExportSpec) -> Option<PathBuf> {
    let id = spec.level_id().ok()?;
    Some(
        opfor
            .join("work")
            .join("mapedit")
            .join(id.to_ascii_lowercase())
            .join("stage"),
    )
}

/// The Install dialog's state (round E3d; in process since round OR6): the map export's install run on a worker -
/// the plan first (what an install changes, nothing written), then, on confirmation, the install; Uninstall takes
/// the level out again.
#[derive(Default)]
pub struct Install {
    /// The dialog is shown.
    pub open: bool,
    /// What it is about: `plan`, `apply` or `revert`.
    pub step: String,
    /// The last run's output.
    pub text: Vec<String>,
    /// The stage the plan was shown for, and its `stage.json` then (round DF1, audit AU4-SM-3): Install now installs
    /// exactly that stage, and refuses when it was exported again since.
    pub planned: Option<(PathBuf, Vec<u8>)>,
    /// The level id Uninstall takes out (the export's id, as [`ExportSpec::level_id`] makes it).
    pub lv: String,
    /// The run (round OP1: one state for DF1's `running` receiver and `ok`).
    run: InstallRun,
}

/// Where the install's run stands.
#[derive(Default)]
enum InstallRun {
    /// None yet.
    #[default]
    Idle,
    /// Running on a worker (round PR1b-2-rest: a [`Task`]).
    Running(Task<(bool, String)>),
    /// Ended: whether it succeeded (a confirmation asked without a run counts as ready).
    Ended(bool),
}

/// The command line that runs the same step outside the editor (`bfbc2 install mapedit <step> <stage or level>
/// --game <opfor>\game`), shown above the step's lines.
#[must_use]
pub fn install_command(opfor: &Path, step: &str, target: &str) -> Vec<String> {
    vec![
        "bfbc2".to_owned(),
        "install".to_owned(),
        "mapedit".to_owned(),
        step.to_owned(),
        target.to_owned(),
        "--game".to_owned(),
        install_game(opfor).display().to_string(),
    ]
}

/// The game copy the Install writes into (round OR15, IN1): the game-folder setting (`bfbc2 settings
/// game-folder`) when there is one, else Project OPFOR's game folder.
#[must_use]
pub fn install_game(opfor: &Path) -> PathBuf {
    bc2_gamefs::settings::game_folder(None, Some(opfor)).map_or_else(|_| opfor.join("game"), |(g, _)| g)
}

/// One step of the map export's install, in process (round OR6: [`bc2_install::mapedit`], what OPFOR's
/// `mapedit_install.py` ran in Python): `plan` or `apply` of a stage, `revert` of a level, on `<opfor>\game`, the
/// shared patches built in `<opfor>\work\mapedit`. Returns whether it went through and its lines (the refusal
/// last). A kill part way (the editor ended) leaves nothing half written: the next run finishes it.
#[must_use]
pub fn install_step(opfor: &Path, step: &str, target: &str) -> (bool, String) {
    use bc2_install::mapedit::{Game, Installer};
    let game = install_game(opfor).display().to_string();
    let work = opfor.join("work").join("mapedit").display().to_string();
    let mut lines: Vec<String> = Vec::new();
    let r = {
        let mut log = |l: &str| lines.push(l.to_owned());
        // the test hook (BC2_INSTALL_CRASH_AT) is never read here: an editor run is never stopped by it
        let steps = bc2_install::stages::Steps::new(None);
        let patches = bc2_levelwrite::installpatch::LevelWrite;
        let mut inst = Installer::with_steps(Game::new(&game, None), &work, steps, &patches, &mut log);
        match step {
            "plan" => inst.plan_only(target),
            "apply" => inst.apply(target),
            _ => inst.revert(&[target.to_owned()]),
        }
    };
    if let Err(e) = &r {
        lines.extend(e.trim_end().lines().map(str::to_owned));
    }
    (r.is_ok(), lines.join("\n"))
}

impl Install {
    /// Runs a step of the install on a worker: `plan` or `apply` of a stage, `revert` of a level.
    pub fn run(&mut self, opfor: &Path, step: &str, target: &str) {
        self.open = true;
        step.clone_into(&mut self.step);
        self.text = vec![format!("> {}", install_command(opfor, step, target).join(" "))];
        let (dir, step, target) = (opfor.to_path_buf(), step.to_owned(), target.to_owned());
        self.run = InstallRun::Running(Task::spawn_value("The install", move || {
            install_step(&dir, &step, &target)
        }));
    }

    /// Runs the plan for a stage, keeping the stage and its `stage.json` for Install now.
    pub fn plan(&mut self, opfor: &Path, stage: &Path) {
        let json = std::fs::read(stage.join("stage.json")).unwrap_or_default();
        self.planned = Some((stage.to_path_buf(), json));
        self.run(opfor, "plan", &stage.display().to_string());
    }

    /// Why Install now cannot install the planned stage (`None`: it can): no plan, or the stage was exported again
    /// after the plan was shown (round DF1, audit AU4-SM-3).
    #[must_use]
    pub fn stale_plan(&self) -> Option<String> {
        let Some((stage, json)) = &self.planned else {
            return Some("No plan was shown: run Install... again".into());
        };
        let now = std::fs::read(stage.join("stage.json")).unwrap_or_default();
        (now != *json).then(|| {
            format!(
                "{} was exported again after the plan was shown: close this and run Install... again",
                stage.display()
            )
        })
    }

    /// Whether a run is going on.
    #[must_use]
    pub fn busy(&self) -> bool {
        matches!(self.run, InstallRun::Running(_))
    }

    /// Whether the last run succeeded (or a confirmation waits).
    #[must_use]
    pub fn ok(&self) -> bool {
        matches!(self.run, InstallRun::Ended(true))
    }

    /// A confirmation asked without a run (Uninstall's question): ready for the answer.
    pub fn ready(&mut self) {
        self.run = InstallRun::Ended(true);
    }

    fn take(&mut self, (ok, out): (bool, String)) {
        self.text.extend(out.lines().map(str::to_owned));
        self.run = InstallRun::Ended(ok);
    }

    /// The run's end, when it came (a worker that ended without an answer is a failed run, round DF1).
    pub fn poll(&mut self) {
        let InstallRun::Running(task) = &self.run else { return };
        if let Some(r) = task.poll() {
            self.take(r.unwrap_or_else(|e| (false, e.to_string())));
        }
    }

    /// Waits for the run (the headless screenshots).
    pub fn settle(&mut self) {
        if let InstallRun::Running(task) = std::mem::take(&mut self.run) {
            self.take(task.wait().unwrap_or_else(|e| (false, e.to_string())));
        }
    }

    /// Hands a worker's receiver to the dialog (tests: a worker that ends without an answer).
    #[doc(hidden)]
    pub fn running_from(&mut self, rx: Receiver<Result<(bool, String), EditorError>>) {
        self.run = InstallRun::Running(Task::from_receiver("The install", rx));
    }
}

/// Writes a file whole (round DF1, audit AU4-BUG-14): [`bc2_levelwrite::project::write_bytes`] (its folder made,
/// then [`bc2_gamefs::atomic::write`]: beside it first, synced, moved over it, the move tried again while a scanner
/// holds the file); a write cut short leaves the old file.
///
/// # Errors
/// The folder, the write or the move failed (the temporary file is removed).
pub fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    bc2_levelwrite::project::write_bytes(path, bytes)
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
        folder: String::new(),
        title: format!("{} (Conquest)", project.name),
        mode: "conquest".into(),
        layer,
        profile,
        identity: Some(Identity::default()),
        path_data: false,
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
    checks_for(spec, game, saved, profiles, false)
}

/// [`checks`] for a project whose target (round E3d) brought its own mode layer in (`target`): the export file's
/// bases, flags and combat area are not asked for (the layer is the project's; its completeness is checked from the
/// level itself).
#[must_use]
pub fn checks_for(
    spec: &ExportSpec,
    game: Option<&Game>,
    saved: bool,
    profiles: Option<&Profiles>,
    target: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    if !saved {
        out.push("Save the project first (the export builds the saved project)".into());
    }
    // round EX1: every name checked together (the game copy's names in use: `name_checks`)
    let mode = modelayer::mode_name(&modelayer::mode_key(&spec.mode));
    out.extend(
        names::check(&spec.names(), None, &mode, None, None)
            .into_iter()
            .map(|p| p.message),
    );
    if let (Some(g), Ok(id)) = (game, spec.level_id()) {
        let low = id.to_ascii_lowercase();
        let exists = !g.level_bundles(&low, "level").is_empty();
        if exists && !export::is_custom(g, &low) {
            out.push(format!("{id} is a level of the game copy: choose another name"));
        }
    }
    if let Some(id) = &spec.identity {
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
    if !target {
        if !MODES.iter().any(|m| m.0 == spec.mode) {
            out.push(format!(
                "The {} mode needs the project's target: choose the era and mode above and Set target",
                spec.mode
            ));
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
            Some(_) if target => {}
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

/// The export form made to show a project's target (round E3d): its era, mode (`maps.ini`'s name) and donor.
pub fn sync_spec(s: &mut ExportSpec, c: &modelayer::ModeChoice) {
    s.profile = c.profile;
    s.mode = modelayer::mode_name(&c.mode);
    s.layer.donor.clone_from(&c.donor);
}

fn to_image(p: &Rgb) -> egui::ColorImage {
    egui::ColorImage::from_rgb([p.w, p.h], &p.px)
}

impl ExportWindow {
    /// Opens the window for a project (its settings read from beside it, else a first form).
    pub fn open_for(&mut self, project: &Path, opfor: Option<&Path>) {
        if self.project() != Some(project) {
            self.unbind();
        }
        self.open = true;
        // bound to the editor's project generation when it first draws for this project (round DF1)
        self.binding = ExportFor::Opened(project.to_path_buf());
        self.opfor = opfor.map(Path::to_path_buf);
        let from_file = std::fs::read_to_string(spec_path(project))
            .ok()
            .and_then(|t| ExportSpec::from_json(&t).ok());
        let profiles = self.profiles.as_ref().and_then(|p| p.as_ref().ok());
        let loaded = MapProject::load(project).ok();
        self.spec = from_file.or_else(|| loaded.as_ref().map(|p| default_spec(p, profiles)));
        // round E3d: the project's own target, else the form's (Conquest of the base's era)
        let target = loaded.as_ref().and_then(|p| p.mode().cloned());
        self.target = match (&target, &self.spec) {
            (Some(c), _) => Some((c.profile, c.mode.clone(), c.donor.clone())),
            (None, Some(s)) => Some((s.profile, modelayer::mode_key(&s.mode), s.layer.donor.clone())),
            (None, None) => None,
        };
        if let (Some(c), Some(s)) = (&target, self.spec.as_mut()) {
            sync_spec(s, c);
        }
        // round EX1: the names the project's log keeps
        if let (Some(n), Some(s)) = (loaded.as_ref().and_then(MapProject::names), self.spec.as_mut()) {
            s.set_names(n);
        }
        if self.taken.is_none()
            && let Some(g) = self.game()
        {
            self.taken = Some((Taken::read(&g), Languages::read(&g)));
        }
        if self.era_modes.is_none() {
            self.era_modes = Some(
                self.game()
                    .ok_or_else(|| EditorError::Tool("no game copy".to_owned()))
                    .and_then(|g| bc2_levelwrite::profile::modes(&g).map_err(|e| EditorError::Tool(e.to_string()))),
            );
        }
        if self.profiles.is_none() && self.scanning.is_none() {
            self.scan();
        }
    }

    /// Lets go of the project the window was opened for (round DF1, audit AU4-SM-2: the map closed or another
    /// project opened): the window, the campaign checklist and a finished Install dialog close, and the form, its
    /// target, the layer counted, the preview pictures and the weather form are dropped. A running export or install
    /// is kept: its end is still reported, and the stage it works on stays guarded until then. The game copy's
    /// profiles, names and modes are the game copy's and are kept.
    pub fn unbind(&mut self) {
        self.open = false;
        self.binding = ExportFor::None;
        self.spec = None;
        self.target = None;
        self.layer = None;
        self.donor_sig = None;
        self.preview = None;
        self.previewing = None;
        self.preview_error = None;
        self.want_preview = None;
        self.weather = crate::weather_ui::WeatherForm::default();
        self.strip.open = false;
        self.why_not = None;
        if self.running.is_none() {
            self.log.clear();
        }
        if !self.install.busy() {
            self.install.open = false;
            self.install.planned = None;
        }
    }

    /// Whether an export runs now.
    #[must_use]
    pub fn exporting(&self) -> bool {
        self.running.is_some()
    }

    /// Hands an export worker's receiver to the window (tests: a worker that ends without an answer).
    #[doc(hidden)]
    pub fn running_from(&mut self, rx: Receiver<Result<Vec<String>, EditorError>>) {
        self.running = Some(Task::from_receiver("The export", rx));
    }

    /// The mode the form's target names (`conquest` before one is chosen).
    fn target_mode(&self) -> String {
        self.target
            .as_ref()
            .map_or_else(|| "conquest".to_owned(), |t| t.1.clone())
    }

    fn game(&self) -> Option<Game> {
        Game::open(&self.opfor.as_ref()?.join("game")).ok()
    }

    /// Reads the content profiles on a worker.
    fn scan(&mut self) {
        let Some(opfor) = self.opfor.clone() else { return };
        let mode = self.target_mode();
        self.scanning = Some(Task::spawn("Reading the content profiles", move || {
            Game::open(&opfor.join("game"))
                .and_then(|g| bc2_levelwrite::profile::scan(&g, &mode))
                .map_err(|e| EditorError::Tool(e.to_string()))
        }));
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
        self.preview_error = None;
        self.previewing = Some(Task::spawn(
            "The preview pictures",
            move || -> Result<PreviewData, EditorError> {
                let game = Game::open(&opfor.join("game")).map_err(|e| EditorError::Tool(e.to_string()))?;
                let donor = spec.layer.donor.to_ascii_lowercase();
                let dk = export::description_key(&game, &donor).map_err(|e| EditorError::Tool(e.to_string()))?;
                let bk = export::description_key(&game, &base).map_err(|e| EditorError::Tool(e.to_string()))?;
                let bp = export::base_picture_name(&game, &bk, &base).map_err(|e| EditorError::Tool(e.to_string()))?;
                let mut p = export::pictures(&game, &donor, &dk, &bp, &picture)
                    .map_err(|e| EditorError::Tool(e.to_string()))?;
                let level = p.pop().ok_or(EditorError::Tool("no picture".into()))?;
                let loading = p.pop().ok_or(EditorError::Tool("no picture".into()))?;
                Ok(PreviewData {
                    from: loading.from.clone(),
                    psnr: [loading.made.psnr, level.made.psnr],
                    loading: loading.made.decoded,
                    level: level.made.decoded,
                    key: format!("{picture}|{donor}"),
                })
            },
        ));
    }

    fn start(&mut self) {
        let (Some(project), Some(opfor), Some(spec)) = (
            self.project().map(Path::to_path_buf),
            self.opfor.clone(),
            self.spec.clone(),
        ) else {
            return;
        };
        // the settings beside the project, written whole (round DF1, audit AU4-BUG-14): a failed write is said
        let mut notes = Vec::new();
        match serde_json::to_string_pretty(&spec) {
            Ok(json) => {
                if let Err(e) = write_whole(&spec_path(&project), (json + "\n").as_bytes()) {
                    notes.push(format!(
                        "note: the export settings were not saved beside the project ({}: {e})",
                        spec_path(&project).display()
                    ));
                }
            }
            Err(e) => notes.push(format!("note: the export settings were not saved ({e})")),
        }
        self.log = vec!["Exporting...".into()];
        self.log.extend(notes.iter().cloned());
        self.running = Some(Task::spawn(
            "The export",
            move || -> Result<Vec<String>, EditorError> {
                let game = Game::open(&opfor.join("game")).map_err(|e| EditorError::Tool(e.to_string()))?;
                let p = MapProject::load(&project).map_err(|e| EditorError::Tool(e.to_string()))?;
                spec.level_id().map_err(|e| EditorError::Tool(e.to_string()))?;
                let stage = stage_of(&opfor, &spec).ok_or(EditorError::Tool("no level id".into()))?;
                let rec = export::build(&game, &p, &project.display().to_string(), &spec, &stage)
                    .map_err(|e| EditorError::Tool(e.to_string()))?;
                let r = export::check(&game, &stage).map_err(|e| EditorError::Tool(e.to_string()))?;
                let mut out = rec.report;
                out.extend(
                    r.lines()
                        .map(|(code, ok, w)| format!("{} {code} {w}", if ok { "ok" } else { "FAILED" })),
                );
                out.push(if r.ok() {
                    format!(
                        "Staged in {} - nothing in the game copy changed. To install it (the game closed): Install... \
                         below, or bfbc2 install mapedit apply {} --game {}",
                        stage.display(),
                        stage.display(),
                        opfor.join("game").display()
                    )
                } else {
                    "A check failed: the stage is not ready to install".into()
                });
                out.extend(notes);
                Ok(out)
            },
        ));
    }

    /// The workers' answers that came (round DF1, audit AU4-BUG-4: a worker that ended without one - it panicked -
    /// is reported and its wait ended, never polled forever).
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(r) = self.running.as_ref().and_then(Task::poll) {
            self.log = match r {
                Ok(lines) => lines,
                Err(e) => vec![format!("Not exported: {e}")],
            };
            self.running = None;
        }
        if let Some(r) = self.scanning.as_ref().and_then(Task::poll) {
            self.scanning = None;
            let project = self.project().map(Path::to_path_buf);
            // a first form made before the profiles were read takes the profile's default donor
            if let (Ok(p), Some(spec), Some(project)) = (&r, self.spec.as_mut(), project)
                && !spec_path(&project).is_file()
            {
                let profile = spec.profile;
                set_profile(spec, p, profile);
            }
            self.profiles = Some(r);
        }
        if let Some(r) = self.previewing.as_ref().and_then(Task::poll) {
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
                Err(e) => self.preview_error = Some(e.to_string()),
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
        if let Some(t) = self.scanning.take() {
            self.profiles = Some(t.wait());
        }
        self.start_preview(base);
        if let Some(t) = self.previewing.take() {
            let r = t.wait();
            self.previewing = Some(Task::answered("The preview pictures", r));
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
/// choice to it), `selected` its primary selected entity (an anchor can take its place), `generation` the editor's
/// project generation ([`crate::app::EditorApp::generation`]).
///
/// Round DF1 (audit AU4-SM-2): the window acts only on the project it was opened for. It binds to the generation it
/// first draws in with that project; a window of another generation is unbound (closed, its form dropped), and a
/// `doc` that is not its project is never written to.
#[allow(clippy::too_many_lines, reason = "the form's rows in order")]
pub fn window(
    ctx: &egui::Context,
    w: &mut ExportWindow,
    doc: Option<&mut Doc>,
    selected: Option<usize>,
    generation: u64,
) -> bool {
    if w.bound().is_some_and(|g| g != generation) {
        w.unbind();
    }
    // only the window's own project is acted on (the campaign checklist commits into the open project, whichever)
    let mut doc = doc;
    crate::strip_ui::window(ctx, &mut w.strip, doc.as_deref_mut());
    let mut doc = doc.filter(|d| w.project() == Some(d.path.as_path()));
    // the workers' answers, the window shown or not (an export's end ungates Install)
    w.install.poll();
    w.poll(ctx);
    install_dialog(ctx, w);
    if !w.open {
        return false;
    }
    if w.bound().is_none() && doc.is_some() {
        w.bind(generation);
    }
    let game = w.game();
    // round DF1 (audit AU4-DS-4): saved is the open project's own state, not the autosave file's absence (a failed
    // autosave writes none); without the project shown (the preview), the export waits
    let shown = doc.is_some();
    let saved = doc.as_deref().is_none_or(|d| d.saved);
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
    // round E3d: the project's target, the form's, the layer's completeness and its anchors
    let project_target = doc.as_deref().and_then(|d| d.project.mode().cloned());
    if let (Some(c), Some(s)) = (&project_target, w.spec.as_mut()) {
        sync_spec(s, c);
    }
    let era_modes = w.era_modes.as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let mut form_target = w.target.clone();
    if let (Some(d), Some(g)) = (project_target.as_ref().map(|c| c.donor.clone()), game.as_ref())
        && w.donor_sig.as_ref().is_none_or(|x| x.0 != d)
    {
        let sig = export::level_partition(g, &d)
            .map_err(|e| EditorError::Tool(e.to_string()))
            .and_then(|b| bc2_levelwrite::LevelWriter::parse(&b).map_err(|e| EditorError::Tool(e.to_string())))
            .map(|lw| modelayer::signature(&lw));
        w.donor_sig = Some((d, sig));
    }
    let key = doc
        .as_deref()
        .map_or(0, |d| d.project.operations.len() as u64 * 1_000_003 + d.project.next);
    if w.layer.as_ref().is_none_or(|l| l.0 != key) {
        w.layer = match (&project_target, doc.as_deref().and_then(Doc::writer), &w.donor_sig) {
            (Some(c), Some(lw), Some((_, Ok(ds)))) => {
                let have = modelayer::signature(lw);
                let miss = modelayer::missing(&c.mode, &have, ds);
                Some((key, have, miss, modelayer::anchors(lw)))
            }
            _ => None,
        };
    }
    let layer_state = w.layer.clone();
    let installed = game
        .as_ref()
        .zip(w.spec.as_ref().and_then(|s| s.level_id().ok()))
        .is_some_and(|(g, id)| export::is_custom(g, &id.to_ascii_lowercase()));
    let stage_dir = w
        .opfor
        .as_deref()
        .zip(w.spec.as_ref())
        .and_then(|(o, s)| stage_of(o, s));
    let staged = stage_dir.as_ref().is_some_and(|d| d.join("stage.json").is_file());
    let installing = w.install.busy();
    let has_doc = doc.is_some();
    let mut set_target = false;
    let mut move_anchor: Option<(String, [f32; 2])> = None;
    let mut install_ask: Option<&'static str> = None;
    let previewing = w.previewing.is_some();
    // round EX1: the names - the game copy's languages, the project's names, the rules against the game copy
    let languages: Vec<String> = w.taken.as_ref().map_or_default(|t| t.1.names());
    let project_names = doc.as_deref().and_then(|d| d.project.names().cloned());
    let project_id = doc.as_deref().map(|d| d.project.id.clone());
    let names_differ = w
        .spec
        .as_ref()
        .is_some_and(|s| project_names.as_ref() != Some(&s.names()));
    let mut keep_names = false;
    // round VW2: the weather the export gives the level
    let project_weather = doc.as_deref().and_then(|d| d.project.weather().cloned());
    let base_level = doc.as_deref().map(|d| d.project.base.level.to_ascii_lowercase());
    let mut keep_weather: Option<Option<bc2_levelwrite::weather::WeatherChoice>> = None;
    let stage_ready = staged && !busy;
    let mut preview_stage = false;
    let mut why_not: Option<String> = None;
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
                    ui.heading("Names");
                    ui.weak(
                        "Every name of the level, checked together: the id the game loads it by, its folder, the \
                         server browser's name and what the menus and the loading screen show in each language.",
                    );
                    egui::Grid::new("export-name").num_columns(2).show(ui, |ui| {
                        ui.label("Level id");
                        let before = spec.level.clone();
                        ui.text_edit_singleline(&mut spec.level).on_hover_text(
                            "What the game loads (Levels/<id>): maps.ini, package.mft, bundleManifest and the menus' level \
                             list name it; the localisation keys <ID>_NAME and <ID>_DESC come from it",
                        );
                        if spec.level != before {
                            spec.folder.clear();
                        }
                        ui.end_row();
                        ui.label("Folder");
                        let mut folder = if spec.folder.is_empty() {
                            spec.level.trim().to_ascii_lowercase()
                        } else {
                            spec.folder.clone()
                        };
                        if ui
                            .add(egui::TextEdit::singleline(&mut folder).desired_width(260.0))
                            .on_hover_text(
                                "Package\\levels\\<folder>\\: the level id in lower case (the game finds a level's \
                                 bundles there); a new folder name renames the id",
                            )
                            .changed()
                        {
                            spec.level = folder.trim().to_ascii_uppercase();
                            spec.folder.clear();
                        }
                        ui.end_row();
                        ui.label("Server browser");
                        ui.text_edit_singleline(&mut spec.title).on_hover_text(
                            "maps.ini's name: the launcher's row, and the server browser's \"OPFOR - <name> (<mode>)\"",
                        );
                        ui.end_row();
                        let mut own = spec.identity.is_some();
                        ui.label("");
                        if ui
                            .checkbox(&mut own, "Its own on-screen name and picture")
                            .on_hover_text(
                                "Off: the menus, the loading screen and the browser's map column show the base level's \
                                 name and picture",
                            )
                            .changed()
                        {
                            spec.identity = own.then(Identity::default);
                        }
                        ui.end_row();
                        if let Some(id) = spec.identity.as_mut() {
                            ui.label("On-screen name");
                            ui.add(
                                egui::TextEdit::singleline(&mut id.name)
                                    .hint_text("empty: the server browser's name")
                                    .desired_width(360.0),
                            )
                            .on_hover_text("The menus' map list, the loading screen and the server browser's map column");
                            ui.end_row();
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
                    if let Some(id) = spec.identity.as_mut() {
                        egui::CollapsingHeader::new("Per language")
                            .id_salt("export-languages")
                            .default_open(!id.languages.is_empty())
                            .show(ui, |ui| {
                                ui.weak(
                                    "Each language's own name and description; empty: the ones above. The letters must \
                                     be ones that language's texts have.",
                                );
                                egui::Grid::new("export-langs").num_columns(3).show(ui, |ui| {
                                    for lang in &languages {
                                        let e = id.languages.entry(lang.clone()).or_default();
                                        ui.label(names::language_title(lang));
                                        ui.add_sized(
                                            [200.0, 20.0],
                                            egui::TextEdit::singleline(&mut e.name).hint_text("name"),
                                        );
                                        ui.add_sized(
                                            [260.0, 20.0],
                                            egui::TextEdit::singleline(&mut e.description).hint_text("description"),
                                        );
                                        ui.end_row();
                                    }
                                });
                                id.languages
                                    .retain(|_, l| !l.name.trim().is_empty() || !l.description.trim().is_empty());
                            });
                    }
                    {
                        let n = spec.names();
                        let (name, _) = n.texts("english");
                        ui.weak(format!(
                            "Shown as: launcher and server browser \"OPFOR - {} ({})\"; menus and loading screen {:?} \
                             ({}_NAME, picture loadingScreens/{}); files Package\\levels\\{}\\",
                            spec.title.trim(),
                            modelayer::mode_name(&modelayer::mode_key(&spec.mode)),
                            if n.own { name } else { "the base level's name".to_owned() },
                            n.id(),
                            n.folder(),
                            n.folder()
                        ));
                    }
                    // the names' rules, at once under them (Export lists them too)
                    {
                        let mode = modelayer::mode_name(&modelayer::mode_key(&spec.mode));
                        let (t, l) = w.taken.as_ref().map_or((None, None), |(t, l)| (Some(t), Some(l)));
                        for p in names::check(&spec.names(), project_id.as_deref(), &mode, t, l) {
                            ui.colored_label(ui.visuals().warn_fg_color, p.message);
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(has_doc && names_differ, egui::Button::new("Keep in project"))
                            .on_hover_text("Puts the names into the project's log (undoable; Export does it too)")
                            .on_disabled_hover_text("The project has these names")
                            .clicked()
                        {
                            keep_names = true;
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
                    if let Some(b) = &base_level {
                        keep_weather = crate::weather_ui::section(
                            ui,
                            &mut w.weather,
                            game.as_ref(),
                            b,
                            project_weather.as_ref(),
                            doc.as_deref().and_then(Doc::writer),
                        );
                        ui.separator();
                    }
                    ui.heading("Level");
                    egui::Grid::new("export-form").num_columns(2).show(ui, |ui| {
                        let tgt = form_target.get_or_insert_with(|| {
                            (spec.profile, modelayer::mode_key(&spec.mode), spec.layer.donor.clone())
                        });
                        ui.label("Era");
                        egui::ComboBox::from_id_salt("export-era")
                            .selected_text(tgt.0.word())
                            .show_ui(ui, |ui| {
                                for p in Profile::ALL {
                                    let have = era_modes.as_ref().is_none_or(|m| m.contains_key(&p));
                                    if ui
                                        .add_enabled(have, egui::Button::selectable(tgt.0 == p, p.word()))
                                        .on_disabled_hover_text("The game copy has no multiplayer level of this era")
                                        .clicked()
                                    {
                                        tgt.0 = p;
                                        let ok = era_modes.as_ref().and_then(|m| m.get(&p)).is_some_and(|m| {
                                            m.contains_key(&tgt.1) && modelayer::not_offered(&tgt.1).is_none()
                                        });
                                        if !ok {
                                            "conquest".clone_into(&mut tgt.1);
                                        }
                                        tgt.2.clear();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("The era the level is made for: its kits, factions, announcer and bundles");
                        ui.end_row();
                        ui.label("Game mode");
                        egui::ComboBox::from_id_salt("export-mode")
                            .selected_text(modelayer::mode_name(&tgt.1))
                            .show_ui(ui, |ui| {
                                for (k, lvs) in era_modes.as_ref().and_then(|m| m.get(&tgt.0)).into_iter().flatten() {
                                    let why = modelayer::not_offered(k);
                                    if ui
                                        .add_enabled(
                                            why.is_none(),
                                            egui::Button::selectable(
                                                tgt.1 == *k,
                                                format!("{} ({} stock levels)", modelayer::mode_name(k), lvs.len()),
                                            ),
                                        )
                                        .on_disabled_hover_text(why.unwrap_or_default())
                                        .clicked()
                                    {
                                        k.clone_into(&mut tgt.1);
                                        tgt.2.clear();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("The era's multiplayer modes: those its stock levels have");
                        ui.end_row();
                        let set = profiles
                            .as_ref()
                            .filter(|ps| ps.mode == tgt.1)
                            .and_then(|ps| ps.get(tgt.0).ok());
                        if tgt.2.is_empty()
                            && let Some(d) = set.and_then(ContentProfile::default_donor)
                        {
                            tgt.2.clone_from(&d.level);
                        }
                        ui.label("Donor level");
                        egui::ComboBox::from_id_salt("export-donor")
                            .selected_text(tgt.2.to_ascii_uppercase())
                            .show_ui(ui, |ui| {
                                for d in set.map_or(&[][..], |s| s.donors.as_slice()) {
                                    let mut text = d.level.to_ascii_uppercase();
                                    if !d.outside.is_empty() {
                                        let _ = write!(text, " (+{} partitions of its own)", d.outside.len());
                                    }
                                    if ui.selectable_label(tgt.2 == d.level, text).clicked() {
                                        tgt.2.clone_from(&d.level);
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "The stock level of the era and mode whose mode layer, kits and description the \
                                 level takes",
                            );
                        ui.end_row();
                        ui.label("Target");
                        ui.horizontal(|ui| {
                            let now = project_target
                                .as_ref()
                                .map(|c| (c.profile, c.mode.clone(), c.donor.clone()));
                            let differs = now.as_ref() != Some(&*tgt);
                            ui.label(project_target.as_ref().map_or_else(
                                || "not set: the export grafts E3's Conquest layer".to_owned(),
                                modelayer::ModeChoice::describe,
                            ));
                            if ui
                                .add_enabled(has_doc && differs && !tgt.2.is_empty(), egui::Button::new("Set target"))
                                .on_hover_text(
                                    "Brings the donor's mode layer into the project as objects you move and edit, \
                                     and takes the level's own layer out (one undo step)",
                                )
                                .clicked()
                            {
                                set_target = true;
                            }
                        });
                        ui.end_row();
                        if project_target.is_none() {
                            spec.profile = tgt.0;
                            if let Some(d) = set.and_then(|s| s.donor(&tgt.2))
                                && spec.layer.donor != d.level
                            {
                                set_donor(spec, d);
                            }
                            spec.mode = modelayer::mode_name(&tgt.1).to_ascii_lowercase();
                        }
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
                                .on_hover_text("The announcer's voice-over, as the donor's set has it");
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
                        if project_target.is_none() {
                            ui.label("Combat area kept");
                            ui.text_edit_singleline(&mut spec.layer.combat_area);
                            ui.end_row();
                        }
                    });
                    if let Some(e) = &scan_error {
                        ui.colored_label(ui.visuals().warn_fg_color, format!("Content profiles: {e}"));
                    }
                    ui.separator();
                    match &layer_state {
                        Some((_, have, miss, anchors)) => {
                            ui.heading("Mode layer (the project's)");
                            if miss.is_empty() {
                                ui.label(format!(
                                    "Complete: {} bases, {} flags, {} combat areas, M-COMs per stage {:?}",
                                    have.bases.iter().sum::<usize>(),
                                    have.flags,
                                    have.combat_areas,
                                    have.stages
                                ));
                            }
                            for m in miss {
                                ui.colored_label(ui.visuals().warn_fg_color, format!("The mode layer: {m}"));
                            }
                            ui.label("Bases, flags and M-COMs (first guesses: take the selected object's place)");
                            for (g, what, at) in anchors {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{what}  x {:.0}  z {:.0}", at[0], at[2]));
                                    if ui
                                        .add_enabled(pick.is_some(), egui::Button::new("Use selection"))
                                        .on_hover_text(
                                            "Moves it, with its spawns, markers and area, where the selected object \
                                             stands (an undoable move)",
                                        )
                                        .on_disabled_hover_text("Select an object in the view: its place is taken")
                                        .clicked()
                                        && let Some(p) = pick
                                    {
                                        move_anchor = Some((g.clone(), [p[0], p[2]]));
                                    }
                                });
                            }
                        }
                        None if project_target.is_some() => {
                            ui.label("Reading the mode layer...");
                        }
                        None => {
                            ui.label("Bases (first guesses: move them, or take the selected object's place)");
                            for a in &mut spec.layer.bases {
                                anchor_row(ui, a, pick);
                            }
                            ui.label("Flags");
                            for a in &mut spec.layer.flags {
                                anchor_row(ui, a, pick);
                            }
                        }
                    }
                    ui.separator();
                    // round A2: the export's path-data step (opt-in)
                    ui.checkbox(&mut spec.path_data, "Stage the level's path data for bots").on_hover_text(
                        "Builds the level's path data with NavKit in process - the project's spawns and objectives as \
                         its seeds, its navigation edits applied; a level whose collision the project changes is \
                         sampled offline first (minutes) - and stages it with the level, checked by the export. A \
                         base level NavKit has no samples of stages DICE's file as it is",
                    );
                    let mut problems =
                        checks_for(spec, game.as_ref(), saved, profiles.as_ref(), project_target.is_some());
                    if let Some((t, l)) = &w.taken {
                        let mode = modelayer::mode_name(&modelayer::mode_key(&spec.mode));
                        for p in names::check(&spec.names(), project_id.as_deref(), &mode, Some(t), Some(l)) {
                            if !problems.contains(&p.message) {
                                problems.push(p.message);
                            }
                        }
                    }
                    if let Some((_, _, miss, _)) = &layer_state {
                        problems.extend(miss.iter().map(|m| format!("The mode layer is incomplete: {m}")));
                    }
                    if !shown {
                        problems.insert(0, NO_PROJECT.into());
                    }
                    for p in &problems {
                        ui.colored_label(ui.visuals().warn_fg_color, p);
                    }
                    // round DF1 (audit AU4-CON-1): never while an install copies the stage, said in words
                    let blocked = if busy {
                        Some("Exporting...".to_owned())
                    } else if installing {
                        Some(WAIT_INSTALL.to_owned())
                    } else {
                        problems.first().cloned()
                    };
                    if installing && !busy {
                        ui.colored_label(ui.visuals().warn_fg_color, WAIT_INSTALL);
                    }
                    why_not.clone_from(&blocked);
                    let tip = "Builds and checks the new level; the game copy is not changed";
                    if ui
                        .add_enabled(blocked.is_none(), egui::Button::new("Export"))
                        .on_hover_text(tip)
                        .on_disabled_hover_text(blocked.unwrap_or_default())
                        .clicked()
                    {
                        go = true;
                    }
                    if ui
                        .add_enabled(stage_ready, egui::Button::new("Preview export"))
                        .on_hover_text(
                            "Opens the staged export read only beside the original base level, with the same camera \
                             (T switches; the Diff panel lists what the export changed)",
                        )
                        .on_disabled_hover_text("Export the level first: the preview opens its stage")
                        .clicked()
                    {
                        preview_stage = true;
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("export-log")
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for l in &w.log {
                                ui.monospace(l);
                            }
                        });
                    ui.separator();
                    ui.heading("Install (Project OPFOR)");
                    ui.horizontal(|ui| {
                        let install_why = if busy {
                            WAIT_EXPORT
                        } else if installing {
                            "An install or uninstall is running"
                        } else {
                            "Export the level first: the install takes its stage"
                        };
                        if ui
                            .add_enabled(staged && !installing && !busy, egui::Button::new("Install..."))
                            .on_hover_text(
                                "Shows what an install changes in the game copy (bfbc2 install mapedit plan), \
                                 then installs it when you confirm",
                            )
                            .on_disabled_hover_text(install_why)
                            .clicked()
                        {
                            install_ask = Some("plan");
                        }
                        let uninstall_why = if busy {
                            WAIT_EXPORT
                        } else if installing {
                            "An install or uninstall is running"
                        } else {
                            "The level is not installed"
                        };
                        if ui
                            .add_enabled(installed && !installing && !busy, egui::Button::new("Uninstall..."))
                            .on_hover_text("Takes the level out of the game copy again (every shared file back)")
                            .on_disabled_hover_text(uninstall_why)
                            .clicked()
                        {
                            install_ask = Some("confirm-revert");
                        }
                        ui.weak(if installed { "installed" } else { "not installed" });
                    });
                    ui.weak("The game must be closed to install or uninstall. Nothing is written before you confirm.");
                });
        });
    w.open = open;
    let mut changed = false;
    w.target = form_target;
    if let Some(step) = install_ask {
        // the level id as the export makes it (trimmed; round DF1, audit AU4-SM-3)
        w.install.lv = w
            .spec
            .as_ref()
            .and_then(|s| s.level_id().ok())
            .map_or_else(String::new, |id| id.to_ascii_uppercase());
        if step == "confirm-revert" {
            w.install.open = true;
            step.clone_into(&mut w.install.step);
            w.install.text = vec![format!(
                "Take {} out of the game copy? Every shared file the install changed goes back from its backup.",
                w.install.lv
            )];
            w.install.ready();
            w.install.planned = None;
        } else if let (Some(o), Some(d)) = (w.opfor.clone(), stage_dir.as_ref()) {
            w.install.plan(&o, d);
        }
    }
    if let (Some(d), Some(g)) = (doc.as_deref_mut(), game.as_ref()) {
        if set_target && let Some((era, mode, donor)) = w.target.clone() {
            match export::choose_mode(g, &d.project, era, &mode, Some(&donor)) {
                Ok(c) => {
                    d.commit(bc2_levelwrite::Op::SetMode { to: Box::new(c) }, "Target: era and mode");
                    changed = true;
                }
                Err(e) => w.log = vec![format!("The target was not set: {e}")],
            }
        }
        if let Some((guid, to)) = move_anchor {
            match export::move_anchor(g, &d.project, &guid, to) {
                Ok(items) if !items.is_empty() => {
                    d.commit(
                        bc2_levelwrite::Op::Move { items },
                        "Use selection: a flag, base or M-COM placed",
                    );
                    changed = true;
                }
                Ok(_) => {}
                Err(e) => w.log = vec![format!("Not moved: {e}")],
            }
        }
    }
    // the donors listed are the target mode's: read them again when it changed
    if w.scanning.is_none()
        && let (Some(t), Some(Ok(p))) = (&w.target, &w.profiles)
        && p.mode != t.1
    {
        w.scan();
    }
    if open_strip && let Some(d) = doc.as_deref() {
        w.strip.open_for(d);
    }
    if want_preview && let Some((_, _, base)) = &strip_text {
        let base = base.clone();
        w.start_preview(&base);
    }
    // round EX1: the names into the project's log (on Export too), the preview asked for
    if (keep_names || go)
        && names_differ
        && let (Some(d), Some(s)) = (doc.as_mut(), w.spec.as_ref())
    {
        d.commit(
            bc2_levelwrite::Op::SetNames {
                to: Box::new(s.names()),
            },
            "Export: the level's names",
        );
        if go {
            // the export builds the saved project
            if let Err(e) = d.save() {
                w.log = vec![format!("Not exported: the project could not be saved ({e})")];
                go = false;
            }
        }
        changed = true;
    }
    // round VW2: the weather into the project's log
    if let (Some(c), Some(d)) = (keep_weather, doc.as_mut()) {
        d.commit(
            bc2_levelwrite::Op::SetWeather { to: c.map(Box::new) },
            "Export: the level's weather",
        );
        changed = true;
    }
    if preview_stage {
        w.want_preview.clone_from(&stage_dir);
    }
    w.why_not = why_not;
    // round DF1 (audit AU4-CON-1): checked again at the start (an install may have begun this frame)
    if go && w.install.busy() {
        w.log = vec![format!("Not exported: {WAIT_INSTALL}")];
    } else if go {
        w.start();
    }
    changed
}

/// The Install dialog (round E3d): the plan's output (what an install changes), then Install on confirmation; the
/// install's or the uninstall's output.
fn install_dialog(ctx: &egui::Context, w: &mut ExportWindow) {
    if !w.install.open {
        return;
    }
    if w.install.busy() || w.running.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
    let mut open = true;
    let mut ask: Option<(&'static str, String)> = None;
    // round DF1 (audit AU4-SM-3): the level and the stage the plan was shown for, never the form as it is now
    let lv = w.install.lv.clone();
    let stage = w.install.planned.as_ref().map(|p| p.0.clone());
    // round DF1 (audit AU4-CON-1): not while an export rebuilds the stage, and not a stage exported again since
    let exporting = w.running.is_some();
    let outdated = w.install.stale_plan();
    let title = match w.install.step.as_str() {
        "plan" => format!("Install {lv}: the plan"),
        "apply" => format!("Install {lv}"),
        _ => format!("Uninstall {lv}"),
    };
    egui::Window::new(title)
        .id(egui::Id::new("export-install"))
        .open(&mut open)
        .resizable(true)
        .default_width(760.0)
        .default_height(560.0)
        .show(ctx, |ui| {
            if w.install.busy() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(match w.install.step.as_str() {
                        "plan" => "Checking the stage and building the plan (nothing is written)...",
                        "apply" => "Installing...",
                        _ => "Uninstalling...",
                    });
                });
            }
            egui::ScrollArea::vertical()
                .id_salt("install-text")
                .max_height(420.0)
                .show(ui, |ui| {
                    for l in &w.install.text {
                        ui.monospace(l);
                    }
                });
            ui.separator();
            if w.install.busy() {
                return;
            }
            match (w.install.step.as_str(), w.install.ok()) {
                ("plan", true) => {
                    ui.label(
                        "Close the game before installing. The install writes the files listed above (backups first).",
                    );
                    let why = if exporting {
                        Some(WAIT_EXPORT.to_owned())
                    } else {
                        outdated.clone()
                    };
                    if let Some(why) = &why {
                        ui.colored_label(ui.visuals().warn_fg_color, why);
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(why.is_none(), egui::Button::new("Install now"))
                            .on_disabled_hover_text(why.clone().unwrap_or_default())
                            .clicked()
                            && let Some(s) = &stage
                        {
                            ask = Some(("apply", s.display().to_string()));
                        }
                        if ui.button("Cancel").clicked() {
                            w.install.open = false;
                        }
                    });
                }
                ("confirm-revert", _) => {
                    if exporting {
                        ui.colored_label(ui.visuals().warn_fg_color, WAIT_EXPORT);
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!exporting && !lv.is_empty(), egui::Button::new("Uninstall now"))
                            .on_disabled_hover_text(WAIT_EXPORT)
                            .clicked()
                        {
                            ask = Some(("revert", lv.clone()));
                        }
                        if ui.button("Cancel").clicked() {
                            w.install.open = false;
                        }
                    });
                }
                (_, ok) => {
                    ui.label(if ok {
                        "Done."
                    } else {
                        "It did not run through: see the lines above (nothing is half written: every check runs first)."
                    });
                    if ui.button("Close").clicked() {
                        w.install.open = false;
                    }
                }
            }
        });
    if !open {
        w.install.open = false;
    }
    if let (Some((step, target)), Some(o)) = (ask, w.opfor.clone()) {
        // checked again at the click (an export may have begun this frame)
        if w.running.is_some() {
            w.install.text.push(WAIT_EXPORT.into());
        } else {
            w.install.run(&o, step, &target);
        }
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
            ..Identity::default()
        });
        let c = checks(&s, None, false, None);
        assert!(c.iter().any(|x| x.starts_with("Save the project")));
        assert!(c.iter().any(|x| x.contains("may hold only the letters A-Z")));
        assert!(c.iter().any(|x| x.contains("needs the project's target")));
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
