# The Frostbite 1 / Battlefield: Bad Company 2 specification

A formal description of the Frostbite 1 engine's data, as Battlefield: Bad Company 2 (BFBC2) ships and loads it,
and as the BFBC2 tool suite reads, writes and transforms it. Its purpose is the suite's quality: every reader and
writer is held to a rule written down here, every rule names the code that implements it and the test that checks
it, and what is not known yet is listed as an open question instead of being guessed silently.

The specification is the suite's own description. The format knowledge comes from the suite's readers and writers
(themselves built on the BC2 modding community's research, first of all Frankelstner's), their tests, the rounds
recorded in `rust/BENCHMARKS.md`, and Project OPFOR BFBC2's research notes. **It contains no EA game data**: no
extracted strings, textures or bytes. It describes structure, counts and observed value ranges; worked examples use
data the suite's tests build.

## Scope

| Layer | What | Where | State |
|---|---|---|---|
| 1 | Container and file formats: byte layouts, encodings, platform variants | `formats/` | round SP1 |
| 2 | The data model: DBX types and values, instances, GUIDs and references, partitions and bundles, the level structure, registration, precedence | `model/` | round SP1 |
| 3 | Engine subsystems: what the game does with the data (loading and the checksum, levels, gameplay and modes, AI / Kynapse, rendering, sound, networking and hosting, the menus) and the crashes it gives on bad data | `engine/` | round SP2; physics and streaming later (plan below) |
| 4 | The suite's transformations: each conversion and port as a function from input to output with its pre- and postconditions and the tests that prove them | `transforms/` | round SP2 started it; grown by each porting round (plan below) |

The platforms and games covered:

- **PC**: BFBC2 for Windows (`BFBC2Game.exe`, `game\` in Project OPFOR BFBC2): little-endian resources, Direct3D
  9 / 10 / 11 shaders. The default when a rule names no variant.
- **Xbox 360**: BFBC2 for the Xbox 360 (the console's `mp2_common` / level bundles, Onslaught): big-endian resources,
  Xenos GPU tiling and microcode.
- **BF1943**: Battlefield 1943 (Xbox 360 / PS3, Frostbite 1.5), the source of the Pacific War assets: the same
  containers, an older Havok, its own DBX classes.
- **PS3** is out of scope unless a rule says otherwise.

## Reading a rule

Every normative statement is a numbered rule:

```
### FBRB-HDR-2: Header length

Status: **verified** · Variants: PC, Xbox 360, BF1943

Bytes 4-7 are a big-endian u32: the length of the stored (gzip) header that follows. ...

Evidence: [test: rust/bc2-formats/src/fbrb/header.rs::read_header_refuses] [code: rust/bc2-formats/src/fbrb/header.rs:173-178]
```

- **The ID** is `<AREA>-<SECTION>-<n>`: the area is the format or model topic (`FBRB`, `DBX`, `TERR`, `REF` ...),
  the section a part of it (`HDR` header, `ENT` entries, `STR` strings ...), `n` counts from 1 within the section.
  IDs are stable: a rule that turns out wrong is corrected in place or marked **withdrawn** in its text, never
  renumbered, and a new rule takes the next free number. Cite a rule by its ID anywhere (code comments, commit
  messages, other rules).
- **The heading** (level 3 or 4) is `<ID>: <title>`. Headings without an ID are commentary.
- **The status line** says how far the rule is proven:

  | Status | Meaning | Needs at least |
  |---|---|---|
  | **verified** | a test checks it (on built data, recorded answers, or the game's files) | one `[test: ...]` |
  | **implemented** | a reader or writer relies on it and works on the game's files, but no test checks this rule directly | one `[code: ...]` |
  | **observed** | seen to hold on real files (counts given) or in the executable; no code depends on it | one `[data: ...]` or `[exe: ...]` |
  | **inferred** | reasoned from other facts, not observed directly | one `[inferred: ...]` |

  `Variants:` lists the platforms or games the rule covers when it is not every one (PC, Xbox 360, BF1943).
- **Evidence** items, anywhere in the rule's body:

  | Item | Meaning |
  |---|---|
  | `[test: path::name]` | the test: a Python `def` / `class` or a Rust `fn` named `name` in `path` |
  | `[code: path:line]` or `[code: path:first-last]` | the reader or writer that implements the rule |
  | `[data: ...]` | what was observed on real files, with counts (structure only: never content) |
  | `[exe: ...]` | a `BFBC2Game.exe` address or function (PC retail executable) |
  | `[inferred: ...]` | the reasoning |
  | `[doc: path:line]` | a document recording the finding (supporting; counts toward no status) |

  Paths are relative to the suite's root. `opfor:` before a path is Project OPFOR BFBC2's checkout
  (`opfor:research/MATERIAL_SLOTS_RE.md`). Line numbers are those of the commit that last ran `trace.py --emit`;
  `trace.py --check` refuses a line past a file's end, so a moved line is caught when a file shrinks, not always when
  it moves: re-check citations of a file you restructure.
- **Data counts** name their corpus. "The game copy" is Project OPFOR BFBC2's `game\` (DICE's PC files plus the
  bundles OPFOR installs: Vietnam, the Pacific War, Onslaught), counted on 2026-11-19 unless the rule says
  otherwise; "stock" means DICE's files only.
- **Must / must not** say what the game requires (it refuses, crashes or misbehaves otherwise); **is / holds** say
  what the files show; **the suite** names what the suite's tools do where the game's behaviour is not known.
- Numbers: offsets and sizes in bytes, offsets from the start of the structure named; `u8` / `u16` / `u32` / `u64`
  unsigned, `i32` signed, `f32` IEEE 754 single; **BE** big-endian, **LE** little-endian.

Each file ends with **Open questions**: what is not known yet, so nobody mistakes a gap for a rule.

## The traceability index

`rules/` lists every rule (ID, title, file, status, variants, evidence), one JSON shard per markdown file at the
same path (`engine/gameplay.md`'s rules in `rules/engine/gameplay.json`), written from the markdown by
`python spec/trace.py --emit` and checked by `python spec/trace.py --check` (in `run_tests.py quick`, which CI runs).
Each shard is `{"about", "rules"}` with sorted keys and the rules in the order of their headings, so a rule's diff
lands beside its markdown file's. Until 2026-11-27 the index was one file, `rules.json`; it reached 977,029 bytes
against `tests/test_repo.py`'s 1,000,000-byte limit, and round SPLIT1 split it (the shards' union is its content, rule
for rule; the largest shard is about 40 KB). After merging a branch that still writes `rules.json`, delete it and run
`--emit`.

- every rule in the markdown is in `rules/` and the other way round, with the same title, file, status and
  evidence;
- every shard is exactly what `--emit` writes (line endings aside) (a hand-edited, stale, missing or leftover shard fails), and no
  shard passes 800,000 bytes (80% of the limit; split the markdown file then);
- every status is backed by its evidence (table above);
- every cited test exists (the file and the `def` / `fn`), every cited code or doc file exists and has the line;
- every `spec: ID` comment in a test names an existing rule;
- every rule ID the prose names is defined, except IDs of the map editor's placement study (`L1-CQ-001`, `L1-RU-004s`
  ...), which must be in `bfbc2-map-editor/placement/l1_rules.json` (`EXTERNAL` in `trace.py`).

`python spec/trace.py --coverage` reports rules per status and area, the rules without a test, and the suite's
tests that cite no rule. A test cites a rule when a rule names it as evidence, or with a comment on or directly above
its definition:

```python
# spec: FBRB-HDR-1, FBRB-HDR-2
def test_damaged_headers():
```

```rust
// spec: DBX-VAR-1
#[test]
fn varint_shortest_form() {
```

After editing rules: `python spec/trace.py --emit`, then `--check`.

## Contents

| File | Covers |
|---|---|
| [glossary.md](glossary.md) | terms |
| **Containers** | |
| [formats/fbrb.md](formats/fbrb.md) | `.fbrb` bundles: header, entries, payload, padded layouts, the manifest |
| [formats/res.md](formats/res.md) | resource entries: the type catalogue, names and extensions, per-type wrappers |
| [formats/game_folder.md](formats/game_folder.md) | the files outside the bundles: the census's kinds, the aerial height map, `database.dbmanifest`, `dfe`, the patcher's record, what is out of scope (SPG4) |
| [formats/bigf.md](formats/bigf.md) | BIGF / BIG4 archives (menu movie resources) |
| [formats/stfs.md](formats/stfs.md) | Xbox 360 STFS packages (title updates, DLC) |
| [formats/xiso.md](formats/xiso.md) | Xbox 360 disc images (XDVDFS) |
| **Data** | |
| [formats/dbx.md](formats/dbx.md) | binary DBX: header, string table, element tree, numbers; the XML form; writing and import |
| [formats/xmlinst.md](formats/xmlinst.md) | the XML instance reader |
| **Terrain, water, physics** | |
| [formats/terrain.md](formats/terrain.md) | TerrainHeightfield, Terrain, VisualTerrain, TerrainMaterialMap |
| [formats/water.md](formats/water.md) | WaterMesh, VisualWater |
| [formats/havok.md](formats/havok.md) | HavokPhysicsData: DICE's wrapper, the Havok packfile, MOPP |
| [formats/collision.md](formats/collision.md) | collision as the level's objects use it |
| **Meshes, textures, shaders** | |
| [formats/mesh.md](formats/mesh.md) | MeshData, mesh sets (Rigid, Composite, Skinned, Tree), PC and Xbox 360 |
| [formats/texture.md](formats/texture.md) | ITexture / XenonTexture: header, PC layout, Xenos tiling |
| [formats/granny.md](formats/granny.md) | Granny models and animations |
| [formats/shaderdb.md](formats/shaderdb.md) | Dx9 / Dx10 / Dx11 shader databases, vertex layouts, Xenos microcode |
| [formats/occluder.md](formats/occluder.md) | OccluderMesh: the occlusion stand-in meshes (SPG4) |
| [formats/soot.md](formats/soot.md) | SootMesh: the explosion soot surfaces (SPG4) |
| [formats/animtree.md](formats/animtree.md) | AnimTreeInfo: compiled animation trees' header, blocks and pointers (SPG4) |
| **Navigation** | |
| [formats/hpd.md](formats/hpd.md) | Kynapse path data (`.hpd`) and its kymesh / graph blocks |
| [formats/kynapse.md](formats/kynapse.md) | Kynapse path data's meaning: worlds per type, sectors, tiles and resolution, areas, portals, links, the route graph, units and frames |
| [formats/navsample.md](formats/navsample.md) | the suite's navigation samples |
| [formats/culltree.md](formats/culltree.md) | static cull trees |
| **Menus, sound, text** | |
| [formats/apt.md](formats/apt.md) | APT / SwfMovie menu movies |
| [formats/audio.md](formats/audio.md) | Wave resources, EA-XAS audio, impulse responses (SPG4) |
| [formats/bink.md](formats/bink.md) | Bink videos: BinkMemory resources and the movies outside the bundles (SPG4) |
| [formats/loc.md](formats/loc.md) | localization tables |
| [formats/aimanim.md](formats/aimanim.md) | AimAnimation |
| [formats/bf1943.md](formats/bf1943.md) | BF1943 as a source: where it is read from, its bundles and levels, the class differences DICE resolved; an index of the BF1943 rules elsewhere (round SPG2) |
| **The data model** | |
| [model/types.md](model/types.md) | the DBX type system and value encoding |
| [model/references.md](model/references.md) | instances, GUIDs, references in and across partitions |
| [model/bundles.md](model/bundles.md) | partitions and bundles, `*deleted*` markers, bundle precedence |
| [model/level.md](model/level.md) | the level: LevelData, its description, layers, Objects, event connections, model groups |
| [model/registration.md](model/registration.md) | `package.mft`, `bundleManifest`, the level checksum |
| [model/ai_orders.md](model/ai_orders.md) | AI orders: the fifteen order classes and their fields, whom an order commands, issuing and chaining, the readiness and urgency data, why no order knows a game mode |
| **Engine subsystems (layer 3)** | |
| [engine/loading.md](engine/loading.md) | the load's session states, the startup bundle, the level checksum (Md5Buffer), partitions and references at load, model groups |
| [engine/levels.md](engine/levels.md) | level ids, mode levels, the level data at run time, what a description declares |
| [engine/gameplay.md](engine/gameplay.md) | modes and mode layers, Conquest, tickets, Rush stages, Squad Deathmatch, spawns, combat areas, DICE's measured conventions (the L1 study) |
| [engine/ai.md](engine/ai.md) | the AI's start, the Kynapse runtime, what a level needs for bots, AI crashes |
| [engine/kynapse_runtime.md](engine/kynapse_runtime.md) | the Kynapse runtime from the executable: configuration, loading, point to sector / tile / area / node, search costs, straight moves, sector links, dynamic edges, what it ignores, crashes |
| [engine/bot_ai.md](engine/bot_ai.md) | bot AI: the AI frame and its time-sliced passes, the bot and player proxy, goals / behaviours / squad behaviours, the control boundary (the AI player, its input object, the actions it sends), perception, combat, movement, vehicles, squads, the game state a bot reads, difficulty; the implementation interface for OPFOR's bot module and the RE still needed |
| [engine/rendering.md](engine/rendering.md) | the Direct3D 11 renderer: shader databases, draws, order, terrain, water, weather, post-process, shadows |
| [engine/audio.md](engine/audio.md) | waves, streaming and on-demand audio, the announcer per content set, voice-over events |
| [engine/network.md](engine/network.md) | the in-process host (`hostMultiplayerLocal`), the transports (in-memory, UDP, the online factory), the connection layer, network settings and start-up options, versions and disconnect reasons, PunkBuster, the server's listen path, the join by address, EA's Plasma back-end and its LAN mode, the server browser |
| [engine/network_feasibility.md](engine/network_feasibility.md) | study (no rules): what locally hosted play, Direct IP, VLAN and LAN would need, their risks, effort, order and the tests that would prove them |
| [engine/menus.md](engine/menus.md) | the level catalogue, engine-movie events, movie loading and unloading |
| [engine/crashes.md](engine/crashes.md) | index: every recorded fault address and its rule |
| [engine/lighting.md](engine/lighting.md) | baked occlusion, irradiance volumes, reflection cubes, the outdoor light, point lights (`ActAsPointLight`, `PointLightEntityData`), sky props, DICE's night rules (round SPG) |
| [engine/weather.md](engine/weather.md) | the weather system's classes, the 355 states, DICE's value ranges, weather effects and sounds, what the export writes (SPG) |
| [engine/effects.md](engine/effects.md) | effect and emitter classes, per-level emitters and atlases, placed and camera-proximity effects (SPG) |
| [engine/weapons.md](engine/weapons.md) | weapon, firing and projectile data, DICE's class conventions, patched balance, the WW2 ruling (SPG) |
| [engine/vehicles.md](engine/vehicles.md) | vehicle entities, physics, health and armour, seats, weapons, spawners, BF1943 and console vehicles (SPG) |
| [engine/materials.md](engine/materials.md) | the material grid, part and face materials, penetration, impact effects, grime and camouflage textures (SPG) |
| [engine/destruction.md](engine/destruction.md) | health states, parts and bangers, destroyed meshes, terrain destruction, model-group members (SPG) |
| [engine/persistence.md](engine/persistence.md) | stats, ranks, unlocks and licences offline; OPFOR's hooks (SPG); the `Persistence/*.dbx` classes, BFBC2's and BF1943's ranks and awards, the Pacific War table (SPG2) |
| [engine/vegetation.md](engine/vegetation.md) | forests, middlegrowth and the terrain's undergrowth: classes, the undergrowth entry, OPFOR's stand-ins (SPG2) |
| [engine/decals.md](engine/decals.md) | the terrain's baked road and decal strips, DICE's decal sources, impact decals (SPG2) |
| [engine/animation.md](engine/animation.md) | the soldier skeleton, compiled animation trees and their baked hand IK, zoom rules, kits, skinning kinds, the ragdoll (SPG2) |
| [engine/module.md](engine/module.md) | OPFOR's game module: every patch point in `BFBC2Game.exe` (221 addresses), how hooks are checked and installed (SPG2) |
| [engine/onslaught.md](engine/onslaught.md) | Onslaught: the mode in DICE's data, what the PC engine does with it, Napisal's and OPFOR's console ports (SPG2) |
| [engine/streaming.md](engine/streaming.md) | the async, streaming and on-demand bundles: mounting, contents, registry, the suite (SPG2) |
| [engine/settings.md](engine/settings.md) | options and settings: `settings.ini`, `GameSettings.ini` and its defaults partition, the engine settings objects, the command line, the options screens' getters and commands (SPG3) |
| [engine/input.md](engine/input.md) | the input system: entry input actions, concepts, device bindings, mappings, the user's binding files, the controls screen, the PC pad path, tuning data, OPFOR's controller patch (SPG3) |
| [engine/scripting.md](engine/scripting.md) | scriptability: the authoring layers and their reach, the level event graph's classes, data entities that reach the engine, consoles and server administration, what a modder scripting layer needs (SPG3) |
| [engine/soldier.md](engine/soldier.md) | the soldier: the character controller's states and poses, sprint, collision, cameras and view recoil, the field of view, aim assistance, spotting (SPG4) |
| [engine/squads.md](engine/squads.md) | teams, squads and players: team data, squad assets and ghosts, the player's squad system's names (SPG4) |
| [engine/hud.md](engine/hud.md) | the HUD's data: configuration, menu movies and tips, the minimap and its icons, markers, screen effects, the spectator, the kill cam (SPG4) |
| [engine/campaign.md](engine/campaign.md) | the single-player campaign: objectives and save points, fades and restrictions, triggers, cutscenes, waypoints and ambient life, descriptions and presence (SPG4) |
| [engine/text.md](engine/text.md) | text: bitmap fonts and their glyphs, the text databases, UI atlases, subtitles (SPG4) |
| [engine/sound_graph.md](engine/sound_graph.md) | sound graphs (ports, samplers, processing and output nodes), voice-over trees and trigger tables, the mix and the sound system (SPG4) |
| [engine/internals.md](engine/internals.md) | engine internals: the executable's `typeinfo` records, memory groups, heaps and pools, the job system, the menu background scene (SPG4) |
| [engine/data_classes.md](engine/data_classes.md) | the remaining DBX classes by system: emitter update processors, tree parameters, vehicle physics components, gadgets, the AI's constants, matchmaking, edge models (SPG4) |
| [engine/typeinfo/core.md](engine/typeinfo/core.md) | the executable's reflection record by record: how each type is registered, the member descriptors, the run-time members against DICE's DBX fields (the pipeline-only ones), `Core` (SPG4-rest) |
| [engine/typeinfo/game_shared.md](engine/typeinfo/game_shared.md) | `GameSharedResources`' 777 records: enumerations with their numbers, value types, entity data, weapons, AI, UI, persistence, the rest; every member's run-time offset (SPG4-rest; tables written by `probes/typeinfo.py`) |
| [engine/typeinfo/gamedll.md](engine/typeinfo/gamedll.md) | `GameDll`'s run-time entities: their hierarchy and sizes, and which data class's factory makes each (SPG4-rest) |
| [engine/typeinfo/modules.md](engine/typeinfo/modules.md) | the engine's other 19 modules, record by record (SPG4-rest) |
| **The suite's transformations (layer 4)** | |
| [transforms/pack.md](transforms/pack.md) | FBRB unpack and pack |
| [transforms/dbx.md](transforms/dbx.md) | DBX read / write round trips, import, DbxEdit's commands |
| [transforms/culltree.md](transforms/culltree.md) | cull-tree additions and the export's reslot |
| [transforms/console.md](transforms/console.md) | Xbox 360 -> PC conversion and the Onslaught port's stages |
| [transforms/ww2.md](transforms/ww2.md) | BF1943 -> BFBC2 ports and the WW2 rebuild (the oracle) |
| [transforms/navkit.md](transforms/navkit.md) | NavKit build, apply, install, restore |
| [transforms/kynapse_generation.md](transforms/kynapse_generation.md) | DICE's generator as rules measured on its output, with NavKit's gap per rule |
| [transforms/stockmp.md](transforms/stockmp.md) | the stock-map AI layer: build, checks, install, revert |
| [transforms/export.md](transforms/export.md) | map export: preconditions, closure, registration, names, the stage's check |
| [transforms/install.md](transforms/install.md) | install and revert: the stage protocol, byte-for-byte restore, the ledger |
| [transforms/validation.md](transforms/validation.md) | the export's rule registry: every way an exported level fails or is refused, checked live, by the build and by the stage's check; DICE's placement tendencies (L1) as warnings |
| [transforms/projectlog.md](transforms/projectlog.md) | the editor's project log: undo / redo, GUIDs, apply |
| [transforms/terrain_edit.md](transforms/terrain_edit.md) | what a terrain sculpt, paint or new terrain must write, and what the game computes (SPG2, for E5 / E7) |
| [transforms/terrain_gen.md](transforms/terrain_gen.md) | a generated terrain end to end: what a valid terrain is, DICE's biomes and reusable assets, DICE's terrain shapes and playable areas in numbers, the generator's design, the proof plan and rounds TG1 - TG5 (TG0) |
| [transforms/shaders.md](transforms/shaders.md) | BF1943's Xenos shaders translated for Dx10 / 11: scope, code, binding tables, keys, fetch formats, the merge, Dx9 (SPG2) |
| [transforms/dependencies.md](transforms/dependencies.md) | dependency graphs: the asset graph (partition references from string tables, dangling references, cycles, resources by name, closures) and the WW2 rebuild's step graph from OPFOR's step records (SPG3) |

[rules/](rules/) is the index of every rule (one shard per markdown file); [trace.py](trace.py) checks it. [CENSUS.md](CENSUS.md) is everything in DICE's game files (file kinds, resource types, DBX classes, the executable's typeinfo) with the rules that cover each, generated by `probes/census.py` from [census.json](census.json); its `--check` (quick tier) fails when a gated item has no rule. [COVERAGE.md](COVERAGE.md) maps every BFBC2 system to its tools, rounds and rules; `probes/` holds
the read-only scripts that measured DICE's data for the engine files.

## Navigation (Kynapse)

Round NKS's specification of navigation meshing as BFBC2 ships and runs it, in four files:

| File | Layer | What |
|---|---|---|
| [formats/hpd.md](formats/hpd.md) | 1 | the bytes: file, sector table, route graph, runtime-mesh record (`HPD-*`, `KYM-*`) |
| [formats/kynapse.md](formats/kynapse.md) | 1-2 | the meaning: worlds per pathfinding type, sector kinds and bounds, tiles and the resolution field, areas, B / C portals, links, node ids, edges, D / F by type, units and frames; the tracked open questions (`KYN-*`) |
| [engine/kynapse_runtime.md](engine/kynapse_runtime.md) | 3 | what `BFBC2Game.exe` does with it, from reverse engineering (`KYRT-*`); the AI around it is [engine/ai.md](engine/ai.md) |
| [transforms/kynapse_generation.md](transforms/kynapse_generation.md) | 4 | DICE's generator behaviour as rules with corpus counts, NavKit's gap per rule and the gap list in priority order (`KYG-*`) |

The corpus rules are checked by `rust/bc2-formats/tests/kynapse_corpus.rs` (game tier) and measured by
`rust/bc2-formats/examples/kynapse_probe.rs`, which also takes NavKit's builds on its command line for the gap counts.
`rust/bc2-formats/examples/kynapse_fit.rs` holds round NKS-rest's per-feature fits of the generator rules (the midline
cut, node placement, edge pairs and one-way edges, detailed blocks, the rectangle) over the 35 files;
`rust/bc2-formats/examples/kynapse_rules.rs` round NKS-rest2's (node coverage and step pairs, the midline's crossings
and holes, the spanner's unexplained pairs), and `rust/bc2-scene/examples/kynapse_geom.rs` the fits against the
levels' own data (nodes against the collision, detailed sectors against the level data, the rectangle against the
spawns). The rectangle rule's game-tier test is `rust/bc2-scene/tests/kynapse_rect.rs` (it needs the prepared
campaign levels).

## Bot AI

Round AIS's specification of what a bot AI decision-and-control implementation needs (2026-11-22):
[engine/bot_ai.md](engine/bot_ai.md) (`BOT-*`: the runtime, the control boundary, perception, combat, movement,
vehicles, squads, game state, difficulty; its last sections are the implementation interface for OPFOR's bot module
and the reverse engineering still needed, in priority order) and [model/ai_orders.md](model/ai_orders.md) (`AIO-*`:
the order data). DICE's campaign is the ground truth; the data rules are measured by
`rust/bc2-formats/examples/ai_probe.rs` and checked by `rust/bc2-formats/tests/ai_corpus.rs` (game tier). The AI's
start and what a level needs stay in [engine/ai.md](engine/ai.md).

## The plan

### Layer 1 and 2 (round SP1)

Written in this order, the containers and DBX first and deepest: FBRB and resource entries; DBX (binary, types,
references, XML); the data model and registration; terrain, water, collision / Havok; mesh and texture (PC and Xbox
360); shader databases; path data; APT; audio; loc / BIGF / STFS / XISO; cull trees; the rest. What this round left
for later is listed under **Remaining** at the end of this file.

### Layer 3: engine subsystems (round SP2, then later rounds)

One file per subsystem under `engine/`, each rule tied to `BFBC2Game.exe` evidence (`[exe: ...]`), a play test, or
the suite's reproduction and its tests. Every recorded crash address is a rule in its subsystem's file and a row of
`engine/crashes.md`. Written in round SP2:

1. **Loading** (`loading.md`): the session states of a load, the startup bundle, the level checksum (Md5Buffer,
   0x6EFBC0, its faults 0x6EFCA4 / 0x6EFF8A), partitions and references at load, model groups.
2. **Levels** (`levels.md`): ids, mode levels, the level data at run time, the description's declarations.
3. **Gameplay / modes** (`gameplay.md`): mode layers, Conquest, tickets, Rush stages, Squad Deathmatch, spawns, combat
   areas; DICE's measured conventions from the map editor's placement study, each rule naming its L1 warning rules.
4. **AI / navigation** (`ai.md`): the AI's start, the Kynapse runtime, what bots need.
5. **Rendering** (`rendering.md`): the Direct3D 11 pipeline as round R5 reproduced it.
6. **Audio / voice-over** (`audio.md`): waves, streaming and on-demand rules, the announcer per content set.
7. **Networking / hosting** (`network.md`): the in-process host, the transports and the connection layer, the
   server's listen path, the back-end, the server browser; `network_feasibility.md` studies remote play.
8. **Menus** (`menus.md`): the level catalogue, engine-movie events, movie unload.

Round SPG (the gap pass, `COVERAGE.md`) wrote lighting, weather, effects, weapons, vehicles, materials,
destruction and persistence; round SPG2 the rest of its list: terrain writing, vegetation, the shader translation, the
game module's patch points, animation and characters, decals, the rest of persistence, BF1943 as a source, Onslaught,
the effects' processor chain and streaming. What is left is `COVERAGE.md` section 2.

### Layer 4: the suite's transformations (grown by each porting round)

One file per transformation under `transforms/`; each rule states a precondition (what the transformation refuses) or
a postcondition (an invariant between input and output) and cites the test that proves it. Started in round SP2 with
pack / unpack, DBX round trips, cull-tree additions, the console conversion, the WW2 rebuild, NavKit, the stock-map AI
layer, map export, install / revert and the editor's project log; each porting round updates the file of the
transformation it changes, or adds one.

## Remaining

Filled in at the end of each round.

**After round SP1** (2026-11-19: 439 rules in 27 files - 250 verified, 48 implemented, 134 observed, 7 inferred;
158 rules without a test; 341 of the suite's 514 test functions cite no rule yet):

- **Formats not specified.** Resource types the suite copies without reading: `AnimTreeInfo`, `OccluderMesh`,
  `IrradianceVolume`, `BinkMemory`, `WeatherSystem`, `ImpulseResponse`, `RagdollResource`; `SootMesh` (converted
  Xbox 360 -> PC by `bc2-convert`, not yet written down); `database.dbmanifest`; EALayer3 streamed audio. The PS3.
- **Tests to tie in.** The `spec:` comment convention is in place but no test carries one yet; the next step is to
  add them to the tests the rules already cite and to write tests for the 158 rules that have none (the observed
  corpus facts first: most can become `game`-tier checks).
- **The biggest open questions** (each file's own section has the rest):
  1. what the level checksum covers and where it is computed; why a registered-but-absent bundle crashes a new level
     but not DICE's (REG / CSUM, `model/registration.md`);
  2. the executable's class schema (field types per class), so that DBX values stop being typed from data
     (TYPE-SCH-1);
  3. the Havok wrapper's part and material hashes and the material-table run form, needed to add parts and
     materials (`formats/havok.md`);
  4. which header fields the game reads at all: FBRB version, flags, size2 and payload length, the DBX header's
     kept word, the RES wrapper's length words (`formats/fbrb.md`, `formats/dbx.md`, `formats/res.md`);
  5. the height field's main-block tail, needed for terrain of a new size (`formats/terrain.md`);
  6. the path data's unexplained header words, A records and the runtime's tile-size choice (`formats/hpd.md`).
- **Layers 3 and 4** as planned above, layer 3 starting with level loading and the checksum (it closes question 1).

**After round SP2** (2026-11-20: 663 rules in 45 files - 359 verified, 55 implemented, 241 observed, 8 inferred;
224 new: layer 3 138 rules in 8 files - 26 verified, 6 implemented, 105 observed, 1 inferred; layer 4 86 rules in 10
files - 82 verified, 2 implemented, 2 observed; REG-NEW-3 corrected to E3c's behaviour; 272 rules without a test, 115
of them new; 335 of the suite's 556 test functions cite no rule):

- **Question 1, narrowed.** The level checksum that faults on bad `Objects` is Md5Buffer (FUN_006EFBC0), run when a
  level loads (LOAD-CSUM-1). Whether the start-up crash of a wrongly registered level (CSUM-CHK-1, CSUM-CHK-2) is the
  same function run over every registered level, and what it reads of `package.mft` / `bundleManifest`, is still
  open.
- **Layer 3 not written:** physics, streaming, gameplay data (plan above). Menus has four rules; the APT runtime's
  action script is not covered.
- **Layer 3 evidence to strengthen.**
  - Most layer 3 rules are **observed**: the 35 placement conventions of `gameplay.md` (they become tests when round V1
    turns `l1_rules.json` into the editor's checks: each rule already names its L1 IDs); the crash rules (a game-tier
    check per data requirement, as `DbxEdit check` does for LEVEL-OBJ-2, would make them verified).
  - Two crash sites (0x6F2310, a hang; 0x5AC3F0) are known only from the crash reader's symbol table, their data not
    recorded; 0x61084D has no recorded cause and no rule (`engine/crashes.md`).
  - Rendering's per-frame engine values wait for the in-game capture R5 asked for (`engine/rendering.md` Open
    questions); audio's untested points (in-memory voice-over, a streaming flag on in-memory audio, the bank that wins
    for Japanese line names in both banks, server-raised voice-over on the in-process host).
  - Gameplay's open points: capture-zone `Extents` half or full size, `SurroundingGeometry`, whether a spawner's own
    transform is a spawn place, ticket bleed on BF1943's data, aircraft capturing, how PC reads `OnlyTakeableByTeam`
    1 and 2, Onslaught's rules.
- **Layer 4 TODO** (each with the round that next changes it): the shader translation (Xenos -> HLSL, BF1943's
  shaders; the Wake carrier as its oracle); emitter atlases (emitteratlas / effectport); Granny skeleton and animation
  conversion; the menus (`aptgraft`, the HUD patches); the XAS audio encoder as its own transformation (only inside
  `console.md` now); the ledger's adoption of stage-protocol files; NavKit's offline sampling; project versions beyond
  2 (round V1). In-game proofs still open: an exported level loaded in game, the stock-map AI layer played.
- **C++ tests.** OPFOR's DLL rule tests (`src/dll/tests/*_test.cpp`) are `int main()` programs, which `trace.py`
  cannot cite as `[test: ...]`; the network and menu rules cite the DLL code instead.

**After round SPG** (2026-11-22: 933 rules in 54 files - 473 verified, 73 implemented, 357 observed, 30 inferred;
164 new in 8 engine files - 14 verified, 17 implemented, 114 observed, 19 inferred; 421 rules without a test):

- **The coverage matrix** (`COVERAGE.md`): 36 systems; before SPG 15 covered, 13 thin, 8 with no rule; after it 24
  covered, 12 thin, none without rules. Its section 2 is the order of what is left.
- **Closed:** the wrapper's material hash function (MAT-SLOT-1; `formats/havok.md` Open questions).
- **The probes** (`probes/*.py`) measure DICE's data read-only and print counts and ranges; the rules they back are
  **observed**. Turning the stable counts into game-tier checks would make them verified.
- **What only RE or a playtest settles**: the point light chosen per draw and the effects' instance cap (LIGHT-PT-7,
  LIGHT-PT-8), the hit material on plain placed objects (MAT-PEN-3), which weather state the engine starts in
  (RNDR-WTHR-6), irradiance-volume sampling (LIGHT-IRR-3).

**After round SPG2** (2026-11-22: 1,169 rules in 68 files - 521 verified, 115 implemented, 485 observed, 48 inferred;
118 new in 11 files - 22 verified, 42 implemented, 46 observed, 8 inferred; the bot AI round AIS had brought the count
to 1,051 in 59 files before it):

- **New files**: `transforms/terrain_edit.md`, `transforms/shaders.md`, `engine/vegetation.md`, `engine/decals.md`,
  `engine/animation.md`, `engine/module.md`, `engine/onslaught.md`, `engine/streaming.md`, `formats/bf1943.md`;
  `engine/persistence.md` and `engine/effects.md` extended. Two probes (`probes/terrain.py`,
  `probes/effects_chain.py`) and the first game-tier check of probe counts (`core/tests/test_spec_probes.py`, in
  `run_tests.py game`): 14 rules verified by it.
- **Corrected in place** (DICE's 30 terrains contradicted rules written from 3 levels): TERR-VT-3 (the undergrowth
  entry's second field is a string; the suite misreads NAM_MP_003), TERR-VT-4 (sub-terrain maps of 4, 8 or 16; the
  rule-only cells' leftover flag), TERR-VT-7 (flags 2 - 64, a shared tile on 8 terrains), RNDR-TERR-5 (14 of 30 height
  ranges are not 500 m), LIGHT-PT-3 (the game copy's ONS_MP_002 is OPFOR's console port, not Napisal's), SND-STRM-1's
  moved citation.
- **Open, recorded as such**: PERS-SCORE-1 against AI-NEED-5 (settled statically by round AIS-rest, BOT-SCO-3; a
  playtest confirms); the main height-field tail (E7); the material map; the undergrowth numbers; the processor semantics; the mount
  functions.

**After round SPV1** (2026-11-22: 1,269 rules - 749 verified, 127 implemented, 346 observed, 47 inferred; 127 observed
rules verified by game-tier checks, nine of them corrected in place first):

- **The checks**: `rust/bc2-formats/tests/spec_corpus.rs` (DICE's 1,021 bundles in one pass: FBRB, RES, DBX, the
  type system, instances and references, textures, meshes, BIGF) and `core/tests/test_spec_probes.py` (new probes
  `probes/level.py` and `probes/registry.py`; SPG's probes' printed counts for the weapon, vehicle, material,
  destruction, lighting and weather rules). The corpus is DICE's files only (the game copy's bundles dated before
  2013): where a rule's count was the whole game copy's, a `[data: ...]` line now gives DICE's.
- **Corrected** (the data contradicted the wording): FBRB-HDR-10 (the build overlays are stored too), FBRB-DEL-2
  (DICE writes an empty gzip member after a bundle of markers; FbrbTool writes none), TYPE-ATTR-3 (14 namespaces, not
  12), LEVEL-ENV-1 (not one per level: 6 levels have none, 2 have three), WPN-DMG-3 (the SMG automatic row), WPN-AMMO-1
  (pickups), WPN-SWAY-2 (cones; the zoomed minimum), VEH-HP-4 (the chassis states), VEH-MAT-1 (a boat's material).
- **Left**: `COVERAGE.md` section 4 lists every rule still observed with the kind of evidence that would verify it -
  67 more data probes over DICE's files, 23 AI value lists for `ai_corpus.rs`, 24 placement conventions (a re-run
  of the L1 study), 23 half-data / half-RE, 32 on other corpora (Xbox 360, BF1943, OPFOR's builds, research
  scripts), 21 needing the game running, 119 read from the executable, and the 37 Kynapse / NavKit rules (round
  NKG2's files).

**After rounds SPG2-rest3, E4 / E4-rest, PR1b-2-rest2 and NKS-deep part A** (2026-11-23: 1,289 rules in 68 files -
762 verified, 127 implemented, 352 observed, 48 inferred):

- **The bot AI's game state** (SPG2-rest3, `engine/bot_ai.md`): the real ticket and life counters (BOT-GS-4
  corrected), a death costs its team a ticket and a revive gives it back with no AI test (BOT-GS-6, BOT-GS-7), Rush's
  lives, stages and events (BOT-GS-8), the M-COM interaction (BOT-GS-9), the bot object and its senses (BOT-RT-6);
  the mount table's drain (STRM-MNT-3 corrected), dirt only on vehicles (MAT-TEX-20). From the executable: observed
  until a playtest.
- **Gameplay authoring** (E4, E4-rest, `engine/gameplay.md`, `transforms/projectlog.md`): a flag's unit is its own
  (GAME-CQ-11), Rush stage counters count their M-COMs (GAME-RUSH-10), DICE's stage wiring - one base counter per
  stage and a strict enable chain on 30 levels (GAME-RUSH-11), the flag texts' letter scheme with keys A-F in 8
  languages (GAME-CQ-12), DICE's round values per mode and era (GAME-RULES-1).
- **Kynapse's generator settings** (NKS-deep part A, `transforms/kynapse_generation.md` section 9, `formats/hpd.md`,
  `formats/kynapse.md`): read from DICE's settings writer and Kynapse's parameter reader in the executable - the
  graph accuracy plus Kynapse's default radius 0.35 m is the propagation distance and the step-pair gap (KYG-PAR-1,
  verified), DICE's AI/Pathfinding settings mapped to the generator's fields (KYG-PAR-2), the break slope StepMax /
  StepLength = 0.778 (KYG-PAR-3); the HPD header named from the generator's writer (KYN-FILE-2), node ids from 101 per
  sector (KYG-NODE-6), the table times (HPD-TAB-4).
- **Model tools' arithmetic** (PR1b-2-rest2): `decimate` / `shapes` reproduce OpenBLAS's SkylakeX kernels written out,
  so the Rust is deterministic on any CPU and gives the oracle recorded on the development PC (XWW2-MOD-1).
- **Running**: NKG3 (the D sides' slope rule KYG-DF-1, on by the user's ruling with sp_02's dip a named exception),
  SPG3 (settings, input, scripting, dependency graphs) and NET2 (the network layer and a LAN / VLAN / Direct IP /
  local hosting feasibility study).

**After round SPG4** (2026-11-23: 1,420 rules in 85 files - 842 verified, 129 implemented, 391 observed, 58 inferred;
72 new in 13 new files and three extended ones; RES-TYPE-1's table re-pointed):

- **The census** (`CENSUS.md`, `census.json`, `probes/census.py`): DICE's game files as four inventories - 16 kinds
  of file outside the bundles (1,317 files), 37 bundle entry types, 630 DBX instance classes (2,888,042 instances
  with their members), and the executable's 1,900 `typeinfo` records in 22 modules (ENG-TI-1 - 3). Before the round
  6 file kinds, 28 entry types, 304 classes and 19 modules had a rule naming them; after it 13 + 3 out of scope, 37,
  630 and 22. The quick tier's `spec-census` fails on any file kind, entry type, class with 10 or more instances or
  module with no rule and no out-of-scope reason; the game tier re-measures the census and fails when DICE's files
  disagree with `census.json`.
- **Every resource type has a layout**: `OccluderMesh` (OCC), `SootMesh` (SOOT; the PC's 16-byte vertex form the
  console converter refuses), `AnimTreeInfo` (ATREE: the compiler's memory image and its base address),
  `ImpulseResponse` (IR-HDR-1: big-endian `irf1` FFT partitions), `RagdollResource` (HAVOK-RAG-1), `BinkMemory` and
  the out-of-bundle movies (BINK: a RES header with zero lengths), the shader types (SHDB-TYPE-1); `IrradianceVolume`
  and `WeatherSystem` were specified already (LIGHT-IRR-1, WTH-RES-1) and are now linked from RES-TYPE-1. Opaque
  inside, said so: the anim tree's blocks, the occluder's triangle records, the IR partitions' data.
- **The loose files**: the aerial height map (GFILE-AHM-1, read by the flying AI of the one level with
  `HasFlyingAI`), `database.dbmanifest` (DICE's editor database manifest, a copy of the build overlays' entry),
  `dfe` (1,000 opaque bytes nothing names), the patcher's `InstallationData.xml` / `UpdateHistory.log`; DirectX,
  `winui.dll` and `Support` out of scope with reasons.
- **Systems with no file before**: the soldier, squads, the HUD (minimap, markers, spectator, kill cam), the
  campaign (objectives, saves, cutscenes, triggers), text and fonts, sound graphs and voice-over, engine internals,
  and the remaining classes by system (`engine/data_classes.md`). Their rules are the data side - which classes and
  members DICE uses, where, how many - verified by the census; the run-time behaviour behind them is each file's
  open questions.
- **Left (SPG4-rest)**: 1,101 typeinfo records no rule names (`CENSUS.md` section 5, by module: mostly
  `GameSharedResources`' value types and enumerations and `GameDll`'s run-time entities), the semantics behind the
  data-side rules (the controller's states, the squad manager, the kill cam, the sound nodes), and the opaque
  insides listed above.

**After round SPG4-rest** (2026-11-27: 1,571 rules in 90 files; 39 new in four new files and eight extended ones;
ENG-TI-1, HUD-KILL-1, OCC-TRI-1 corrected in place):

- **Every typeinfo record named, with its layout** (`engine/typeinfo/`, `typeinfo.json`, `probes/typeinfo.py`): each
  of the 1,900 records is registered by a static initialiser (RTTI-REG-1) that gives a class its parent and, for a
  data class, its member table; a member is a 24-byte descriptor with its run-time offset (RTTI-MEM-1). The tables
  list every record's parent, size, own members and offsets or values; the quick tier's `spec-typeinfo` keeps them
  current and the game tier re-reads the executable. The census's typeinfo records went from 816 to 1,900 covered.
- **DICE's data against the run-time members** (RTTI-DBX-1, 2): 398 classes the same, 230 with pipeline-only fields
  the game cannot read: the static models' pathfinding switches, the AI's sensing template, the mesh build's
  settings, the sound nodes' names.
- **Which run-time class each data class makes** (RTTI-FAC-1): the factories followed to their constructors, 173
  classes.
- **The run-time behind SPG4's data side**: the controller's nine states, ids and transitions (SOLD-PHYS-2); the
  spawn on a squad mate, its checks and 13 positions (SQD-PLAY-2); the kill cam is the spectate entities gated by
  `Server.KillCam` and hardcore (HUD-KILL-2); the campaign save's `header` and `body` (CAMP-SAVE-1); the sound nodes'
  factories and enumerations (SGR-NODE-2).
- **The opaque insides**: the occluder's per-triangle 192-bit part masks (OCC-TRI-1), the soot's declaration flags and
  part slots (SOOT-FLAG-1, SOOT-SLOT-1), the impulse responses' block-scaled spectra (IR-PART-1).
- **Left (SPG4-rest2)**: `spec/COVERAGE.md` section 2.
