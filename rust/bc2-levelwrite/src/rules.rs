//! The export's rule registry (round V1, `spec/transforms/validation.md`): every way an exported level can fail in
//! the game, or be refused by the export, as a named rule with a code, a severity, a plain-language message and the
//! specification rules it rests on.
//!
//! Three kinds of rules, one registry:
//!
//! - the **XVAL** rules (`XVAL-<SECTION>-<n>`): the export's own. Each is checked in one or more [`Phase`]s - live
//!   in the editor while the project is edited ([`crate::validate`]), by `mapexport build` before it writes (a
//!   refusal), and by `mapexport check` on a staged level ([`crate::export::check`]: each check line carries its
//!   code). They are the `spec/transforms/validation.md` rules of the same ID; a test keeps the two the same.
//! - the **L1** placement rules (`L1-CQ-002` ...): DICE's measured layout tendencies from round L1's study of the
//!   stock maps (`bfbc2-map-editor/placement/l1_rules.json`, read at build time), checked by
//!   [`crate::placement`]. Never errors: a value outside the rule's warning band is a [`Severity::Warning`], one
//!   inside it but outside the stock band a [`Severity::Note`]; each finding shows the stock range.
//!
//! A [`Finding`] is one rule broken (or, for a note line of the stage check, observed) on one project or stage,
//! with the objects it is about (the editor's Problems panel goes to them).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// How bad a broken rule is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Worth knowing: the level works, it is unlike DICE's.
    Note,
    /// The level may play badly or not as meant; the export goes ahead.
    Warning,
    /// The export refuses it, or the game would crash or misbehave.
    Error,
}

impl Severity {
    /// The word the panels and `mapexport` print.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// Where a rule is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Live in the editor (and `mapexport validate`), on the project as edited.
    Edit,
    /// By `mapexport build`, before it writes: a broken rule is a refusal.
    Build,
    /// By `mapexport check`, on the staged level.
    Stage,
    /// L1's placement tendencies (live in the editor), warnings and notes only.
    Placement,
    /// The DBX inspector's Level > Check and Fix slots (`DbxEditGUI`'s, round GR4), on a partition opened by hand.
    Inspector,
}

/// One rule of the registry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rule {
    /// Its code (`XVAL-REG-1`, `L1-RU-004`).
    pub code: String,
    /// How bad breaking it is.
    pub severity: Severity,
    /// Its title (the spec heading's).
    pub title: String,
    /// In plain words: what goes wrong in the game, and what to do.
    pub message: String,
    /// The specification rules it rests on (spec IDs; L1's own ID for a placement rule).
    pub spec: Vec<String>,
    /// Where it is checked.
    pub phases: Vec<Phase>,
    /// The round that brought the check (`EX1`, `XF1`, `V1` ...).
    pub from: String,
}

/// One rule broken (or observed) on a project or a stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// The rule's code.
    pub code: String,
    /// How bad (the rule's, or a placement rule's note).
    pub severity: Severity,
    /// What was found, in words.
    pub message: String,
    /// The objects it is about (upper-case guids): the editor goes to the first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<String>,
}

impl Finding {
    /// A finding of `code` at the rule's severity.
    #[must_use]
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            severity: rule(code).map_or(Error, |r| r.severity),
            message: message.into(),
            objects: Vec::new(),
        }
    }

    /// The same at another severity (a placement rule's note).
    #[must_use]
    pub fn at(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// The same, naming these objects.
    #[must_use]
    pub fn naming(mut self, objects: Vec<String>) -> Self {
        self.objects = objects;
        self
    }

    /// `<code> <severity>: <message>`, as `mapexport validate` prints it.
    #[must_use]
    pub fn line(&self) -> String {
        format!("{:<12} {:<7} {}", self.code, self.severity.word(), self.message)
    }
}

/// An XVAL rule as the table below writes it: code, severity, title, message, spec IDs, phases, round.
type Row = (
    &'static str,
    Severity,
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [Phase],
    &'static str,
);

use Phase::{Build, Edit, Inspector, Stage};
use Severity::{Error, Note, Warning};

/// The export's own rules. Each is a rule of `spec/transforms/validation.md` with the same ID and title.
const XVAL: &[Row] = &[
    // ---- the project's log
    (
        "XVAL-EDIT-1",
        Warning,
        "The base level changed since the project was made",
        "The base level's partition is not the one the edits were made on (a game patch or a rebuilt level). The \
         edits are kept; check that each still does what you meant.",
        &["XLOG-BASE-1"],
        &[Edit],
        "OP1",
    ),
    (
        "XVAL-EDIT-2",
        Error,
        "An operation names an object the level does not have",
        "An edit names an object the base level no longer has. The export refuses the project until that edit is \
         undone or the object is back.",
        &["XLOG-BASE-1", "XLOG-APP-1"],
        &[Edit, Build],
        "OP1",
    ),
    (
        "XVAL-EDIT-3",
        Error,
        "An operation does not apply",
        "An edit could not be applied to the level (its reason is given). The export refuses the project until it \
         is undone.",
        &["XLOG-APP-1"],
        &[Edit, Build],
        "OP1",
    ),
    (
        "XVAL-EDIT-4",
        Error,
        "The level partition does not parse",
        "The level's partition could not be read, so no edit is written to it. A partition the game cannot parse is \
         a null the loader passes on and the load crashes (0x49EDF4).",
        &["LOAD-PART-1"],
        &[Edit, Stage],
        "V1",
    ),
    (
        "XVAL-EDIT-5",
        Error,
        "Terrain and paint edits are not exported yet",
        "The project edits the terrain or paints it, and the export does not write terrain yet (round E5): it \
         refuses the project. Undo the strokes to export the rest.",
        &["XLOG-DELTA-1", "TERR-VT-5"],
        &[Edit, Build],
        "OP1",
    ),
    (
        "XVAL-EDIT-6",
        Error,
        "Only the level's own root files are exported",
        "The project edits a file the export does not write (only the level partition and the base's own static \
         group files are written). The export refuses it.",
        &["XLOG-FILE-2", "LOAD-PART-5"],
        &[Build],
        "OP1",
    ),
    // ---- the mode
    (
        "XVAL-MODE-1",
        Error,
        "Only modes the export offers",
        "Onslaught needs bots (NavKit path data and the AI layer) and Air Superiority is BF1943's: the export \
         refuses both.",
        &["GAME-MODE-8"],
        &[Edit, Build],
        "E3d",
    ),
    (
        "XVAL-MODE-2",
        Error,
        "The donor is a level of the era's mode",
        "The mode layer's donor must be a stock level of the chosen era and mode; another level's layer would name \
         kits, factions and game mode data the era does not load.",
        &["XEXP-PRE-2"],
        &[Build, Stage],
        "E3b",
    ),
    (
        "XVAL-MODE-3",
        Error,
        "The mode layer is complete",
        "A multiplayer level needs its whole mode layer (player, mission, teams, bases, counters, spawns, and per \
         mode its flags, stages or squads); without it the session waits forever in state 14. Put back what is \
         missing, or choose the mode again.",
        &["XEXP-CHK-2", "GAME-LAYER-1", "GAME-LAYER-4", "LEVEL-MP-1"],
        &[Edit, Stage],
        "E3d",
    ),
    (
        "XVAL-MODE-4",
        Warning,
        "The donor level changed since the mode was chosen",
        "The donor's partition is not the one the mode was chosen from (a patch or a rebuilt WW2 level): the layer \
         is grafted from it as it is now. Choose the mode again to see what changed.",
        &["XEXP-PRE-2", "XLOG-BASE-1"],
        &[Edit, Build],
        "V1",
    ),
    (
        "XVAL-MODE-5",
        Error,
        "The export file's combat area is in the base",
        "An export file without a project mode grafts the donor's layer into the base's combat area it names; that \
         area must be in the base level.",
        &["XEXP-BLD-1"],
        &[Build],
        "E3",
    ),
    // ---- registration
    (
        "XVAL-REG-1",
        Error,
        "One package.mft block and one bundleManifest line per bundle",
        "Each of the level's bundles needs its package.mft block and its bundleManifest line, or the server crashes \
         computing the level checksum at start-up.",
        &["REG-NEW-1", "CSUM-CHK-1", "CSUM-CHK-2"],
        &[Stage],
        "E3",
    ),
    (
        "XVAL-REG-2",
        Error,
        "Every bundle the registration names exists",
        "A registered bundle that is missing crashes the server's level checksum at start-up.",
        &["CSUM-CHK-2", "REG-MFT-4"],
        &[Stage],
        "E3",
    ),
    (
        "XVAL-REG-3",
        Error,
        "A new name, never a level of the game",
        "An export never replaces a level of the game copy or one another tool registered: choose another id.",
        &["REG-NEW-2", "XEXP-PRE-1"],
        &[Build, Stage],
        "E3",
    ),
    (
        "XVAL-REG-4",
        Note,
        "Server and client need the same registration",
        "Install the level on the server and on every client: both register its bundles in bundleManifest and \
         package.mft, or the level checksums differ (and a server without them crashes computing its checksum).",
        &["CSUM-CHK-4", "CSUM-CHK-1"],
        &[Stage],
        "V1",
    ),
    (
        "XVAL-REG-5",
        Error,
        "The announcer's registration is the donor's",
        "The level's voice-over blocks and manifest lines must be its donor's under the new name, with no block \
         whose bundleManifest line is missing.",
        &["XEXP-REG-2", "SND-VO-3"],
        &[Stage],
        "E3c",
    ),
    // ---- names
    (
        "XVAL-NAME-1",
        Error,
        "The level id's form",
        "The level id is a file, folder and localisation key: 3 to 40 of A-Z, 0-9 and _, starting with a letter.",
        &["XEXP-PRE-1", "LVL-ID-1"],
        &[Edit, Build, Stage],
        "EX1",
    ),
    (
        "XVAL-NAME-2",
        Error,
        "No _NAME in the level id",
        "The loading screen cuts the name key at its first _NAME to find its picture: an id holding _NAME gives a \
         black loading screen.",
        &["LVL-DESC-2"],
        &[Edit, Build, Stage],
        "EX1",
    ),
    (
        "XVAL-NAME-3",
        Error,
        "The level id clashes with nothing the game has",
        "The id must not be a level, another project's export, a loading picture or a text the game already has.",
        &["REG-NEW-2", "XEXP-PRE-1"],
        &[Build, Stage],
        "EX1",
    ),
    (
        "XVAL-NAME-4",
        Error,
        "The folder is the id in lower case",
        "The game finds a level's bundles in Package\\levels\\<id>\\: the folder cannot differ from the id.",
        &["LVL-ID-1"],
        &[Edit, Build, Stage],
        "EX1",
    ),
    (
        "XVAL-NAME-5",
        Error,
        "The server-browser name's form",
        "maps.ini's name: plain ASCII, no leading or trailing space, no quote, a browser row of at most 159 \
         characters.",
        &["XEXP-ID-1"],
        &[Edit, Build, Stage],
        "EX1",
    ),
    (
        "XVAL-NAME-6",
        Error,
        "The server-browser name is not another map's",
        "Two maps with one name give server browser rows that look the same.",
        &["XEXP-ID-1"],
        &[Build, Stage],
        "EX1",
    ),
    (
        "XVAL-NAME-7",
        Error,
        "The on-screen names fit every language",
        "The on-screen name (64) and description (400) must fit and use only letters each language's texts have, \
         or they do not show.",
        &["XEXP-ID-1"],
        &[Edit, Build, Stage],
        "EX1",
    ),
    (
        "XVAL-NAME-8",
        Error,
        "Every place names the level the same",
        "The description, the menus' level list, every package.mft block and bundleManifest line and the name \
         key's loading picture must name the same level: a mismatch is the start-up checksum crash or a black \
         loading screen.",
        &["XEXP-ID-1", "REG-NEW-5", "LVL-DESC-2"],
        &[Stage],
        "EX1",
    ),
    (
        "XVAL-NAME-9",
        Error,
        "The level's own name keys and pictures",
        "With its own name the level's description and partition use its own keys, every text is stored in each \
         language's encoding, and its pictures are in their templates' form.",
        &["XEXP-ID-1"],
        &[Stage],
        "E3b",
    ),
    // ---- the staged files
    (
        "XVAL-FILE-1",
        Error,
        "The staged files are the ones recorded",
        "A staged bundle changed after the export: build the stage again.",
        &["XEXP-CHK-1"],
        &[Stage],
        "E3",
    ),
    (
        "XVAL-FILE-2",
        Error,
        "Every staged bundle opens and every partition parses",
        "A partition that does not parse is a null the loader passes on: the load crashes (0x49EDF4).",
        &["LOAD-PART-1", "XEXP-CHK-1"],
        &[Stage],
        "E3",
    ),
    // ---- references
    (
        "XVAL-REF-1",
        Error,
        "Local references name an instance of their partition",
        "A reference to an instance the partition does not have is a null the game reads.",
        &["REF-RES-1", "REF-DEL-2"],
        &[Stage],
        "E3",
    ),
    (
        "XVAL-REF-2",
        Note,
        "DICE's own dangling references",
        "References to partitions no bundle of the game has, made exactly as DICE's source partitions make them: the \
         stock levels load with them.",
        &["REF-RES-3"],
        &[Stage],
        "E3",
    ),
    (
        "XVAL-REF-3",
        Error,
        "Every referenced partition is in a bundle the level loads",
        "A blueprint that resolves nowhere crashes the server's checksum (0x6EFCA4).",
        &["REF-RES-2", "REF-RES-5", "CSUM-CHK-3"],
        &[Stage],
        "E3",
    ),
    (
        "XVAL-REF-4",
        Error,
        "Every external reference names an instance of its partition",
        "A reference to an instance its partition does not have is a null the game reads.",
        &["REF-RES-2"],
        &[Stage],
        "E3",
    ),
    (
        "XVAL-REF-5",
        Error,
        "The closure resolves under the profile's bundles",
        "Every partition the level reaches must resolve in the bundles it loads; the export copies what does not, \
         and never registers it.",
        &["XEXP-CLO-1", "REF-RES-5"],
        &[Stage],
        "E3d",
    ),
    // ---- the level
    (
        "XVAL-LVL-1",
        Error,
        "LevelData.Name is the level's path",
        "LevelData.Name must be Levels/<id>, the name every other file uses.",
        &["LEVEL-DATA-2", "LVL-ID-1"],
        &[Stage],
        "E3",
    ),
    (
        "XVAL-LVL-2",
        Error,
        "The description names the level",
        "The description's LevelName must be Levels/<id>, or the menus start another level.",
        &["LVL-DESC-1", "LVL-ID-1"],
        &[Stage],
        "E3",
    ),
    (
        "XVAL-LVL-3",
        Error,
        "The height field is in a bundle the level loads",
        "Without its height field the map loads without ground: keep the base's terrain bundles.",
        &["TERR-NAME-1", "LOAD-PART-6"],
        &[Stage],
        "E3d",
    ),
    // ---- the content profile
    (
        "XVAL-PROF-1",
        Error,
        "Every block loads the era's common bundles",
        "Each package.mft block must load the era's common bundles in their order, or the era's kits, vehicles and \
         sounds do not resolve.",
        &["XEXP-PRE-2", "LVL-DESC-1"],
        &[Stage],
        "E3b",
    ),
    (
        "XVAL-PROF-2",
        Error,
        "The game mode is the donor's and resolves",
        "The description's GameMode must be the donor's and resolve: a GameMode that does not resolve crashes \
         (0x74EAB3).",
        &["LEVEL-DESC-2", "LOAD-START-2"],
        &[Stage],
        "E3b",
    ),
    (
        "XVAL-PROF-3",
        Error,
        "The factions are the donor's",
        "The description's factions must be the donor's, the era's teams.",
        &["XEXP-PRE-2"],
        &[Stage],
        "E3b",
    ),
    (
        "XVAL-PROF-4",
        Error,
        "The kit pictures are the donor's and resolve",
        "The description's kit pictures must be the donor's, each resolving in the bundles the level loads.",
        &["XEXP-PRE-2", "LOAD-PART-4"],
        &[Stage],
        "E3b",
    ),
    (
        "XVAL-PROF-5",
        Error,
        "The era's level fields",
        "The level fields the era sets (Vietnam's sound states ...) must be the era's.",
        &["XEXP-PRE-2"],
        &[Stage],
        "E3b",
    ),
    (
        "XVAL-PROF-6",
        Error,
        "The announcer is the era's",
        "Every announcer partition the level names must resolve as on its donor, all from one era's voices.",
        &["XEXP-VO-1", "SND-VO-2"],
        &[Stage],
        "E3c",
    ),
    // ---- the weather
    (
        "XVAL-WTHR-1",
        Error,
        "The weather the export gave the level",
        "The level's weather state, its spawn switch and its copied partitions must be what the export wrote.",
        &["RNDR-WTHR-8", "RNDR-WTHR-5"],
        &[Stage],
        "VW2",
    ),
    // ---- the cull tree
    (
        "XVAL-CULL-1",
        Error,
        "One cull tree, one object per slot",
        "Every slotted static object needs a slot of its own inside the tree: a shared slot or one outside the tree \
         draws the wrong object or none.",
        &["CULL-SLOT-3", "LEVEL-SLOT-2", "LEVEL-SLOT-3"],
        &[Stage],
        "E2",
    ),
    (
        "XVAL-CULL-2",
        Error,
        "A moved or placed object stands in its own blueprint's cull shape",
        "A re-slotted object must stand in leaves of its own blueprint's shape where it stands, or it pops or \
         vanishes as the camera moves.",
        &["CULL-ADD-1", "CULL-SPH-1"],
        &[Stage],
        "XF1",
    ),
    (
        "XVAL-CULL-3",
        Error,
        "Every new static object has a cull slot",
        "A new object left at slot 0 is see-through and indestructible: only objects no level slots may stay \
         without a slot.",
        &["CULL-SLOT-3"],
        &[Stage],
        "XF1",
    ),
    (
        "XVAL-CULL-4",
        Error,
        "Objects stay within +-2048 m",
        "The cull tree holds centres within +-2048 m on each axis: the export cannot slot an object moved or placed \
         beyond that.",
        &["CULL-SPH-1"],
        &[Edit, Build],
        "V1",
    ),
    // ---- shaders
    (
        "XVAL-SHDR-1",
        Error,
        "Shader entries for every mesh the level draws",
        "Every mesh the level reaches needs entries in its shader databases (except those DICE's base reaches \
         without them): a part with no program crashes the renderer (0x98BD6E) or draws nothing.",
        &["RNDR-DRAW-6", "RNDR-DB-1"],
        &[Stage],
        "XF1",
    ),
    (
        "XVAL-SHDR-2",
        Error,
        "No 40 km/h sign drawn with another level's entries",
        "The 40 km/h road sign (SpeedSign_40) drawn with another level's shader entries crashed the renderer at \
         0x98BD6E (OPFOR's Isla Inocentes test, unsolved). Place it only on a level whose own database draws it.",
        &["RNDR-DRAW-6"],
        &[Edit, Stage],
        "V1",
    ),
    // ---- sound
    (
        "XVAL-SND-1",
        Error,
        "No streamed wave data in a level bundle",
        "Streamed wave data in a level bundle is read as a partition and the load crashes (0x49EDF4).",
        &["SND-STRM-1", "SND-WAVE-2"],
        &[Stage],
        "QW2",
    ),
    (
        "XVAL-SND-2",
        Error,
        "No deleted marker hides a sound",
        "A deleted marker in a level bundle hides the entry of the same name in the bundles after it: a hidden \
         wave is a null wave in a sampler and the client crashes (0xFF2AC2).",
        &["SND-NULL-1", "SND-LOAD-1"],
        &[Stage],
        "V1",
    ),
    // ---- terrain
    (
        "XVAL-TERR-1",
        Error,
        "The terrain is the base's own, whole",
        "The visual terrain's patch shaders must be drawn with the mask atlas of their own level: patch shaders \
         from a donor whose atlas has another layout draw every tiled cell black. The level's terrain bundle must \
         be the base's, entry for entry.",
        &["TERR-VT-5", "TERR-NAME-1"],
        &[Stage],
        "V1",
    ),
    // ---- console data
    (
        "XVAL-CONS-1",
        Error,
        "No Xbox 360 resource in a PC level",
        "Xbox 360 textures (big-endian headers, tiled texels) and Havok physics (layout rules 04 00 00 01) are not \
         the PC's: convert them first (bfbc2-console-convert) or the game reads garbage.",
        &["XCONS-TEX-1", "XCONS-HK-1", "TEX-HDR-1"],
        &[Stage],
        "V1",
    ),
    // ---- static groups
    (
        "XVAL-GRP-1",
        Error,
        "An edited static group keeps DICE's layout and its kinds",
        "A static group's physics lists one entry per member and part (0xAB4E58 when they disagree). Members may be \
         moved, deleted and added (round SG1): they are laid out again as DICE lays groups out (members, leaves, \
         cumulative parts and network ids, physics), every kind a group holds keeps a member (the last one of a kind \
         is never deleted: move it instead) and no group gains a kind it did not hold (an added member is a copy of \
         one of its group's; another object is placed loose).",
        &["LEVEL-MG-4", "LEVEL-MG-5", "LEVEL-MG-8", "XLOG-MEM-2", "LOAD-MG-1"],
        &[Build, Stage],
        "V1, SG1",
    ),
    // ---- the level partition's own consistency (DbxEditGUI's Level > Check, in the DBX inspector since GR4)
    (
        "XVAL-OBJ-1",
        Error,
        "No empty entry in the level's object list",
        "LevelData.Objects holds an empty (null) entry: the game reads the list without a check and crashes.",
        &["LEVEL-OBJ-2", "REF-DEL-2"],
        &[Edit, Inspector],
        "GR4",
    ),
    (
        "XVAL-OBJ-2",
        Error,
        "A world object keeps its physics and mesh",
        "A world object without its PhysicsData or MeshEntityType crashes the model-group set-up.",
        &["LEVEL-MG-3"],
        &[Edit, Inspector],
        "GR4",
    ),
    (
        "XVAL-OBJ-3",
        Warning,
        "World objects are in the level's object list",
        "An object not in LevelData.Objects is not loaded: missing terrain or buildings in the game.",
        &["LEVEL-OBJ-3", "LEVEL-OBJ-4"],
        &[Edit, Inspector],
        "GR4",
    ),
    (
        "XVAL-CULL-5",
        Warning,
        "No cull slot shared by two objects",
        "Objects sharing a cull slot are shown and hidden together (see-through objects). The export gives moved \
         and placed objects slots of their own; the DBX inspector's Level > Fix slots repairs a hand-edited file.",
        &["CULL-SLOT-3", "LEVEL-SLOT-2"],
        &[Inspector],
        "GR4",
    ),
    (
        "XVAL-CULL-6",
        Warning,
        "Slots agree with the level whose cull trees are used",
        "Objects with another slot than the same object in the cull trees' level are see-through: Level > Fix slots \
         in the DBX inspector gives each the slot of its counterpart.",
        &["CULL-SLOT-2", "LEVEL-SLOT-3"],
        &[Inspector],
        "GR4",
    ),
    (
        "XVAL-AI-1",
        Error,
        "The staged path data is the level's and loads",
        "The export's path-data step stages the level's path data only where it is the level's (the project edited \
         no collision and has no navigation edits) and only Kynapse path data whose sectors' nodes are sorted by id \
         (unsorted ones crash the game when it leaves the level). Build or sample the level's path data again.",
        &["HPD-NODE-2", "AI-CRASH-4", "AI-NEED-2"],
        &[Build, Stage],
        "A2",
    ),
    // ---- placement checks of the export's own (not L1's tendencies)
    (
        "XVAL-PLACE-1",
        Warning,
        "Vehicle spawners do not stand on each other",
        "Two vehicle spawners of one team stand within 1 m of each other: their vehicles spawn into one another.",
        &["GAME-SPAWN-1"],
        &[Edit],
        "V1",
    ),
    (
        "XVAL-PLACE-2",
        Warning,
        "Spawn points stand on ground",
        "A spawn point over water, inside a rock or a wall, under the ground or outside the terrain: a soldier \
         spawning there drowns or is stuck. Move it onto open ground (DICE's spawn points all stand on it).",
        &["GAME-SPAWN-1", "GAME-SPAWN-3"],
        &[Edit],
        "A1",
    ),
    (
        "XVAL-PLACE-3",
        Warning,
        "A vehicle spawner has its vehicle's room and domain",
        "A vehicle spawner needs room for its vehicle (static collision too close at a vehicle's height) and its \
         domain: land vehicles on land, boats on water, none inside collision. Otherwise the vehicle spawns into the \
         world or cannot move.",
        &["GAME-SPAWN-1", "GAME-VEH-1"],
        &[Edit],
        "A1",
    ),
    (
        "XVAL-PLACE-4",
        Warning,
        "Gameplay objects stand inside a combat area",
        "An M-COM, a spawn point or a land or sea vehicle spawner outside every combat area (a vehicle more than \
         150 m out, farther than DICE's own): a player there is told to return to the area and dies as a deserter. \
         Move it in, or the area out.",
        &["GAME-AREA-1", "GAME-AREA-2"],
        &[Edit],
        "A1",
    ),
    (
        "XVAL-PLACE-5",
        Warning,
        "Spawns and objectives have a path for the AI",
        "A spawn point, flag or M-COM with no route node of the level's path data near it, or only one on an island \
         cut off from the main network: a bot spawned there cannot leave, and bots cannot attack or defend an \
         objective there. Move it onto the main network, or build the path data again (the AI workspace).",
        &["AI-NEED-2", "AI-NAV-1"],
        &[Edit, Stage],
        "A1",
    ),
    (
        "XVAL-PLACE-6",
        Warning,
        "A land vehicle has a drivable way out",
        "A tank, IFV, car or quad bike whose spawner it cannot drive 20 m away from (walls, steps, steep ground or \
         deep water all round it; DICE's land vehicles all drive 40 m or more): it spawns and stays. Move the \
         spawner to open ground.",
        &["GAME-VEH-1", "GAME-VEH-2"],
        &[Edit],
        "A2",
    ),
    (
        "XVAL-PLACE-7",
        Warning,
        "A boat has water access",
        "A boat whose spawner's water reaches less than 24 m of open water (a pond, a pier's piles; DICE's boats all \
         reach 30 m or more): it cannot sail out. Move the spawner to the shore of open water.",
        &["GAME-VEH-1", "GAME-VEH-2"],
        &[Edit],
        "A2",
    ),
    (
        "XVAL-PLACE-8",
        Warning,
        "An aircraft has an open launch area",
        "A helicopter with static collision less than 12 m over its spawner, or a plane without a straight take-off \
         run of 30 m (DICE's helipads are open to 60 m, its planes have runs of 34 m or more): it cannot take off. \
         Move the spawner to open ground.",
        &["GAME-VEH-1", "GAME-VEH-2"],
        &[Edit],
        "A2",
    ),
];

/// L1's index as `l1_rules.json` holds it (round L1).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct L1Rule {
    pub id: String,
    pub metric: String,
    pub modes: Option<Vec<String>>,
    pub groups: Vec<String>,
    pub warn: Option<[Option<f64>; 2]>,
    pub info: Option<[Option<f64>; 2]>,
    pub message: String,
    pub levels: usize,
    pub range: Option<[f64; 2]>,
}

/// L1's placement rules, from the study's index (`bfbc2-map-editor/placement/l1_rules.json`, built in).
pub(crate) fn l1_rules() -> &'static [L1Rule] {
    static L1: OnceLock<Vec<L1Rule>> = OnceLock::new();
    L1.get_or_init(|| {
        serde_json::from_str(include_str!("../../../bfbc2-map-editor/placement/l1_rules.json")).unwrap_or_default()
    })
}

/// Every rule: the XVAL rules, then L1's placement rules.
#[must_use]
pub fn all() -> &'static [Rule] {
    static ALL: OnceLock<Vec<Rule>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut out: Vec<Rule> = XVAL
            .iter()
            .map(|&(code, severity, title, message, spec, phases, from)| Rule {
                code: code.into(),
                severity,
                title: title.into(),
                message: message.into(),
                spec: spec.iter().map(|s| (*s).to_owned()).collect(),
                phases: phases.to_vec(),
                from: from.into(),
            })
            .collect();
        for r in l1_rules() {
            if !crate::placement::implemented(&r.id) {
                continue;
            }
            let mut spec = vec![r.id.clone()];
            spec.extend(crate::placement::spec_of(&r.id).iter().map(|s| (*s).to_owned()));
            out.push(Rule {
                code: r.id.clone(),
                severity: Warning,
                title: crate::placement::title_of(r),
                message: r.message.clone(),
                spec,
                phases: vec![Phase::Placement],
                from: "L1".into(),
            });
        }
        out
    })
}

/// The rule of a code.
#[must_use]
pub fn rule(code: &str) -> Option<&'static Rule> {
    all().iter().find(|r| r.code == code)
}

/// The registry's rules per severity (note, warning, error).
#[must_use]
pub fn counts() -> [usize; 3] {
    let mut c = [0; 3];
    for r in all() {
        if let Some(x) = c.get_mut(r.severity as usize) {
            *x += 1;
        }
    }
    c
}

/// The registry as text, a line per rule (`mapexport rules`).
#[must_use]
pub fn table() -> Vec<String> {
    all()
        .iter()
        .map(|r| {
            format!(
                "{:<12} {:<7} {:<9} {} [{}] ({})",
                r.code,
                r.severity.word(),
                r.phases
                    .iter()
                    .map(|p| match p {
                        Edit => "edit",
                        Build => "build",
                        Stage => "stage",
                        Phase::Placement => "placement",
                        Inspector => "inspector",
                    })
                    .collect::<Vec<_>>()
                    .join("+"),
                r.title,
                r.spec.join(", "),
                r.from
            )
        })
        .collect()
}

/// The rule of a finding of `bc2_formats::dbxedit::check::level_check` (`DbxEditGUI`'s Level > Check, the DBX
/// inspector's since round GR4), by its words.
#[must_use]
pub fn dbx_check_code(message: &str) -> &'static str {
    if message.starts_with("empty entry in the level's object list") {
        "XVAL-OBJ-1"
    } else if message.starts_with("world object without") {
        "XVAL-OBJ-2"
    } else if message.starts_with("not in the level's object list") {
        "XVAL-OBJ-3"
    } else if message.starts_with("shared by") {
        "XVAL-CULL-5"
    } else if message.contains("have another slot than the same object") {
        "XVAL-CULL-6"
    } else if message.contains("which that file does not have") {
        "XVAL-REF-4"
    } else if message.contains("not in the given folders") {
        "XVAL-REF-3"
    } else {
        "XVAL-REF-1"
    }
}

/// The rule of an operation that did not apply, by its words: SG1's static group refusals (the last member of a kind,
/// an added member not numbered as one) are XVAL-GRP-1, anything else XVAL-EDIT-3.
#[must_use]
pub fn apply_code(message: &str) -> &'static str {
    if message.contains("a group keeps at least one of each kind")
        || message.contains("an added member is numbered as one")
    {
        "XVAL-GRP-1"
    } else {
        "XVAL-EDIT-3"
    }
}

/// An export refusal of rule `code`: the error the build returns, its message prefixed by the code.
#[must_use]
pub fn refusal(code: &str, why: impl std::fmt::Display) -> crate::export::ExportError {
    crate::export::ExportError(format!("{code}: {why}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for r in all() {
            assert!(seen.insert(r.code.clone()), "{} twice", r.code);
            assert!(r.code.starts_with("XVAL-") || r.code.starts_with("L1-"), "{}", r.code);
            assert!(
                !r.spec.is_empty() && !r.message.is_empty() && !r.title.is_empty(),
                "{}",
                r.code
            );
            if r.code.starts_with("L1-") {
                assert_eq!(r.severity, Warning, "{}: placement rules are never errors", r.code);
            }
        }
        assert!(l1_rules().len() >= 49, "l1_rules.json read");
    }

    #[test]
    fn a_finding_takes_its_rules_severity() {
        assert_eq!(Finding::new("XVAL-REG-1", "x").severity, Error);
        assert_eq!(Finding::new("XVAL-REG-4", "x").severity, Note);
        assert_eq!(Finding::new("L1-RU-004", "x").severity, Warning);
        assert!(refusal("XVAL-REG-3", "taken").0.starts_with("XVAL-REG-3: "));
    }
}
