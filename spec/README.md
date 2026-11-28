# The Frostbite 1 / Battlefield: Bad Company 2 specification

A formal description of the Frostbite 1 engine's data, as Battlefield: Bad Company 2 (BFBC2) ships and loads it,
and as the BFBC2 tool suite reads, writes and transforms it. Its purpose is the suite's quality: every reader and
writer is held to a rule written down here, every rule names the code that implements it and the test that checks
it, and what is not known yet is listed as an open question instead of being guessed silently.

The specification is the suite's own description. The format knowledge comes from the suite's readers and writers
(themselves built on the BC2 modding community's research, first of all Frankelstner's), their tests, the rounds
recorded in `rust/BENCHMARKS.md`, and Project OPFOR BFBC2's research notes (since round DOC1 in battlefield-opfor's
`docs/` knowledge base, `bfo:`, and the private research repository, `private:`; section "Reading a rule"). **It
contains no EA game data**: no extracted strings, textures or bytes. It describes structure, counts and observed value
ranges; worked examples use data the suite's tests build.

The suite's other documents (tool manuals, design, release process, audits) are indexed in `docs/README.md`; the
narrative, plans and decisions behind the rules live in battlefield-opfor's knowledge base, which cites rules by ID.

## Scope

| Layer | What | Where | State |
|---|---|---|---|
| 1 | Container and file formats: byte layouts, encodings, platform variants | `formats/` | round SP1 |
| 2 | The data model: DBX types and values, instances, GUIDs and references, partitions and bundles, the level structure, registration, precedence | `model/` | round SP1 |
| 3 | Engine subsystems: what the game does with the data (loading and the checksum, levels, gameplay and modes, AI / Kynapse, rendering, sound, networking and hosting, the menus) and the crashes it gives on bad data | `engine/` | round SP2, extended by rounds SPG - SPG4-rest, NET2, AIS, GM0 - GM4 (`COVERAGE.md`) |
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
  (`opfor:src/dll/probes.cpp`); `bfo:` is the battlefield-opfor checkout (`BFBC2_BFO_ROOT`, else beside the suite),
  whose `docs/` knowledge base holds the documents moved out of Project OPFOR BFBC2 in round KB0 and deleted there
  in round DOC1 (`bfo:docs/research/opfor/MATERIAL_SLOTS_RE.md`; the table of what went where:
  `bfo:docs/research/opfor/MIGRATION.md`). `private:` is the private decompilation repository (`BFBC2_DECOMP_DB`,
  else `battlefield-decomp-work\bfbc2-decomp` beside or above the suite: `bfbc2core.paths.decomp_root`), which
  holds Project OPFOR BFBC2's decompiler-derived research under `opfor-research/` since round DOC1
  (`private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md`). It never has a remote, so public CI and other
  machines lack it: `trace.py --check` then skips its citations and prints how many. Line numbers are those of the commit that last ran `trace.py --emit`;
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

## The decompiled database (round DC0)

A Ghidra database of `BFBC2Game.exe` (PC build 795745) with every function decompiled to pseudo-C lives **outside
every public repo**, in `battlefield-decomp-work\bfbc2-decomp` beside the checkouts (`BFBC2_DECOMP_DB`; the private repository,
which never has a remote; `bfbc2-decomp-private` until round DOC1). It is EA-derived and private. Its
build scripts and the lookup tool are `tools/decomp/` (README there):

```
python tools/decomp/decomp.py 0x5B33C0          the function containing an address, with its pseudo-C
python tools/decomp/decomp.py --rule AI-START-2  the addresses a rule cites, each with its function
```

How rules use it:

- **Cite by address**, as before: `[exe: BFBC2Game.exe 0x5B33C0 ...]`. An address is the fact; the database is how a
  reader checks it. A function's address is its entry; an address inside a function names an instruction's first byte
  (or say "the operand of the instruction at ..."). Ranges are `first - last`, both addresses in the image (file
  offsets say "file offset").
- **Never paste decompiled code, pseudo-C or the strings it shows** into a rule, a test or any committed file. Describe
  what the code does in your own words, with addresses, offsets, constants and names (the facts DC0 may commit).
- A name Ghidra shows (`Session::setState`, a reflected class's `ClientSoldierEntity::...`, `spec_<words>`) may be
  quoted when it helps; the address stays the citation. `spec_` names are words DC0 picked from a rule's sentence, not
  DICE's names.
- Every address the spec cites carries a `[DC0]` comment in the database naming the citing rule ids; the next refresh
  (`run_dc.py plan apply export index`) picks up new rules.

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
| `bfo:docs/research/suite/network_feasibility.md` (moved to battlefield-opfor's knowledge base in round KB2) | study (no rules): what locally hosted play, Direct IP, VLAN and LAN would need, their risks, effort, order and the tests that would prove them |
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
| [engine/gamemodes.md](engine/gamemodes.md) | game modes and eras: GameModeData's readers, the exe's mode-id and game-mod sites, per-era resources, licence checks, authoring a new mode or era (GM0) |
| [engine/eras.md](engine/eras.md) | native eras 2 (Pacific War) and 3 (BFBC1): a decision and patch range for every reader of the game mod, the menus' contract, Vietnam's checks kept (GM1) |
| [engine/eras_n.md](engine/eras_n.md) | N native eras: every limit on the era count, the menu tracker's pages, the launcher pipe, a host era table (GM4) |
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
and a pointer to the reverse engineering still needed, `bfo:docs/tasks/spec-task-notes.md`) and [model/ai_orders.md](model/ai_orders.md) (`AIO-*`:
the order data). DICE's campaign is the ground truth; the data rules are measured by
`rust/bc2-formats/examples/ai_probe.rs` and checked by `rust/bc2-formats/tests/ai_corpus.rs` (game tier). The AI's
start and what a level needs stay in [engine/ai.md](engine/ai.md).

## The plan, and what remained

The layer plan of round SP1 (layers 1 and 2, layer 3's engine subsystems, layer 4's transformations) and the
"Remaining" list each round filled in at its end moved to battlefield-opfor's knowledge base in round KB2 (2026-11-28)
as history: `bfo:docs/research/suite/spec-history.md`, part 1. What is open now is each chapter's "Open questions"
section and the suite's `ROADMAP.md`.
