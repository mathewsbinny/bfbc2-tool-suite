//! The Export window's **Weather** section (round VW2): the weather the exported level starts in, a choice of the
//! project's log ([`bc2_levelwrite::Op::SetWeather`]) that `bc2_levelwrite::weather` writes into the level.
//!
//! - **One of the level's states** (VW2a): any state of the level's weather system; the export sets the settings'
//!   `ActiveState` and the spawn-time switch to it.
//! - **Another level's state** (VW2b): appended to the level's system (the level keeps its sky), or the donor's
//!   whole system swapped in (its sky).
//! - **Night** (VW2c): DICE's one night state (`MP_002`'s `ons_mp_002_night`, the Onslaught Package's) appended, the
//!   reflection cube scaled down and the level's sky panorama darkened; lamps are placed with the Objects
//!   workspace's tools, never by the export.
//!
//! Night and rain are off by default and marked "not yet seen in game": the export builds them only once "Build it
//! anyway" is ticked (in-game tests are paused).

use bc2_levelwrite::export::Game;
use bc2_levelwrite::weather::{self as wx, Borrow, Night, WeatherChoice};
use eframe::egui;

/// What the form chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// The level's weather as it is (no operation).
    #[default]
    AsItIs,
    /// One of the level's states.
    Own,
    /// Another level's state appended (the level's sky).
    Append,
    /// Another level's whole system (its sky).
    Swap,
    /// The night look.
    Night,
}

impl Kind {
    /// Every choice, in the menu's order.
    pub const ALL: [Self; 5] = [Self::AsItIs, Self::Own, Self::Append, Self::Swap, Self::Night];

    /// Its name in the menu.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::AsItIs => "As the level has it",
            Self::Own => "One of the level's states",
            Self::Append => "Another level's state (this level's sky)",
            Self::Swap => "Another level's weather system (its sky)",
            Self::Night => "Night (not yet seen in game)",
        }
    }
}

/// A level's states for the form: (state partitions, `ActiveState`, the spawn switch's state).
pub type States = Result<(Vec<String>, usize, Option<usize>), String>;

/// A level's weather states, as the export reads them (`w`: its partition as the project leaves it, else the
/// level's own).
///
/// # Errors
/// The level's partition, its bundles or its weather do not read.
pub fn states_of(game: &Game, lv: &str, w: Option<&bc2_levelwrite::LevelWriter>) -> States {
    let parsed;
    let w = if let Some(w) = w {
        w
    } else {
        parsed = bc2_levelwrite::LevelWriter::parse(
            &bc2_levelwrite::export::level_partition(game, lv).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        &parsed
    };
    let mut b = game.loaded(lv, "level");
    b.extend(game.startup());
    let live = game.live(&b).map_err(|e| e.to_string())?;
    let lw = wx::level_weather(w, &live).map_err(|e| e.0)?;
    Ok((lw.states, lw.active, lw.spawn.map(|x| x.1)))
}

fn short(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// The section's state.
#[derive(Debug, Clone, Default)]
pub struct WeatherForm {
    /// The choice.
    pub kind: Kind,
    /// The state chosen (of the level's own, or of the donor's).
    pub state: usize,
    /// The donor level (another level's state).
    pub donor: String,
    /// "Build it anyway": a night or rain export turned on.
    pub unseen_ok: bool,
    /// The level's states (the level they were read for).
    pub own: Option<(String, States)>,
    /// The donor's states (the donor they were read for).
    pub donor_states: Option<(String, States)>,
    /// The form was filled from the project's log.
    synced: bool,
}

impl WeatherForm {
    /// The form filled from the project's choice (once).
    pub fn sync(&mut self, choice: Option<&WeatherChoice>) {
        if self.synced {
            return;
        }
        self.synced = true;
        let Some(c) = choice else { return };
        self.state = c.state;
        self.unseen_ok = c.unseen_ok;
        self.kind = match (&c.borrow, &c.night) {
            (_, Some(_)) => Kind::Night,
            (Some(b), None) => {
                self.donor.clone_from(&b.donor);
                if b.swap { Kind::Swap } else { Kind::Append }
            }
            (None, None) => Kind::Own,
        };
    }

    /// The choice the form makes (`None`: the level's weather as it is).
    ///
    /// # Errors
    /// The states are not read, or no state is chosen.
    pub fn choice(&self) -> Result<Option<WeatherChoice>, String> {
        let path = |list: &Option<(String, States)>| -> Result<String, String> {
            match list {
                Some((_, Ok((p, _, _)))) => p
                    .get(self.state)
                    .cloned()
                    .ok_or_else(|| format!("the system has no state {}", self.state)),
                Some((_, Err(e))) => Err(e.clone()),
                None => Err("the states are not read yet".into()),
            }
        };
        let base = |name: &str| WeatherChoice {
            state: self.state,
            name: name.to_owned(),
            borrow: None,
            night: None,
            unseen_ok: self.unseen_ok,
        };
        Ok(match self.kind {
            Kind::AsItIs => None,
            Kind::Own => Some(base(short(&path(&self.own)?))),
            Kind::Append | Kind::Swap => {
                let p = path(&self.donor_states)?;
                Some(WeatherChoice {
                    borrow: Some(Borrow {
                        donor: self.donor.to_ascii_lowercase(),
                        state: p.clone(),
                        swap: self.kind == Kind::Swap,
                    }),
                    ..base(short(&p))
                })
            }
            Kind::Night => Some(WeatherChoice {
                night: Some(Night::default()),
                ..base(short(wx::NIGHT_TEMPLATE.1))
            }),
        })
    }
}

fn state_list(ui: &mut egui::Ui, id: &str, states: &States, chosen: &mut usize) {
    match states {
        Ok((list, active, spawn)) => {
            egui::ComboBox::from_id_salt(id)
                .selected_text(list.get(*chosen).map_or("(choose)", |p| short(p)))
                .show_ui(ui, |ui| {
                    for (k, p) in list.iter().enumerate() {
                        let mut t = format!("{k}: {}", short(p));
                        if *spawn == Some(k) {
                            t.push_str(" - spawn switch");
                        }
                        if *active == k {
                            t.push_str(" - ActiveState");
                        }
                        ui.selectable_value(chosen, k, t).on_hover_text(p.as_str());
                    }
                });
        }
        Err(e) => {
            ui.label(format!("Not read: {e}"));
        }
    }
}

/// The section (in the Export window's scroll area). Returns the choice to keep in the project when "Keep in
/// project" was clicked (`Some(None)`: the level's weather as it is).
pub fn section(
    ui: &mut egui::Ui,
    f: &mut WeatherForm,
    game: Option<&Game>,
    base: &str,
    project_choice: Option<&WeatherChoice>,
    partition: Option<&bc2_levelwrite::LevelWriter>,
) -> Option<Option<WeatherChoice>> {
    f.sync(project_choice);
    ui.heading("Weather");
    ui.weak(
        "The weather the level starts in: the export sets the settings' ActiveState and the spawn-time switch (what \
         players see) to it. Night and rain are not yet seen in game: off unless you build them anyway.",
    );
    if let Some(g) = game
        && f.own.as_ref().is_none_or(|(lv, _)| lv != base)
    {
        f.own = Some((base.to_owned(), states_of(g, base, partition)));
    }
    let mut keep = None;
    egui::Grid::new("export-weather").num_columns(2).show(ui, |ui| {
        ui.label("Weather");
        egui::ComboBox::from_id_salt("export-weather-kind")
            .selected_text(f.kind.name())
            .show_ui(ui, |ui| {
                for k in Kind::ALL {
                    ui.selectable_value(&mut f.kind, k, k.name());
                }
            });
        ui.end_row();
        match f.kind {
            Kind::AsItIs => {}
            Kind::Own => {
                ui.label("State");
                if let Some((_, s)) = &f.own {
                    state_list(ui, "export-weather-own", s, &mut f.state);
                }
                ui.end_row();
            }
            Kind::Append | Kind::Swap => {
                ui.label("Donor level");
                ui.text_edit_singleline(&mut f.donor)
                    .on_hover_text("A level of the game copy (mp_002, sp_03, mp_008cq ...)");
                ui.end_row();
                let donor = f.donor.trim().to_ascii_lowercase();
                if let Some(g) = game
                    && !donor.is_empty()
                    && f.donor_states.as_ref().is_none_or(|(lv, _)| *lv != donor)
                    && ui.button("Read its states").clicked()
                {
                    f.donor_states = Some((donor.clone(), states_of(g, &donor, None)));
                    f.state = 0;
                }
                ui.label("Its state");
                match &f.donor_states {
                    Some((lv, s)) if *lv == donor => state_list(ui, "export-weather-donor", s, &mut f.state),
                    _ => {
                        ui.weak("Read the donor's states first");
                    }
                }
                ui.end_row();
            }
            Kind::Night => {
                ui.label("Template");
                ui.label(format!(
                    "{} ({}'s system); the reflection cube x {}, the sky panorama x {}",
                    wx::NIGHT_TEMPLATE.1,
                    wx::NIGHT_TEMPLATE.0,
                    wx::NIGHT_ENVMAP_SCALE,
                    wx::NIGHT_PANORAMA
                ));
                ui.end_row();
            }
        }
    });
    let choice = f.choice();
    if let Ok(Some(c)) = &choice
        && c.unseen()
    {
        ui.checkbox(&mut f.unseen_ok, "Build it anyway (not yet seen in game)");
    }
    ui.horizontal(|ui| {
        let now = f.choice();
        let differs = now.as_ref().is_ok_and(|c| c.as_ref() != project_choice);
        if ui
            .add_enabled(now.is_ok() && differs, egui::Button::new("Keep in project"))
            .on_hover_text("The weather becomes an operation of the project's log (undoable)")
            .clicked()
            && let Ok(c) = now
        {
            keep = Some(c);
        }
        match (&choice, project_choice) {
            (Err(e), _) => {
                ui.weak(e.as_str());
            }
            (_, Some(c)) => {
                ui.weak(format!("The project's: {}", c.describe()));
            }
            (_, None) => {
                ui.weak("The project keeps the level's own weather");
            }
        }
    });
    keep
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_form_makes_the_choice() {
        let mut f = WeatherForm::default();
        assert_eq!(f.choice(), Ok(None));
        f.kind = Kind::Own;
        assert!(f.choice().is_err());
        f.own = Some((
            "mp_002".into(),
            Ok((
                vec![
                    "weather/r_mp02/states/midday".into(),
                    "weather/r_mp02/states/ons_mp_002_dawn".into(),
                ],
                0,
                Some(0),
            )),
        ));
        f.state = 1;
        let c = f.choice().unwrap().unwrap();
        assert_eq!(
            (c.state, c.name.as_str(), c.borrow.is_none()),
            (1, "ons_mp_002_dawn", true)
        );
        f.kind = Kind::Swap;
        f.donor = "MP_002".into();
        f.donor_states = f.own.clone();
        let c = f.choice().unwrap().unwrap();
        assert_eq!(c.borrow.unwrap().state, "weather/r_mp02/states/ons_mp_002_dawn");
        f.kind = Kind::Night;
        let c = f.choice().unwrap().unwrap();
        assert!(c.unseen() && !c.unseen_ok && c.name == "ons_mp_002_night");
        let mut g = WeatherForm::default();
        g.sync(Some(&c));
        assert_eq!(g.kind, Kind::Night);
    }
}
