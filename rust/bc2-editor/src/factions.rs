//! Which faction each team is, as the level's own data says (`EDITOR_ARCHITECTURE.md` 3.10, "Decisions on G1's first
//! version": team labels name the side's faction per level and content profile, not T1 / T2).
//!
//! The source is the level partition's `LevelData.LevelDescription`: `FactionTeam1`, `FactionTeam2` (the game's
//! `Faction` enum: `FactionUS`, `FactionRUS`, `FactionNeutral`) and `AttackerTeam` - the fields the stock-MP AI tool
//! reads (`bfbc2-stockmp-ai/levelmodel.py`, `bc2-stockmp`'s `Model::factions`). The enum has no value of its own for
//! Vietnam's or the Pacific War's second side: DICE's Vietnam levels say `FactionRUS` for the North Vietnamese
//! (their kit art is `vietnam/vc_*`), and the Pacific War levels converted from BF1943 say it for Japan. So the
//! label is the faction read through the level's content profile: `FactionRUS` is RU on a BFBC2 level, NVA on a
//! Vietnam level and JP on a WW2 level.
//!
//! Where the data says nothing - no level description, or `FactionNeutral` for both teams (the campaign, squad
//! deathmatch) - the labels stay T1 / T2 and [`Teams::source`] says why, for the properties panel.

use bc2_formats::dbx::{self, DbxTree};

/// A level's content profile (the top bar's badge; the WW2 profile hides modern kits, 3.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    /// Bad Company 2's own levels.
    Bfbc2,
    /// The Vietnam expansion (`nam_*`).
    Vietnam,
    /// Pacific War, the WW2 levels converted from Battlefield 1943.
    Ww2,
}

/// The Pacific War levels (OPFOR's WW2 subproject: `game\Package\levels\<name>`, converted from BF1943).
pub const WW2_LEVELS: [&str; 4] = ["coral_sea", "guadal_canal", "iwo_jima_s", "wake_island_s"];

impl Profile {
    /// The profile of a level, by its name: `nam_*` is Vietnam; the Pacific War levels (and any `ww2_*` / `pw_*`
    /// level made for it) are WW2; everything else is BFBC2.
    #[must_use]
    pub fn of(lv: &str) -> Self {
        let lv = lv.to_lowercase();
        if lv.starts_with("nam_") {
            Self::Vietnam
        } else if lv.starts_with("ww2_") || lv.starts_with("pw_") || WW2_LEVELS.contains(&lv.as_str()) {
            Self::Ww2
        } else {
            Self::Bfbc2
        }
    }

    /// Its name in a sentence.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Bfbc2 => "BFBC2",
            Self::Vietnam => "Vietnam",
            Self::Ww2 => "WW2 (Pacific War)",
        }
    }

    /// The badge's text.
    #[must_use]
    pub fn badge(self) -> &'static str {
        match self {
            Self::Bfbc2 => "BFBC2",
            Self::Vietnam => "VIETNAM",
            Self::Ww2 => "WW2 - PACIFIC WAR",
        }
    }
}

/// One side: its short label (beside its colour, never the colour alone) and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Side {
    /// `US`, `RU`, `NVA`, `JP`, or `T1` / `T2` where the data says nothing.
    pub label: String,
    /// `United States`, `Russia` ... or `Team 1` / `Team 2`.
    pub name: String,
    /// The faction value the level's data has (`FactionUS`), if any.
    pub faction: Option<String>,
}

/// The two teams of a level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Teams {
    /// Team 1.
    pub team1: Side,
    /// Team 2.
    pub team2: Side,
    /// `AttackerTeam` as 1 or 2 (rush and assault modes), `None` for neutral or unsaid.
    pub attacker: Option<u8>,
    /// Where the labels come from, in words (the properties panel says it).
    pub source: String,
    /// Whether the labels come from the level's data (false: the T1 / T2 fallback).
    pub from_data: bool,
}

/// The label and name of a faction value on a level of `profile`, or `None` for `FactionNeutral` and values the
/// game does not have.
#[must_use]
pub fn faction_side(profile: Profile, faction: &str) -> Option<(&'static str, &'static str)> {
    match (faction, profile) {
        ("FactionUS", _) => Some(("US", "United States")),
        ("FactionRUS", Profile::Bfbc2) => Some(("RU", "Russia")),
        ("FactionRUS", Profile::Vietnam) => Some(("NVA", "North Vietnamese Army")),
        ("FactionRUS", Profile::Ww2) => Some(("JP", "Japan")),
        _ => None,
    }
}

fn fallback(team: u8) -> Side {
    Side {
        label: format!("T{team}"),
        name: format!("Team {team}"),
        faction: None,
    }
}

impl Teams {
    /// T1 / T2, with the reason.
    #[must_use]
    pub fn unknown(why: &str) -> Self {
        Self {
            team1: fallback(1),
            team2: fallback(2),
            attacker: None,
            source: why.to_owned(),
            from_data: false,
        }
    }

    /// The teams of a level from its partition's tree.
    #[must_use]
    pub fn from_tree(tree: &DbxTree<'_>, profile: Profile) -> Self {
        let Some(level) = tree
            .instances()
            .find(|n| n.attr("type").is_some_and(|t| t.ends_with(".LevelData")))
        else {
            return Self::unknown("The level partition has no LevelData; teams are shown as T1 and T2.");
        };
        let Some(desc) = level.child("LevelDescription") else {
            return Self::unknown("The level has no level description; teams are shown as T1 and T2.");
        };
        let text = |name: &str| {
            desc.child(name)
                .filter(|c| c.kind() == dbx::TEXT)
                .map(|c| c.text().to_owned())
                .filter(|t| !t.is_empty())
        };
        let attacker = match text("AttackerTeam").as_deref() {
            Some("Team1") => Some(1),
            Some("Team2") => Some(2),
            _ => None,
        };
        let factions = [text("FactionTeam1"), text("FactionTeam2")];
        let sides: Vec<Option<Side>> = factions
            .iter()
            .map(|f| {
                let f = f.as_deref()?;
                faction_side(profile, f).map(|(label, name)| Side {
                    label: label.to_owned(),
                    name: name.to_owned(),
                    faction: Some(f.to_owned()),
                })
            })
            .collect();
        let said = |k: usize| factions.get(k).cloned().flatten().unwrap_or_else(|| "nothing".into());
        if let (Some(team1), Some(team2)) = (sides.first().cloned().flatten(), sides.get(1).cloned().flatten()) {
            Self {
                source: format!(
                    "From the level description: FactionTeam1 {}, FactionTeam2 {}, read as a {} level.",
                    said(0),
                    said(1),
                    profile.word()
                ),
                team1,
                team2,
                attacker,
                from_data: true,
            }
        } else {
            let mut t = Self::unknown(&format!(
                "The level description names no faction (FactionTeam1 {}, FactionTeam2 {}); teams are shown as T1 \
                 and T2.",
                said(0),
                said(1)
            ));
            t.attacker = attacker;
            t
        }
    }

    /// The teams of a level from its partition's bytes (a binary DBX).
    #[must_use]
    pub fn from_partition(data: &[u8], profile: Profile) -> Self {
        match dbx::parse_tree(data) {
            Ok(tree) => Self::from_tree(&tree, profile),
            Err(e) => Self::unknown(&format!(
                "The level partition could not be read ({e}); teams are shown as T1 and T2."
            )),
        }
    }

    /// A team's side: 1 or 2 (`None` for neutral and anything else).
    #[must_use]
    pub fn side(&self, team: u8) -> Option<&Side> {
        match team {
            1 => Some(&self.team1),
            2 => Some(&self.team2),
            _ => None,
        }
    }

    /// A team's short label beside its colour: the side's label, or `NEU` for neutral.
    #[must_use]
    pub fn label(&self, team: u8) -> &str {
        self.side(team).map_or("NEU", |s| s.label.as_str())
    }

    /// A team's name in the properties panel: `United States (US)`, `Team 1 (T1)`, `Neutral`.
    #[must_use]
    pub fn describe(&self, team: u8) -> String {
        let Some(s) = self.side(team) else {
            return "Neutral".into();
        };
        if !self.from_data {
            return format!("{} ({})", s.name, s.label);
        }
        let role = match self.attacker {
            Some(a) if a == team => ", attackers",
            Some(_) => ", defenders",
            None => "",
        };
        format!("{} ({}) - team {team}{role}", s.name, s.label)
    }
}

/// The level partition of a level straight from a bundle (`level-00.fbrb`), read in memory: the entry
/// `levels/<lv>.dbx`, any case. For levels NavKit has not unpacked (the Package-only Vietnam and WW2 levels).
///
/// # Errors
///
/// The bundle cannot be read or has no such entry.
pub fn partition_from_bundle(bundle: &std::path::Path, lv: &str) -> Result<Vec<u8>, String> {
    let mut f = std::io::BufReader::new(std::fs::File::open(bundle).map_err(|e| format!("{}: {e}", bundle.display()))?);
    let name = format!("levels/{}.dbx", lv.to_lowercase());
    bc2_formats::fbrb::read(&mut f, &[name.as_str()])
        .map_err(|e| format!("{}: {name}: {e}", bundle.display()))?
        .into_iter()
        .next()
        .ok_or_else(|| format!("{}: no {name}", bundle.display()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn profiles_and_sides() {
        assert_eq!(Profile::of("MP_005"), Profile::Bfbc2);
        assert_eq!(Profile::of("nam_mp_002cq"), Profile::Vietnam);
        assert_eq!(Profile::of("Wake_Island_S"), Profile::Ww2);
        assert_eq!(Profile::of("ww2_custom"), Profile::Ww2);
        assert_eq!(faction_side(Profile::Bfbc2, "FactionRUS"), Some(("RU", "Russia")));
        assert_eq!(faction_side(Profile::Vietnam, "FactionRUS").unwrap().0, "NVA");
        assert_eq!(faction_side(Profile::Ww2, "FactionRUS").unwrap().0, "JP");
        assert_eq!(faction_side(Profile::Ww2, "FactionUS").unwrap().0, "US");
        assert_eq!(faction_side(Profile::Bfbc2, "FactionNeutral"), None);
        let t = Teams::unknown("x");
        assert_eq!((t.label(1), t.label(2), t.label(0)), ("T1", "T2", "NEU"));
        assert_eq!(t.describe(1), "Team 1 (T1)");
        assert_eq!(t.describe(2), "Team 2 (T2)");
        assert_eq!(t.describe(0), "Neutral");
        assert!(
            Teams::from_partition(b"not a dbx", Profile::Bfbc2)
                .source
                .contains("could not be read")
        );
    }
}
