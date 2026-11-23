//! Round E4's panel: the Gameplay workspace's tools and the round's rules ([`crate::gameplay`]).
//!
//! - **Tools**: what the Gameplay tool's click does on the views (add a flag, a spawn point, an M-COM to a Rush
//!   stage, or carry a flag, base, spawner or M-COM with what moves with it) and the team new objects take (the asset
//!   browser's team picker); Delete takes the selection with its units.
//! - **Capture points**: the level's bases and flags with their letters, teams and spawn points; a row selects one;
//!   the selected ones' team set (one operation).
//! - **Rush stages**: each stage's M-COMs, an M-COM added to it with the tool; a whole stage added after the last
//!   with the tool (round E4-rest) or taken out with what belongs to it.
//! - **The selected flag's name** (round E4-rest): its name in DICE's form (`E: Harbor`); the export makes the deploy
//!   screen's text from it.
//! - **Round rules**: the mode's rules as the level holds them, each with DICE's values to choose from and a value of
//!   one's own, DICE's for the mode and era by default; applied as one operation of the log.

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use bc2_levelwrite::gameplay::{self as lwgameplay, Rule, Rules, Team};

use crate::gameplay::GameplayKind;

/// The panel's state between frames: the rules being edited, and the log length they were read at.
#[derive(Debug, Default)]
pub struct GameplayPanel {
    /// The rules being edited (read from the level when the log changes).
    pub rules: Option<Rules>,
    /// The log (operations, redo) the rules were read at.
    read_at: Option<(usize, usize)>,
    /// The selected flag's name being edited, and the flag (round E4-rest).
    pub name: Option<(usize, String)>,
    /// A button's request made from outside the panel (the interface tests: what a click on it asks), done the next
    /// time the panel is drawn.
    pub request: Option<PanelRequest>,
}

/// What one of the panel's buttons asks (round E4-rest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelRequest {
    /// "Remove stage" on stage `k` (play order, from 0).
    RemoveStage(usize),
    /// "Rename" of a flag (its guid) to a name.
    Rename(String, String),
}

/// What the panel's buttons asked for this frame.
#[derive(Default)]
struct Asked {
    kind: Option<GameplayKind>,
    select: Option<usize>,
    team: Option<Team>,
    apply: bool,
    delete: bool,
    remove_stage: Option<usize>,
    rename: Option<(String, String)>,
}

/// One capture point's row.
struct PointRow {
    k: usize,
    letter: String,
    name: String,
    team: Option<Team>,
    base: bool,
    points: usize,
}

fn rows(d: &Doc) -> Vec<PointRow> {
    let Some(w) = d.writer() else { return Vec::new() };
    let flags = lwgameplay::flags(w);
    let mut out = Vec::new();
    for e in w.d.of_type(b".CapturePointEntityData") {
        let g = String::from_utf8_lossy(w.d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase();
        let Some(k) = d.index(&g) else { continue };
        let text = |e: bc2_formats::dbx::edit::NodeId, f: &[u8]| {
            w.d.child(e, f)
                .and_then(|x| w.d.text(x))
                .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned())
        };
        let letter = lwgameplay::marker_of(w, &g)
            .and_then(|m| w.find(&m))
            .map_or_else(String::new, |m| text(m, b"Sid"));
        let points = lwgameplay::spawners(w, &g)
            .iter()
            .map(|s| lwgameplay::points(w, s).len())
            .sum();
        out.push(PointRow {
            k,
            letter,
            name: text(e, b"Name"),
            team: lwgameplay::team_of(w, &g),
            base: !flags.contains(&g),
            points,
        });
    }
    out
}

impl Tabs<'_> {
    #[allow(clippy::too_many_lines, reason = "the panel top to bottom")]
    pub(super) fn gameplay_panel(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        let p = self.palette;
        let m = self.metrics;
        let Some(d) = self.view.doc.as_ref() else {
            self.header(ui, "Gameplay", "");
            ui.label(RichText::new("Open or make a project to edit its gameplay.").color(p.ink_muted));
            return;
        };
        let rows = rows(d);
        let stages = crate::gameplay::rush_stages(d);
        let rules = crate::gameplay::rules(d);
        let log = (d.project.operations.len(), d.project.redo.len());
        if self.view.gameplay.read_at != Some(log) {
            self.view.gameplay.rules = rules.map(|r| r.1);
            self.view.gameplay.read_at = Some(log);
        }
        let flags = rows.iter().filter(|r| !r.base).count();
        let summary = match rules {
            Some((mode, _, _)) => format!(
                "{} · {flags} flag{} · {} stage{}",
                mode_name(mode),
                if flags == 1 { "" } else { "s" },
                stages.len(),
                if stages.len() == 1 { "" } else { "s" }
            ),
            None => "no mode layer".into(),
        };
        self.header(ui, "Gameplay", &summary);
        let tool_on = self.view.tool == Tool::Gameplay;
        let kind = self.view.tools.gameplay.kind;
        let editing = self.view.workspace == Workspace::Gameplay;
        let mut asked = Asked::default();
        match self.view.gameplay.request.take() {
            Some(PanelRequest::RemoveStage(k)) => asked.remove_stage = Some(k),
            Some(PanelRequest::Rename(g, n)) => asked.rename = Some((g, n)),
            None => {}
        }
        let mut team = self.view.browser.place_team;
        let selected: Vec<usize> = self.view.selection.clone();
        let sel_team = selected
            .iter()
            .filter_map(|k| rows.iter().find(|r| r.k == *k))
            .map(|r| r.team)
            .next()
            .flatten();
        let teams = self.view.teams.clone();
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_3 as i8))
                .show(ui, |ui| {
                    if !editing {
                        status_line(
                            ui,
                            &p,
                            "Information",
                            p.status_info,
                            "Switch to the Gameplay workspace to use the tools on the views.",
                        );
                    }
                    // the tool
                    ui.label(RichText::new("Tool").text_style(style("body-strong")).color(p.ink));
                    ui.horizontal_wrapped(|ui| {
                        for k in [
                            GameplayKind::Flag,
                            GameplayKind::SpawnPoint,
                            GameplayKind::Spawner,
                            GameplayKind::Carry,
                        ] {
                            if ui
                                .add_enabled(editing, egui::Button::selectable(tool_on && kind == k, k.name()))
                                .on_hover_text(k.hint())
                                .clicked()
                            {
                                asked.kind = Some(k);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Team").color(p.ink_muted));
                        for t in [1u8, 2, 0] {
                            let label = if t == 0 {
                                "Neutral".to_owned()
                            } else {
                                format!("Team {t} ({})", teams.label(t))
                            };
                            if ui
                                .selectable_label(team == t, label)
                                .on_hover_text("New flags and spawns take this team")
                                .clicked()
                            {
                                team = t;
                            }
                        }
                    });
                    if tool_on {
                        ui.label(RichText::new(kind.hint()).color(p.ink_muted));
                    }
                    ui.add_space(m.space_3);
                    // the capture points
                    ui.label(
                        RichText::new(format!("Capture points ({flags} flags)"))
                            .text_style(style("body-strong"))
                            .color(p.ink),
                    );
                    egui::Grid::new("gameplay_points")
                        .num_columns(4)
                        .spacing([m.space_3, m.space_1])
                        .show(ui, |ui| {
                            for r in &rows {
                                let what = if r.base { "Base" } else { "Flag" };
                                let label = if r.letter.is_empty() {
                                    r.name.clone()
                                } else {
                                    format!("{}  {}", r.letter, r.name)
                                };
                                if ui
                                    .selectable_label(selected.contains(&r.k), RichText::new(label).color(p.ink))
                                    .clicked()
                                {
                                    asked.select = Some(r.k);
                                }
                                ui.label(RichText::new(what).color(p.ink_muted));
                                let (word, colour) = match r.team {
                                    Some(Team::Team1) => (format!("Team 1 ({})", teams.label(1)), p.team_1),
                                    Some(Team::Team2) => (format!("Team 2 ({})", teams.label(2)), p.team_2),
                                    _ => ("Neutral".to_owned(), p.team_neutral),
                                };
                                ui.label(RichText::new(word).color(colour));
                                ui.label(RichText::new(format!("{} spawn points", r.points)).color(p.ink_muted));
                                ui.end_row();
                            }
                        });
                    if sel_team.is_some() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Selected: owned by").color(p.ink_muted));
                            for (t, word) in [
                                (Team::Team1, "Team 1"),
                                (Team::Team2, "Team 2"),
                                (Team::TeamNeutral, "Neutral"),
                            ] {
                                if ui
                                    .add_enabled(editing, egui::Button::selectable(sel_team == Some(t), word))
                                    .clicked()
                                {
                                    asked.team = Some(t);
                                }
                            }
                        });
                    }
                    // the selected flag's name (round E4-rest)
                    if let [one] = selected.as_slice()
                        && let Some(r) = rows.iter().find(|r| r.k == *one && !r.base)
                    {
                        let mut edit = match self.view.gameplay.name.take() {
                            Some((k, t)) if k == r.k => t,
                            _ => r.name.clone(),
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Name").color(p.ink_muted));
                            ui.add_enabled(editing, egui::TextEdit::singleline(&mut edit).desired_width(160.0))
                                .on_hover_text(
                                    "The flag's name (DICE's form: \"E: Harbor\"); the export makes the deploy                                      screen's text from it",
                                );
                            if ui
                                .add_enabled(editing && edit.trim() != r.name, egui::Button::new("Rename"))
                                .clicked()
                                && let Some(g) = self.view.doc.as_ref().and_then(|d| d.guid(r.k))
                            {
                                asked.rename = Some((g.to_owned(), edit.clone()));
                            }
                        });
                        self.view.gameplay.name = Some((r.k, edit));
                    }
                    if ui
                        .add_enabled(
                            editing && !selected.is_empty(),
                            egui::Button::new("Delete with its unit"),
                        )
                        .on_hover_text(
                            "Deletes the selected flags, spawners, spawn points or M-COMs with what belongs to them \
                             (Delete)",
                        )
                        .clicked()
                    {
                        asked.delete = true;
                    }
                    // the Rush stages
                    if !stages.is_empty() {
                        ui.add_space(m.space_3);
                        ui.label(
                            RichText::new("Rush stages")
                                .text_style(style("body-strong"))
                                .color(p.ink),
                        );
                        for (k, s) in stages.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "Stage {}: {} M-COM{}",
                                        k + 1,
                                        s.len(),
                                        if s.len() == 1 { "" } else { "s" }
                                    ))
                                    .color(p.ink),
                                );
                                let mine = GameplayKind::Mcom(k);
                                if ui
                                    .add_enabled(
                                        editing,
                                        egui::Button::selectable(tool_on && kind == mine, "Add M-COM"),
                                    )
                                    .on_hover_text(mine.hint())
                                    .clicked()
                                {
                                    asked.kind = Some(mine);
                                }
                                if ui
                                    .add_enabled(editing && stages.len() > 1, egui::Button::new("Remove stage"))
                                    .on_hover_text(
                                        "Takes the stage out with its M-COMs, bases, spawns, combat areas and                                          counters; the stage before hands on to the next (one undo step)",
                                    )
                                    .clicked()
                                {
                                    asked.remove_stage = Some(k);
                                }
                            });
                        }
                        let add = GameplayKind::Stage;
                        if ui
                            .add_enabled(
                                editing && stages.len() > 1,
                                egui::Button::selectable(tool_on && kind == add, "Add stage after the last"),
                            )
                            .on_hover_text(add.hint())
                            .clicked()
                        {
                            asked.kind = Some(add);
                        }
                    }
                    // the round's rules
                    ui.add_space(m.space_3);
                    ui.label(
                        RichText::new("Round rules")
                            .text_style(style("body-strong"))
                            .color(p.ink),
                    );
                    let Some((mode, now, stock)) = rules else {
                        ui.label(
                            RichText::new("The level has no mode layer: choose the export's mode first.")
                                .color(p.ink_muted),
                        );
                        return;
                    };
                    let profile = self.view.doc.as_ref().map_or_default(|d| d.project.target_profile());
                    let mut edited = self.view.gameplay.rules.unwrap_or(now);
                    for c in lwgameplay::choices(mode, profile) {
                        let value = edited.get(c.rule);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(c.name).color(p.ink));
                            let shown = value.map_or_else(|| "not set".to_owned(), |v| format!("{v}"));
                            egui::ComboBox::from_id_salt(("rule", c.name))
                                .selected_text(shown)
                                .show_ui(ui, |ui| {
                                    for (i, v) in c.dice.iter().enumerate() {
                                        let word = if i == 0 && stock.get(c.rule) == Some(*v) {
                                            format!("{v} (DICE's default)")
                                        } else {
                                            format!("{v} (DICE's)")
                                        };
                                        if ui.selectable_label(value == Some(*v), word).clicked() {
                                            edited.set(c.rule, Some(*v));
                                        }
                                    }
                                });
                            let mut own = value.unwrap_or(c.range[0]);
                            let whole = !matches!(c.rule, Rule::LossPerMin | Rule::CaptureS);
                            let drag = egui::DragValue::new(&mut own)
                                .range(c.range[0]..=c.range[1])
                                .speed(if whole { 1.0 } else { 0.5 })
                                .max_decimals(usize::from(!whole))
                                .suffix(format!(" {}", c.unit));
                            if ui
                                .add_enabled(editing, drag)
                                .on_hover_text("A value of your own (outside DICE's: the export warns)")
                                .changed()
                            {
                                edited.set(c.rule, Some(own));
                            }
                            if value.is_some_and(|v| !c.dice.contains(&v)) {
                                ui.label(RichText::new("not DICE's").color(p.status_warn));
                            }
                        });
                    }
                    self.view.gameplay.rules = Some(edited);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(editing && edited != now, egui::Button::new("Apply rules"))
                            .on_hover_text("Sets the rules on every counter and flag that holds them: one undo step")
                            .clicked()
                        {
                            asked.apply = true;
                        }
                        if ui
                            .add_enabled(edited != stock, egui::Button::new("DICE's defaults"))
                            .on_hover_text(format!(
                                "DICE's rules for {} on the stock levels: {}",
                                mode_name(mode),
                                stock.describe()
                            ))
                            .clicked()
                        {
                            self.view.gameplay.rules = Some(now.with_defaults(stock));
                        }
                        if ui.add_enabled(edited != now, egui::Button::new("Revert")).clicked() {
                            self.view.gameplay.rules = Some(now);
                        }
                    });
                    if now == stock {
                        ui.label(RichText::new(format!("DICE's rules: {}", stock.describe())).color(p.status_ok));
                    } else {
                        ui.label(RichText::new(format!("The level's rules: {}", now.describe())).color(p.ink_muted));
                    }
                });
        });
        self.view.browser.place_team = team;
        let gpu = self.gpu;
        if let Some(k) = asked.kind {
            self.view.tools.gameplay.kind = k;
            self.view.tool = Tool::Gameplay;
            self.view.tools.choose(Tool::Gameplay);
            k.hint().clone_into(&mut self.view.message);
        }
        if let Some(k) = asked.select {
            self.view.select(gpu, Some(k), true);
        }
        if let Some(t) = asked.team
            && let Some(d) = self.view.doc.as_ref()
            && let Some(w) = d.writer()
        {
            let items: Vec<lwgameplay::TeamSet> = selected
                .iter()
                .filter_map(|&k| d.guid(k))
                .filter(|g| lwgameplay::team_field(w, g).is_ok())
                .map(|g| lwgameplay::TeamSet {
                    guid: g.to_owned(),
                    from: lwgameplay::team_of(w, g),
                    to: t,
                })
                .filter(|x| x.from != Some(t))
                .collect();
            if !items.is_empty() {
                self.view.commit(gpu, &palette, Op::SetTeam { items }, "");
            }
        }
        if asked.apply
            && let Some((_, now, _)) = self.view.doc.as_ref().and_then(crate::gameplay::rules)
            && let Some(to) = self.view.gameplay.rules
        {
            let changed = to.changed_from(now);
            self.view.commit(
                gpu,
                &palette,
                Op::SetRules {
                    from: Some(now),
                    to: changed,
                },
                "",
            );
            self.view.gameplay.read_at = None;
        }
        if asked.delete {
            self.view.edit_action(gpu, &palette, Action::Delete);
        }
        if let Some(k) = asked.remove_stage
            && let Some(d) = self.view.doc.as_ref()
        {
            match crate::gameplay::remove_stage_op(d, k) {
                Ok(op) => {
                    self.view
                        .commit(gpu, &palette, op, &format!("Remove Rush stage {}", k + 1));
                }
                Err(why) => self.view.message = format!("Stage {} not removed: {why}", k + 1),
            }
        }
        if let Some((g, name)) = asked.rename
            && let Some(d) = self.view.doc.as_ref()
        {
            match crate::gameplay::rename_op(d, &g, &name) {
                Ok(op) => {
                    self.view.commit(gpu, &palette, op, "Rename flag");
                    self.view.gameplay.name = None;
                }
                Err(why) => self.view.message = format!("Not renamed: {why}"),
            }
        }
    }
}

/// A mode's name in the panel.
fn mode_name(m: bc2_levelwrite::placement::Mode) -> &'static str {
    use bc2_levelwrite::placement::Mode;
    match m {
        Mode::Conquest => "Conquest",
        Mode::Rush => "Rush",
        Mode::SquadRush => "Squad Rush",
        Mode::SquadDeathmatch => "Squad Deathmatch",
    }
}
