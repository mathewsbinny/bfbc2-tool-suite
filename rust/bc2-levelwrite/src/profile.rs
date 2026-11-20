//! Content profiles (round E3b, `EDITOR_ARCHITECTURE.md` 3.12): what an export takes from the set of maps it is made
//! for - BFBC2, BFBC1, Vietnam or WW2 (Pacific War) - read from that set's stock levels in the game copy, never from
//! a hand-kept list.
//!
//! - **The set** of a level is what its bundles load ([`Profile::of_stack`], the editor's start-screen recognition,
//!   round G3): `ww2_common` WW2, `nam_common` Vietnam, a `bc1_` level of `game\Package` BFBC1, else BFBC2. Its
//!   multiplayer levels are those whose description (`startup-00`) names a multiplayer game mode.
//! - **The common bundles** of each kind (level, terrain, ingame, loader) are the `_common` targets every
//!   multiplayer level of the set loads, in their order: BFBC2 `mp_common`; BFBC1 `mp2_common`, `mp_common`; Vietnam
//!   `nam_common`, `mp_common`; WW2 `ww2_common`, `mp_common` (on OPFOR's game copy).
//! - **The donors** are the set's levels of the export's mode (Conquest: `conquest`, Vietnam's and WW2's
//!   `namconquest`). The mode's layer, the description - game mode, factions, kit pictures, licences, matchmaking
//!   id, start delay, the level's memory - come from the donor chosen (the export's `layer.donor`). The default donor
//!   is the first, by name, whose layer names no partition outside the set's common bundles and the startup bundles.
//! - **The profile's level fields** are the `LevelData` fields every donor of a set has with one value, and the sets
//!   do not all share (on OPFOR's game copy: `SoundStates`, `sound/nam_soundstates` on Vietnam): an export takes the
//!   donor's.
//! - **The team labels** are the donor's `FactionTeam1` / `2` read through the profile (`bc2-editor`'s
//!   `factions::faction_side`: `FactionRUS` is RU, NVA or JP).
//! - **The announcer** (round E3c, [`Announcer`]) is the donor's: its own line banks and trigger sets (a WW2 level's;
//!   the other sets' are their commons'), its `vo/<lang>` blocks and manifest lines, its own voice-over bundles.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use bc2_formats::dbx::edit::{EditDbx, NodeId};
use serde::{Deserialize, Serialize};

use crate::export::{ExportError, Found, Game, KINDS, Result, read_found};
use crate::mpconvert::{self, CapturePoint};
use crate::project::Profile;
use crate::write::LevelWriter;

/// A stock level of the export's mode in a set: a donor for the mode's layer and the description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Donor {
    /// The level (lower case: `mp_009cq`).
    pub level: String,
    /// Its description's `GameMode` (`gamemodes/conquest/<guid>`).
    pub game_mode: String,
    /// Its description's `FactionTeam1`, `FactionTeam2`.
    pub factions: [String; 2],
    /// Its description's kit pictures: `Team1 KIT_ASSAULT awards/images/kitselection/us_assault_wood/<guid>`.
    pub kits: Vec<String>,
    /// Its capture points.
    pub capture_points: Vec<CapturePoint>,
    /// The partitions its layer names that none of the set's common bundles nor the startup bundles have.
    pub outside: Vec<String>,
    /// Its `LevelData`'s own fields (name -> value: a text, or a reference).
    #[serde(skip)]
    pub fields: BTreeMap<String, String>,
    /// Its announcer's voice-over data (round E3c).
    #[serde(default)]
    pub announcer: Announcer,
}

/// The partition prefixes of the announcer's data: the line banks (`sound/vo/vo_common`, `vo_en` ...) and the trigger
/// sets (`sound/vo_triggers/...`) the mode layer's `VoiceOvers` name.
pub const ANNOUNCER_PREFIXES: [&str; 2] = ["sound/vo/", "sound/vo_triggers/"];

/// Whether an entry name is announcer data ([`ANNOUNCER_PREFIXES`]).
#[must_use]
pub fn is_announcer(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ANNOUNCER_PREFIXES.iter().any(|p| n.starts_with(p))
}

/// A level's announcer voice-over data as the game copy has it (round E3c), read from the level's own bundles and
/// its `package.mft` / `bundleManifest` entries. On OPFOR's game copy:
///
/// - **BFBC2** (`mp_009cq`): no line bank or Conquest trigger set of its own (they are `mp_common`'s; OPFOR's `mp_001`
///   adds two Squad Deathmatch trigger sets); seven `vo/<lang>` blocks targeting `mp1_common`'s voice-over bundles,
///   the level's own absent (DICE's `mp_sp_005cq`, made with the DLC's `mp2_common`, targets BFBC1's);
/// - **BFBC1** (`bc1_*`): `mp2_common`'s banks (its copies come before `mp_common`'s); seven blocks targeting
///   `mp2_common`'s then `mp_common`'s voice-over bundles, the level's own absent;
/// - **Vietnam** (`nam_mp_*`): `nam_common`'s, the blocks targeting `nam_common`'s then `mp_common`'s;
/// - **WW2** (OPFOR's Pacific War levels): the level's own banks and trigger sets (BF1943's lines merged into
///   BFBC2's banks, the level's `GameInfo` set), its own `vo/en-00` bundle of BF1943's waves in `Package`, one
///   `vo/en` block targeting `mp1_common`'s English bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Announcer {
    /// Its own level bundles' live announcer entries ([`is_announcer`]), as named there, by name (an export copies
    /// those its layer reaches).
    pub partitions: Vec<String>,
    /// Its `vo/<lang>` blocks of `package.mft`, in order: (`vo/en`, the targets).
    pub blocks: Vec<(String, Vec<String>)>,
    /// The `vo/<lang>` bundles `bundleManifest` names for it (`vo/en`).
    pub manifest: Vec<String>,
    /// Its own `vo/<lang>` bundles that are there: (`vo/en`, the bundle as `package.mft` names it).
    pub bundles: Vec<(String, String)>,
}

impl Announcer {
    /// A level's announcer data, read from the game copy.
    ///
    /// # Errors
    /// A bundle does not read.
    pub fn of(game: &Game, lv: &str) -> Result<Self> {
        let low = lv.to_ascii_lowercase();
        let own = game.live(&game.level_bundles(&low, "level"))?;
        let mut partitions: Vec<String> = own
            .values()
            .filter(|(_, e)| is_announcer(&e.name))
            .map(|(_, e)| e.name.clone())
            .collect();
        partitions.sort_by_key(|n| n.to_ascii_lowercase());
        let blocks = game.vo_blocks(&low);
        let mut manifest = Vec::new();
        let mut bundles = Vec::new();
        for (kind, _) in &blocks {
            if game.in_manifest(&format!("levels/{low}/{kind}")) {
                manifest.push(kind.clone());
            }
            if let Some(b) = [
                format!("package:levels/{low}/{kind}-00.fbrb"),
                format!("game:levels/{low}/{kind}-00.fbrb"),
            ]
            .into_iter()
            .find(|b| game.bundle(b).is_file())
            {
                bundles.push((kind.clone(), b));
            }
        }
        Ok(Self {
            partitions,
            blocks,
            manifest,
            bundles,
        })
    }

    /// The blocks as a new level `lv` takes them: its own source, a target in the donor's own folder re-pointed to
    /// the new level's, the rest as they are.
    #[must_use]
    pub fn blocks_for(&self, donor: &str, lv: &str) -> Vec<Vec<String>> {
        let (d, l) = (donor.to_ascii_lowercase(), lv.to_ascii_lowercase());
        let from = format!("levels/{d}/");
        self.blocks
            .iter()
            .map(|(kind, targets)| {
                let mut lines = vec![format!("BundleSource game:levels/{l}/{kind}-00.fbrb")];
                for t in targets {
                    let lt = t.to_ascii_lowercase();
                    let t = match lt.find(&from) {
                        Some(i) => format!(
                            "{}levels/{l}/{}",
                            lt.get(..i).unwrap_or_default(),
                            lt.get(i + from.len()..).unwrap_or_default()
                        ),
                        None => t.clone(),
                    };
                    lines.push(format!("BundleTarget {t}"));
                }
                lines
            })
            .collect()
    }

    /// One line for reports.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "{} partitions of its own{}; voice-over blocks {}; its own voice-over bundles {:?}",
            self.partitions.len(),
            if self.partitions.is_empty() {
                String::new()
            } else {
                format!(" ({})", self.partitions.join(", "))
            },
            if self.blocks.is_empty() {
                "none".to_owned()
            } else {
                self.blocks
                    .iter()
                    .map(|(k, t)| format!("{k} -> {}", t.join(" + ")))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            self.bundles.iter().map(|b| &b.1).collect::<Vec<_>>()
        )
    }
}

impl Donor {
    /// The bases (team 1's first) and the flags (by name) of its layer.
    #[must_use]
    pub fn bases_and_flags(&self) -> (Vec<&CapturePoint>, Vec<&CapturePoint>) {
        let (mut bases, mut flags): (Vec<&CapturePoint>, Vec<&CapturePoint>) =
            self.capture_points.iter().partition(|c| c.base);
        bases.sort_by(|a, b| a.team.cmp(&b.team).then(a.name.cmp(&b.name)));
        flags.sort_by(|a, b| a.name.cmp(&b.name));
        (bases, flags)
    }
}

/// One set's profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentProfile {
    /// The set.
    pub profile: Profile,
    /// Its multiplayer levels in the game copy (lower case, by name).
    pub levels: Vec<String>,
    /// Per bundle kind ([`KINDS`]): the `_common` bundles every one of them loads, as `package.mft` names them.
    pub commons: Vec<(String, Vec<String>)>,
    /// Its levels of the mode, by name.
    pub donors: Vec<Donor>,
    /// The profile's `LevelData` fields and the set's value.
    pub fields: Vec<(String, String)>,
}

impl ContentProfile {
    /// The common bundles of a kind.
    #[must_use]
    pub fn commons(&self, kind: &str) -> &[String] {
        self.commons
            .iter()
            .find(|(k, _)| k == kind)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    /// A donor by level name.
    #[must_use]
    pub fn donor(&self, lv: &str) -> Option<&Donor> {
        self.donors.iter().find(|d| d.level.eq_ignore_ascii_case(lv))
    }

    /// The default donor: the first whose layer stays inside the set's common bundles and the startup bundles, else
    /// the first.
    #[must_use]
    pub fn default_donor(&self) -> Option<&Donor> {
        self.donors
            .iter()
            .find(|d| d.outside.is_empty())
            .or_else(|| self.donors.first())
    }
}

/// Every set of the game copy that has levels of the mode.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profiles {
    /// The mode (`conquest`).
    pub mode: String,
    /// The sets, in [`Profile::ALL`]'s order.
    pub sets: Vec<ContentProfile>,
}

impl Profiles {
    /// A set's profile.
    ///
    /// # Errors
    /// The game copy has no level of the mode in that set.
    pub fn get(&self, p: Profile) -> Result<&ContentProfile> {
        self.sets.iter().find(|s| s.profile == p).ok_or_else(|| {
            ExportError(format!(
                "the game copy has no {} level of the {} mode to take the {} profile from",
                p.word(),
                self.mode,
                p.word()
            ))
        })
    }
}

/// The mode a `GameMode` reference names, as the export calls it (`gamemodes/namconquest/<guid>` -> `conquest`;
/// `None` for the campaign's).
#[must_use]
pub fn mode_of(reference: &str) -> Option<String> {
    let name = reference.split('/').nth(1)?.to_ascii_lowercase();
    let name = name.strip_prefix("nam").unwrap_or(&name).to_owned();
    (!name.is_empty() && !name.starts_with("sing") && name != "null").then_some(name)
}

/// The description asset's `Description` complex of a description partition.
fn description(w: &LevelWriter) -> Option<NodeId> {
    let a = w.d.of_type(b".LevelDescriptionAsset").first().copied()?;
    w.d.child(a, b"Description")
}

fn text_of(d: &EditDbx, n: NodeId, f: &[u8]) -> Option<String> {
    let x = d.child(n, f)?;
    match d.attr(x, b"ref") {
        Some(r) => Some(String::from_utf8_lossy(r).into_owned()),
        None => d.text(x).map(|t| String::from_utf8_lossy(t).into_owned()),
    }
}

/// A partition's `LevelData` fields that are plain fields (a text or a reference; arrays and complexes left out).
#[must_use]
pub fn level_fields(w: &LevelWriter) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(ld) = w.level_data() else { return out };
    for &k in w.d.children(ld).unwrap_or_default() {
        if w.d.tag(k) != b"field" {
            continue;
        }
        let Some(name) = w.d.attr(k, b"name") else { continue };
        let v = match (w.d.attr(k, b"ref"), w.d.text(k)) {
            (Some(r), _) => String::from_utf8_lossy(r).into_owned(),
            (None, Some(t)) => String::from_utf8_lossy(t).into_owned(),
            // a number, bool or byte: its raw bytes
            (None, None) => w.d.raw(k).iter().fold(String::from("0x"), |mut s, b| {
                use std::fmt::Write as _;
                let _ = write!(s, "{b:02x}");
                s
            }),
        };
        out.insert(String::from_utf8_lossy(name).into_owned(), v);
    }
    out
}

struct Listed {
    lv: String,
    profile: Profile,
    mode: String,
    desc: Vec<u8>,
}

/// The `_common` targets of a block, in order.
fn commons_of(game: &Game, lv: &str, kind: &str) -> Vec<String> {
    game.targets(&format!("levels/{lv}/{kind}-00.fbrb"))
        .into_iter()
        .filter(|t| {
            t.to_ascii_lowercase()
                .rsplit_once('/')
                .is_some_and(|(dir, _)| dir.ends_with("_common"))
        })
        .collect()
}

/// Every level of the game copy with its own level bundle there, and its set (lower case, by name).
fn levels_of(game: &Game) -> Vec<(String, Profile)> {
    let mut lvs: Vec<(String, Profile)> = Vec::new();
    for s in game.sources() {
        let low = s.to_ascii_lowercase();
        let Some(lv) = low
            .strip_prefix("game:levels/")
            .and_then(|r| r.strip_suffix("/level-00.fbrb"))
        else {
            continue;
        };
        if lv.contains('/')
            || lv.ends_with("_common")
            || lv.starts_with("opfor_")
            || game.level_bundles(lv, "level").is_empty()
        {
            continue;
        }
        let targets = game.targets(&format!("levels/{lv}/level-00.fbrb"));
        let package = game.bundle(&format!("package:levels/{lv}/level-00.fbrb")).is_file();
        lvs.push((lv.to_owned(), Profile::of_stack(lv, &targets, package)));
    }
    lvs.sort_by(|a, b| a.0.cmp(&b.0));
    lvs.dedup_by(|a, b| a.0 == b.0);
    lvs
}

/// The multiplayer levels of the game copy and their modes, as their descriptions name them.
fn listed(game: &Game) -> Result<Vec<Listed>> {
    let lvs = levels_of(game);
    let live = game.live(&game.startup())?;
    let wanted: Vec<(&(String, Profile), &Found)> = lvs
        .iter()
        .filter_map(|x| live.get(&format!("levels/{}_description.dbx", x.0)).map(|f| (x, f)))
        .collect();
    let datas = read_found(&wanted.iter().map(|x| x.1).collect::<Vec<_>>())?;
    let mut out: Vec<Listed> = Vec::new();
    for (((lv, profile), _), data) in wanted.into_iter().zip(datas) {
        let Ok(w) = LevelWriter::parse(&data) else { continue };
        let Some(m) = description(&w)
            .and_then(|c| text_of(&w.d, c, b"GameMode"))
            .and_then(|r| mode_of(&r))
        else {
            continue;
        };
        out.push(Listed {
            lv: lv.clone(),
            profile: *profile,
            mode: m,
            desc: data,
        });
    }
    Ok(out)
}

/// The multiplayer modes each set's stock levels have (round E3d), from the game copy's descriptions: set -> mode
/// (`conquest`, `rush`, `squadrush`, `squaddeathmatch` ...) -> its levels. A set offers an export only these modes.
///
/// # Errors
/// A bundle does not read.
pub fn modes(game: &Game) -> Result<BTreeMap<Profile, BTreeMap<String, Vec<String>>>> {
    let mut out: BTreeMap<Profile, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for l in listed(game)? {
        out.entry(l.profile).or_default().entry(l.mode).or_default().push(l.lv);
    }
    Ok(out)
}

/// Each set's common level bundles (round E3d): the `_common` targets every multiplayer level of the set loads, in
/// order, over all its modes (BFBC2 `mp_common`; BFBC1 `mp2_common`, `mp_common`; Vietnam `nam_common`,
/// `mp_common`; WW2 `ww2_common`, `mp_common`).
///
/// # Errors
/// A bundle does not read.
pub fn set_commons(game: &Game) -> Result<BTreeMap<Profile, Vec<String>>> {
    let mut by: BTreeMap<Profile, Vec<Vec<String>>> = BTreeMap::new();
    for l in listed(game)? {
        by.entry(l.profile).or_default().push(commons_of(game, &l.lv, "level"));
    }
    Ok(by
        .into_iter()
        .map(|(p, lists)| {
            let mut it = lists.into_iter();
            let first = it.next().unwrap_or_default();
            let rest: Vec<HashSet<String>> = it.map(|v| v.iter().map(|t| t.to_ascii_lowercase()).collect()).collect();
            (
                p,
                first
                    .into_iter()
                    .filter(|t| rest.iter().all(|r| r.contains(&t.to_ascii_lowercase())))
                    .collect(),
            )
        })
        .collect())
}

#[allow(
    clippy::too_many_lines,
    reason = "the scan's steps in order: the levels, their sets, commons, donors, fields"
)]
/// The sets' profiles for a mode (`conquest`), read from the game copy (nothing is written).
///
/// # Errors
/// A bundle does not read.
pub fn scan(game: &Game, mode: &str) -> Result<Profiles> {
    let mode = mode.to_ascii_lowercase();
    // every multiplayer level of the game copy, its set and mode
    let listed = listed(game)?;
    let mut sets = Vec::new();
    for p in Profile::ALL {
        let mine: Vec<&Listed> = listed.iter().filter(|l| l.profile == p).collect();
        if !mine.iter().any(|l| l.mode == mode) {
            continue;
        }
        let commons: Vec<(String, Vec<String>)> = KINDS
            .iter()
            .map(|&k| {
                let mut lists = mine.iter().map(|l| commons_of(game, &l.lv, k));
                let first = lists.next().unwrap_or_default();
                let rest: Vec<HashSet<String>> = lists
                    .map(|v| v.iter().map(|t| t.to_ascii_lowercase()).collect())
                    .collect();
                let all: Vec<String> = first
                    .into_iter()
                    .filter(|t| rest.iter().all(|r| r.contains(&t.to_ascii_lowercase())))
                    .collect();
                (k.to_owned(), all)
            })
            .collect();
        // what the commons and the startup bundles give (a donor's layer should need nothing else)
        let mut given: Vec<PathBuf> = commons
            .iter()
            .find(|(k, _)| k == "level")
            .map_or_default(|(_, v)| v.iter().map(|t| game.bundle(t)).collect());
        given.extend(game.startup());
        let given: HashSet<String> = game.live(&given)?.into_keys().collect();
        let mut donors = Vec::new();
        for l in mine.iter().filter(|l| l.mode == mode) {
            donors.push(donor(game, l, &given)?);
        }
        sets.push(ContentProfile {
            profile: p,
            levels: mine.iter().map(|l| l.lv.clone()).collect(),
            commons,
            donors,
            fields: Vec::new(),
        });
    }
    // the profile's fields: one value on every donor of each set, not one value over all sets
    let names: BTreeSet<String> = sets
        .iter()
        .flat_map(|s| s.donors.iter().flat_map(|d| d.fields.keys().cloned()))
        .collect();
    let mut chosen: Vec<String> = Vec::new();
    for n in names {
        let per: Vec<Option<&String>> = sets
            .iter()
            .map(|s| {
                let vals: BTreeSet<Option<&String>> = s.donors.iter().map(|d| d.fields.get(&n)).collect();
                if vals.len() == 1 {
                    vals.into_iter().next().flatten()
                } else {
                    None
                }
            })
            .collect();
        let distinct: BTreeSet<&String> = per.iter().flatten().copied().collect();
        if sets.len() > 1 && per.iter().all(Option::is_some) && distinct.len() > 1 {
            chosen.push(n);
        }
    }
    for s in &mut sets {
        s.fields = chosen
            .iter()
            .filter_map(|n| Some((n.clone(), s.donors.first()?.fields.get(n)?.clone())))
            .collect();
    }
    Ok(Profiles { mode, sets })
}

fn donor(game: &Game, l: &Listed, given: &HashSet<String>) -> Result<Donor> {
    let dw = LevelWriter::parse(&l.desc)?;
    let dc = description(&dw);
    let t = |f: &[u8]| dc.and_then(|c| text_of(&dw.d, c, f)).unwrap_or_default();
    let mut kits = Vec::new();
    if let Some(arr) = dc.and_then(|c| dw.d.child(c, b"KitImages")) {
        for &k in dw.d.children(arr).unwrap_or_default() {
            kits.push(format!(
                "{} {} {}",
                text_of(&dw.d, k, b"Team").unwrap_or_default(),
                text_of(&dw.d, k, b"KitCategory").unwrap_or_default(),
                text_of(&dw.d, k, b"Texture").unwrap_or_default()
            ));
        }
    }
    let bundles = game.loaded(&l.lv, "level");
    let live: HashMap<String, Found> = game.live(&bundles)?;
    let f = live
        .get(&format!("levels/{}.dbx", l.lv))
        .ok_or_else(|| ExportError(format!("{}: no level partition", l.lv)))?;
    let data = read_found(&[f])?.pop().unwrap_or_default();
    let w = LevelWriter::parse(&data)?;
    let outside = mpconvert::layer_partitions(&w)
        .into_iter()
        .filter(|p| !given.contains(&format!("{p}.dbx")))
        .collect();
    Ok(Donor {
        level: l.lv.clone(),
        game_mode: t(b"GameMode"),
        factions: [t(b"FactionTeam1"), t(b"FactionTeam2")],
        kits,
        capture_points: mpconvert::capture_points(&w),
        outside,
        fields: level_fields(&w),
        announcer: Announcer::of(game, &l.lv)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        assert_eq!(mode_of("gamemodes/namconquest/a-b").as_deref(), Some("conquest"));
        assert_eq!(mode_of("gamemodes/Conquest/x").as_deref(), Some("conquest"));
        assert_eq!(mode_of("gamemodes/goldrush/x").as_deref(), Some("goldrush"));
        assert_eq!(mode_of("gamemodes/singelplayerfirstlevel/x"), None);
        assert_eq!(mode_of("gamemodes/singleplayer/x"), None);
        assert_eq!(mode_of("null"), None);
    }
}
