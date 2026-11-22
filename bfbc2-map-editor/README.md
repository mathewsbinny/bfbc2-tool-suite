# bfbc2-map-editor

The BFBC2 Tool Suite's **map editor** (`bfbc2-editor.exe`): view any map of the game copy; edit a copy of it as a
**map project** (objects, gameplay, the AI workspace's navigation edits, layers, undo); see it as the game draws it;
export it as a new multiplayer level for any era (BFBC1, BFBC2, Vietnam, WW2) and any mode its era offers, and
install or uninstall that level through Project OPFOR BFBC2. It is written in Rust, with egui on wgpu (Vulkan,
DirectX 12 or a software adapter), and its game-accurate view draws with Direct3D 11.

This folder holds the editor's documents; its code is in the suite's `rust\` workspace:

| File or folder | What |
|---|---|
| [EDITOR_ARCHITECTURE.md](EDITOR_ARCHITECTURE.md) | the design document: rulings, architecture (3.1 - 3.12), order of work (section 5), the older GUIs folded in (section 9: the Map Inspector, the Navigation Studio, the mesh viewer, DbxEditGUI, LevelDiff), the planned AI workspaces (section 10: tactical authoring, the bot AI debugger, the AI behaviour simulator) |
| `..\rust\bc2-editor` | the application, binary `bfbc2-editor.exe`: the window, the views, the workspaces, Help, the Export window |
| `..\rust\bc2-levelwrite` | the level writer and the map project (`<name>.mapedit.json` and its operation log), the export, and the `mapexport.exe` command line |
| `..\rust\bc2-render` | the game-accurate view: DICE's own Direct3D 11 shaders, textures and light |
| `..\rust\bc2-scene` | the scene the views draw and edit: entities, collision, terrain and water, picking |
| `..\rust\bc2-formats` | the game's file formats the editor reads (DBX, FBRB, terrain, water, collision ...) |

The editor grew out of the Python **Map Inspector**, retired in round GR1 (2026-11-20); the **Navigation Studio**
became its AI workspace in round GR2 (2026-11-20). What each had and where it went: EDITOR_ARCHITECTURE.md 9.1 and
9.2; when they went and how to get them back: [RETIRED.md](../RETIRED.md). Its rounds and their figures:
[ROADMAP.md](../ROADMAP.md) (Done) and `..\rust\BENCHMARKS.md` (one section per round); what changed for users:
[CHANGELOG.md](../CHANGELOG.md).

## Build and run

The editor has no download yet: build it with Rust (as in the suite README's "The Rust core"; the first build takes
a few minutes). From the suite's folder:

    python rust\build_native.py --editor

This builds `bfbc2-editor.exe` and `mapexport.exe` in this checkout's build folder (`rust-targets\<checkout>`, the
suite README's "Where Rust builds") and copies them to the suite's `bin\`. Then:

    bin\bfbc2-editor.exe [level | --project <name>.mapedit.json] [--opfor <folder>] [--theme dark|light] [--navmesh]
    bin\bfbc2-editor.exe --dbx <file.dbx>

| Argument | What it does |
|---|---|
| (none) | the start screen: Open project, Edit existing level, Create level |
| `<level>` | opens that level read-only, e.g. `bin\bfbc2-editor.exe mp_005` (prepared first if it needs it) |
| `--navmesh` | opens the level in the navmesh view mode with the Navigation panel (what the hub's "Open navigation" runs) |
| `--project <file>` | opens a map project on its base level |
| `--dbx <file.dbx>` | opens the DBX inspector on a binary `.dbx` file (round GR4, DbxEditGUI's successor; the hub's More tools > **DBX inspector**) |
| `--mode bfbc1\|bfbc2\|vietnam\|ww2` | the start screen opens on that set's multiplayer maps |
| `--opfor <folder>` | Project OPFOR BFBC2's folder (else `OPFOR_ROOT`, or a `project-opfor-bfbc2` folder beside or above the suite) |
| `--theme dark\|light` | the theme for this run |
| `--version` | prints the version |

From the hub (`BFBC2 Tool Suite.pyw`): Levels > **Open in Map Editor** (or double-click a level), Levels > **Open
navigation** (`--navmesh`), or More tools > **Open Map Editor**.

Without a display, `bfbc2-editor.exe <level> --screenshot out.png [--size 1600x950] [--select <guid>] [--theme
light]` draws the window offscreen and saves it (`--demo` instead of a level: a small made-up scene). The tests use
more switches with `--screenshot`: `--export` (a project's Export window) with `--install-plan` (OPFOR's install
plan for its stage; nothing is installed) or `--no-strip`, and `--new-project <level>` with `--era` and
`--target-mode` (the New project form). The editor keeps its layout, theme and bookmarks in eframe's per-user storage
under `%APPDATA%`.

On a laptop with two GPUs, Windows may give the editor the slower integrated one: set `bfbc2-editor.exe` to "High
performance" in Settings > System > Display > Graphics. Its status line shows the GPU it uses.

### mapexport

`bin\mapexport.exe` is the export without the window (`rust\bc2-levelwrite\src\bin\mapexport.rs`); Project OPFOR's
`src\tools\mapedit\mapedit_install.py` runs it to check and install a stage (`--plan`, `--apply`, `--revert <LV>`,
`--revert-all`, `--status`, `--check-game <folder>`; OPFOR's DEVELOPMENT_NOTES, "Map editor exports"). It never
writes the game copy.

    bin\mapexport.exe build   <project.mapedit.json> <export.json> --game <game folder> --out <stage folder>
    bin\mapexport.exe check   <stage folder> --game <game folder>
    bin\mapexport.exe names   <export.json> --game <game folder> [--project <project.mapedit.json>]
    bin\mapexport.exe startup <startup-00.fbrb> --out <file> --work <temp folder> [--add <LEVEL> <description.dbx>]... [--remove <LEVEL>]...
    bin\mapexport.exe overlay <overlay-00.fbrb> --out <file> --work <temp folder> [--add <stage folder>]... [--remove <LEVEL>]...
    bin\mapexport.exe awards  <ondemand_awards-00.fbrb> --out <file> --work <temp folder> [--add <stage folder>]... [--remove <LEVEL>]...

Exit code 0, or 1 with the reason on standard error (a failed check: 1, the checks printed).

## What it does

Its rounds (`ROADMAP.md`, Done): G1 - G3 (2026-11-18) the shell, the read-only map view and the Package-only levels;
E2 (2026-11-18) and E2b (2026-11-19) editing a map as a **map project**; R5 - R5c (2026-11-18 / 19) the
game-accurate view; G4 (2026-11-19) the start flow; E3 - E3d (2026-11-18 / 20) export as a new multiplayer level of
any era and mode, with Install and Uninstall through Project OPFOR; GR1 (2026-11-20) the Map Inspector folded in and
retired: Properties' *All fields* (every field of an object, nested, with its type), the *Links* panel (references,
referenced by and event connections, both ways; K draws them in the views) and Measure (M); GR2 (2026-11-20) the
Navigation Studio folded into the AI workspace and retired; EX1 (2026-11-20) every name of an export, its preview
beside the original, and the Diff panel (LevelDiff folded in and retired). The editor's exports are built and checked offline; none
has been tested in the game yet (the user paused in-game tests on 2026-11-20).

**The start screen (round G4).** Three paths: **Open project** (a `.mapedit.json`; the recent projects are listed),
**Edit existing level** (the four sets - BFBC1, BFBC2, Vietnam, WW2 - as cards, then that set's multiplayer maps:
**Open** views one read-only, **New project** starts a project on it) and **Create level** (from any existing level,
campaign levels included, whose campaign parts the export's checklist strips; or from a height map - a 16-bit PNG or
RAW of 1024 / 2048 samples a side, or 1025 / 2049, its size, height range, water level and the set its content
comes from, recorded in the project; the terrain itself is generated in a later round, E7). **Back** goes up a step.

**Editing a map (rounds E2, E2b).**

- **A project:** a new project gets a name and a folder (default `Documents\BFBC2 Map Projects`); the editor writes
  `<name>.mapedit.json` there at once and opens it in the **Objects** workspace. Opening a project loads its base
  map first. The workspaces: View, Objects, Gameplay (flags, spawns, vehicle spawns, objectives and areas only),
  AI (orders, AI spawns and area points; the navmesh view), and Terrain and Vegetation (their brushes greyed until
  their rounds). Round E2b added context menus per workspace, edit modes, sinking an object into the surface with
  Alt+wheel, deep duplicate (what an object owns comes with it), copy and paste, polygon selection, and the
  **asset browser** over the whole content set with its Spawners and Objectives modes. The project is data only - the base level, its partition's sha1, the content profile and the operation log (each
  operation with who and when, new objects named by UUIDv5 guids) - never game files; the base map is never changed.
  A project whose base level has changed (a game patch, a rebuilt WW2 level) opens with the operations naming
  objects it no longer has listed under Problems; none is dropped.
- **Problems** (round V1): after every change the project is checked against the export's rules (`mapexport
  rules` lists them; `spec/transforms/validation.md`) and DICE's placement tendencies from the stock maps - flag
  spacing, HQ distances, capture zones, spawns, combat-area margins, Rush stage spacing, Squad Deathmatch spawns -
  errors first, then warnings and notes, each with its rule's code (hover for its message and spec citations) and
  a **Go to** link that selects the object and frames it. A placement finding quotes DICE's range ("Stage 3 is 40 m
  from stage 2; DICE's Rush stages are 93-316 m apart"); it is advice, never an error. `mapexport validate
  <project> --game <game>` prints the same list. Round A1 added the ground: spawn points over water, inside rocks or
  under the ground, a vehicle spawner without room for its vehicle or out of its domain, gameplay objects outside
  every combat area, and - once the AI workspace has read the level's path data - spawns and objectives the bots
  cannot reach from the main route network.
- **Auto layout** (round A1, the Gameplay workspace's panel; Commands: Show auto layout): with an export target chosen,
  **Propose layout** lays the mode's layer (DICE's own layout from the target's donor) out on the level - turned,
  scaled and placed where the ground, water, rocks and buildings suit it, fitted spot by spot and held against DICE's
  statistics. The proposal is drawn over the views and listed with its findings and each statistic against DICE's
  range; **Accept** makes it one move operation (an undo takes it back), **Discard** drops it. A seed gives other
  proposals; the same seed the same one.
- **Select:** click (with its group), Ctrl or Shift + click adds or removes, drag a box on empty ground (Select tool,
  or any tool off the gizmo). Alt + left drag orbits (pans in the top / front / side views) while editing.
- **Gizmo** (toolbar at the top of each view; **Q** select, **W** move, **E** rotate, **R** ruler, **X** world or
  local axes): drag an arrow, a plane square or the centre to move, a ring to turn; the dragged handle turns
  orange, the arms are in the axis colours. **Snapping:** grid (**G**, its size in metres), angle (its step in
  degrees), vertex (**V**: the pivot goes to the corner under the pointer, Shift for the edge), and **surface**
  (**B**): moved objects sit on the terrain or mesh below them, sunk by the **sink depth** (metres, or a percentage
  of the object's height; along the surface normal or straight down; optionally turned to the normal). The depth is
  stored with the object's operation and kept when it is moved again; Properties shows and changes it per object
  ("Place on surface"). **End** (Snap to ground) puts the selection on the surface below it.
- **Delete** (Del: the object, its array entries elsewhere, and its event connections), **Duplicate** (Ctrl+D: a copy
  with a new guid, in `LevelData.Objects` and with copies of its event connections, 2 m along X and selected),
  **Align** on X / Y / Z (to the primary, the last selected) and **Distribute** on X / Z, **Group** (Ctrl+G) and
  **Ungroup** (Ctrl+Shift+G), from the command palette.
- **Ruler** (R): click two points (vertex snapping applies); the distance, its horizontal part and the height
  difference.
- **Layers** panel: a layer from the selection, hide, isolate, lock (locked objects cannot be selected in the views),
  colour (shown beside its objects in the outliner); objects stay in the build when hidden. **Folders** (Properties >
  Folder, `Buildings/North`) and groups are listed at the top of the outliner.
- **Properties:** position and yaw as numbers, the surface placement, the layer and folder, and the object's own
  fields typed from the level's data (`dbxcatalogue`'s rules: text with the values the level uses, yes / no, whole
  numbers, decimals; a number the data does not say is whole or decimal is shown, not written).
- **History** panel: every operation with who and when; click one to undo or redo to it. **Ctrl+Z** / **Ctrl+Y**
  (or Ctrl+Shift+Z). **Ctrl+S** saves; every change is written at once to `<name>.mapedit.json.autosave`, and a
  project opened with an autosave that differs from it offers to recover it. A project is open in one editor at a
  time (`<name>.mapedit.json.lock`, round QW2): a second editor refuses it, and a save refuses to overwrite a file
  another editor or tool changed meanwhile (your changes stay in the autosave). **Check edits** (palette) applies the
  whole log to the base partition again and says what it changes.

The edits are written by `rust/bc2-levelwrite` (levelwrite: one mutation API on the level partition, the port of
`dbxbin`; README of the crate in its `lib.rs`): an untouched partition writes back byte for byte, a moved object
changes only its `Transform`, and a delete, duplicate or field set only what it names (proven on MP_005, SP_01 and
Guadalcanal against DbxEdit and the Python `dbxbin` reference: `rust/BENCHMARKS.md`, "E2").

**The game-accurate view (rounds R5 - R5c).** "Game-accurate view" (command palette) draws the 3D view as the game
draws it: DICE's own Direct3D 11 shaders, textures and the level's light, pixel for pixel against the C++ render
harness. Since R5c it is interactive and editable: edits show at once and picking works in it. Its known gaps (Wake
Island's black terrain cells, the sky pass, game LODs ...) are round R5d (`ROADMAP.md`).

**Exporting a map (rounds E3 - E3d).** "Export as a multiplayer level" (command palette) opens the Export window:

- **The target:** the era (BFBC1, BFBC2, Vietnam, WW2) and the game mode are project settings, also chosen in the
  New project form. Only the modes the era's stock levels have are offered (WW2: Conquest; Onslaught and Air
  Superiority are listed greyed with the reason). Choosing a target brings the donor level's mode layer into the
  project as ordinary, editable objects (flags, bases, M-COMs per Rush stage, spawns, combat areas) and takes the
  base's own layer out, as one undo step; "Use selection" moves a flag, base or M-COM with what belongs to it.
- **The export:** the form (map name, level id, the target) is kept as `<project>.export.json`; its checks run as it
  changes and Export stays disabled, with the reason, while one fails. Export builds the level on a worker into a
  stage folder in Project OPFOR's `work\mapedit\` - the level's bundles, its own name and description in every
  language, its loading and map-list pictures, the target era's kits, teams and announcer voice-over, and what the
  level needs from other sets' bundles copied into its own (`closure.txt`) - then runs `mapexport check` and shows
  the report. The same from a command prompt: `bin\mapexport.exe build <project> <export.json> --game <game> --out
  <stage>` and `bin\mapexport.exe check <stage> --game <game>`.
- **Install and Uninstall:** "Install..." runs Project OPFOR's `src\tools\mapedit\mapedit_install.py --plan` and shows
  what an install changes; "Install now" (the game closed) runs `--apply`; "Uninstall..." runs `--revert <LV>`. The
  editor itself never writes the game copy.

- **The names (round EX1):** the Export window's Names section edits every name of the level - the **level id**
  (what the game loads, `Levels/<ID>`), its **folder** (`Package\levels\<id>\`: the id in lower case, as on every
  level of the game; renaming the folder renames the id), the **server-browser name** (`maps.ini`: the launcher's row
  and the browser's `OPFOR - <name> (<mode>)`) and the **on-screen name and description per language** (the menus,
  the loading screen, the browser's map column; each language's own letters). They are checked together, with the
  reason in words: no stock level's id or another project's export, no `_NAME` inside the id (a black loading
  screen), no text or loading picture the game already has, characters and lengths the files hold. **Keep in
  project** (and Export) puts them in the project's log. `bin\mapexport.exe names <export.json> --game <game>` checks
  them from a command prompt.
- **Preview export (round EX1):** after an export, "Preview export" opens the stage read only beside the original
  base level, with the same camera; **T** switches between them; Close preview gives the project back.
- **The Diff panel (round EX1, LevelDiff folded in):** **Compare with base** (the project against its base) and
  **Compare with level** (against any other level): every object added, removed, moved or changed, filtered by kind,
  group (mode layer, from other sets, campaign content, world) and text; a click selects it and flies there, its
  fields are listed, Inspect opens All fields. The 3D view tints the differing objects and marks with a box where an
  object of the other level stands. From a command prompt: `bin\bfbc2.exe leveldiff <a.dbx> <b.dbx>` (LevelDiff's
  arguments and output).

Six canary exports (Aurora as WW2 Conquest with WW2 objects, Atacama Rush as Conquest, Wake Island as BFBC2,
a Vietnam map as WW2, Valparaiso as Vietnam Squad Deathmatch, Guadalcanal as BFBC2 Rush) pass the check and install
/ revert on a scratch copy of the game's shared files (`rust/BENCHMARKS.md`, "E3d"). Not yet tested in the game.

**The AI workspace's navigation (round GR2).** The navmesh view mode (`--navmesh`, or the hub's Open navigation)
and the AI workspace draw a level's path data: NavKit's newest build and DICE's file side by side, the walkable
ground coloured by island, sector, pitch, difference or anomaly, and each file's outlines, portals, route graph,
ladder ground and ladder links as layers. Panels: **Navigation** (the files and their figures, the colouring, the
layers, the selected area or spawn, the anomaly overlay, Compare with a reference level's AI layer, Export JSON),
**Islands and spawns** (largest first, the spawns off the main network, each to fly to) and **Navigation edits**.
The **path test** (Shift+P, or "Path test from here" on the ground) finds each file's route between two clicks, or
says why there is none. In a project, navigation edits (walkable and unwalkable regions, seeds, linked areas, route
edges, traversals, notes) are operations of the project's log, saved with it and as the level's side file
(`navtune.json`); **Build with these edits** runs NavKit's apply in the editor itself (the code `bfbc2.exe navkit
apply` runs: the same files and lines, shown as they come, with **Cancel the build**), and **Install into the game** /
**Restore DICE's original** show what they write in the game copy's AI path data before they run. Placing AI
spawners and orders, routes and the cross-domain links come later (EDITOR_ARCHITECTURE.md 3.4a, step E5a); the
tactical authoring, bot AI debugger and AI behaviour simulator workspaces after that (section 10).

**Viewing a map (rounds G1 and G2).**

- **Opening a level:** the start screen's levels are the game copy's (found as the suite finds them: `--opfor`,
  OPFOR_ROOT, or a `project-opfor-bfbc2` folder beside or above the suite). A level NavKit has not
  prepared yet, or whose terrain unpack is missing or older than its bundle, is **prepared first, in the editor**:
  "Preparing MP_005 - Step 2 of 4: Unpacking the level", a progress bar, the tools' last line and Cancel, then the
  map opens. The steps are the suite's own, run by `bin\bfbc2.exe prepare_level` (its Python twin
  `prepare_level.py` was retired in round RT1: the editor needs `python rust\build_native.py`, or `--download`, once):
  NavKit's prepare (the shared bundles, the level's bundles, its
  objects, collision and spawn points; 10 - 60 s since round E4e) and the terrain unpack (`level.heightfield_of`'s,
  once the Map Inspector's). They write
  only OPFOR's `work\levels\<level>` and `work\navkit`, never the game copy; a failure is said in plain words on the
  start screen.
- **The set cards (round G3, the user's design; since round G4 behind "Edit existing level", and only multiplayer
  maps listed):** four cards - BFBC1, BFBC2, Vietnam, WW2 - each with its installed map count and its own picture, the game's loading screen read from the game copy at run
  time (`awards/images/loadingscreens/` in `ondemand_awards-00.fbrb`: DICE's `bc1_oasis`, `mp_005` and `nam_mp_002`,
  OPFOR's converted `wake_island_s` for WW2; a set without it gets a plain tile, a set with no installed map is greyed
  "Not installed"); four in a row, two by two in a narrow window.
  A card opens that set's maps: Back, the set's name and a search box, a row per map with its picture
  (`awards/images/levels/<level>.res`), name, game mode (its description's `GameMode`), prepared or not, **Open** and
  **New project** (the name, folder, era and mode). The sets come from the
  levels' data: the `bc1_` maps of `game\Package` (badge BFBC1), the levels loading `nam_common` (Vietnam) or
  `ww2_common` (WW2), every other level BFBC2. No game art is kept in the repository: the pictures are decoded
  once into Project OPFOR BFBC2's `work\editor\art` (`art.json` records the bundles' sizes and times; made again when
  they change; about 1 MB) - 4.8 s the first time, 0.1 s after.
- **Levels in `game\Package` only (round G3):** Vietnam's (`NAM_*`, badge VIETNAM), the Pacific War's (Wake Island,
  Coral Sea, Guadalcanal, Iwo Jima ...; badge WW2 - PACIFIC WAR) and DICE's other Package-only levels (the Bad
  Company 1 maps, extra mode layouts such as MP_005GR) are listed too, when installed: the game's registry
  (`game\Dist\win32\package.mft`) names the level and every bundle it loads is there. Pacific War and Vietnam are
  optional; a level that is not installed is simply not listed. Opening one prepares it in the editor itself, in
  Rust and in memory (no Python, no whole unpack): the bundles the level loads are read in the game's order (its own,
  then `nam_common` / `ww2_common` / `mp2_common`, then `mp_common`; a `*deleted*` marker hides a later bundle's
  file, as in the game) and only what the map view opens is kept - the level partition and static groups, the placed
  blueprints, their collision and `Mesh` aliases, the terrain and water. 2 - 5 s and 8 - 20 MB per level; opening it then takes a third of a second
  (rust/BENCHMARKS.md, G3: all 45 Package levels of OPFOR's game copy, each the same as a whole unpack loaded by the
  suite's level loader).
  **Where it goes:** `work\editor\levels\<level>\` - deliberately **not** `work\levels`, whose folders several tests
  compare against recorded corpora. It is a cache (`python -m bfbc2core.caches --list` shows it as `editor`;
  `--prune editor` / `--clear editor` trim it; `BFBC2_EDITOR_CACHE_MB`, default 2048): each level's
  `editor-level.json` records the bundles' sizes and times and every file, and a copy whose bundles changed or with a
  file missing is made again on its next opening. The editor drops the least recently opened copies beyond the cap.
- **Teams by faction:** labels, the outliner and Properties name each side's faction as the level's data says it
  (`LevelDescription.FactionTeam1` / `FactionTeam2`, through the content profile): US / RU on BFBC2 levels (RU / US
  where the Russians are team 1, as on MP_012GR), US / NVA on Vietnam's, US / JP on the Pacific War's; T1 / T2 where
  the level names no faction (the campaign, squad deathmatch), and Properties says so. Gameplay no team owns
  (objectives, combat areas) is violet (`unowned`), team-neutral flags light grey with NEU.
- **Help** (the top bar's Help menu, or F1): controls and shortcuts, a getting-started guide, what each workspace is
  for; searchable. It is generated from the action registry and the views' control list, so it cannot go stale.
- **The window:** docked panels (drag a tab to dock, tab or float it; **Reset layout** puts them back): the Outliner
  (the map by kind and group, searchable, Hide / Show per group), the views, Properties (the selection's type, GUID,
  team, blueprint, source partition and element, transform), Problems, and with a project History and Layers. The
  top bar has the workspace switcher (View; with a project Objects, Gameplay, AI, and Terrain and Vegetation greyed),
  the level or project and its content profile
  badge, Save / Undo / Redo, Help and the commands; the theme (Dark or Light) follows
  the suite's design language (`design/project/tokens.json`, IBM Plex fonts). The 3D view draws with its own tokens
  (`scene-*` for the sky, terrain, water and object greys; `view-*` for team, unowned, selection and status colours,
  each at least 3.5:1 on the lit terrain in both themes).
- **The views:** the 3D view and top, front and side views (top bar, or Alt+1 - Alt+4), each with its own camera.
  3D view: left drag orbits, middle drag pans, the wheel zooms, hold the right button to fly (W A S D, Q / E, Shift
  faster); the orthographic views pan with the left or middle button. Click selects (the selection is outlined in
  orange; the status bar shows the surface point under the click), double click also focuses, **F** focuses every
  view on the selection, **Esc** clears it, **L** shows or hides the labels. Ctrl+1 - Ctrl+9 keep a camera bookmark,
  1 - 9 recall it (per level and view).
- **Command palette:** Ctrl+Shift+P lists every command with its shortcut; type to filter, Enter runs.

## Tests

Its tests (`python tests\test_editor.py`, quick tier) prove it headless: the theme generated from every token in both
themes; the outliner against the retired Python Map Inspector's lists for SP_01, MP_005, MP_009GR and ONS_MP_004
(recorded before its retirement by `rust\bc2-editor\tests\record_inspector.py`, entity by entity: order, GUID, category and layer, team, position); the
view drawn offscreen (not blank, the outline in the `selection` colour and nowhere else, picking, a click selecting,
the command palette opening a top view) and each of the four levels drawn with a flag selected; round GR1's property
tree, references and referrers against what the Map Inspector showed for 1,074 objects of the same four levels
(`tests\inspect_parity.rs`, its answers kept as digests) and its Links, All fields and Measure driven headless
(`tests\gr1.rs`). Round G2's
(`tests\g2.rs`, `tests\factions.rs`): every `view-*` / `scene-*` token found in a rendered frame, the overlays'
contrast measured on the frame's own pixels, panel contrast and the third neutral's hue distance, the faction labels
on BFBC2, Vietnam and WW2 levels (read only), Help generated from the registry (an action without a description
fails), and opening a level that needs preparing (a stand-in script: its steps, then the level; a failure in words).
`BC2_EDITOR_PREPARE=<level>` also runs the real preparation on a stand-in OPFOR folder in temp.
Round E2 adds levelwrite's proofs (`cargo test -p bc2-levelwrite`: the project log, its file, undo / redo, autosave and recovery,
the base-changed report; edits on MP_005, SP_01 and Guadalcanal copied to a temp folder: they parse, differ from the
base exactly where the operations say, round-trip through the Rust DBX writers, load in the scene with each object
where the log put it, and equal DbxEdit's `drop` / `setfield` and the Python `dbxbin` reference byte for byte), the
surface placement with sink depths on terrain and on a mesh, the gizmo's maths, and the editing interface driven
headless (`tests/editing.rs`: click, a 5 m drag on the grid, undo, redo, a 15-degree turn, box selection, duplicate,
delete, snap to ground sunk 0.5 m and kept on the next move, a layer, save and reopen).
The later rounds' tests sit beside them: `tests\e2b.rs` and `tests\asset_browser.rs` (E2b), `tests\g3.rs` and
`tests\g4.rs` (the start screens), `tests\game_view*.rs` (R5 - R5c), and in `rust\bc2-levelwrite\tests` the exports
(`export_sp01.rs`, `export_e3b.rs`, `export_e3c.rs`, `export_e3d.rs`, `export_ex1.rs` - the names), `tests\ex1.rs`
(the Names section, the preview, the Diff panel) and `rust\bc2-cli\tests\leveldiff.rs` (`bfbc2 leveldiff` against
the retired LevelDiff.exe's recorded answers); each round's figures are in its
`rust/BENCHMARKS.md` section.
