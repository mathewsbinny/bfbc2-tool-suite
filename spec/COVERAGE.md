# Specification coverage: every BFBC2 system against the tools, the rounds and the rules

Which of BFBC2's systems the specification covers, which tools and rounds touch each one (now or queued in
`ROADMAP.md`, and Project OPFOR BFBC2's WW2 / Onslaught pipeline and game module), and where the gaps are. Written by
round SPG (2026-11-22); keep it current when a round adds a spec file or a system gains a tool. Rule counts are
`rules.json`'s after SPG (`python spec/trace.py --coverage` for the live numbers). Section 4 is round SPV1's
verification census: every file's statuses before and after it, and what would verify each rule still observed.

**Status.** **covered**: the system's data and the engine's use of it are specified to the depth the planned work
needs, most rules observed or better. **thin**: some rules, but the planned work needs more (a format without its
engine side, an engine side without its data, or prose without rules). **none**: no rule.

**Tools named.** Suite crates: `bc2-formats`, `bc2-convert`, `bc2-render`, `bc2-editor` (the map editor),
`bc2-levelwrite` (its export), `bc2-install`, `bc2-cli` (`bfbc2.exe`), `bc2-shader`. Suite Python folders by their
short names (`bf1943-import`: dbxport, havokport, emitteratlas, xenos; `console-convert`, `shader-tools`,
`anim-tools`, `model-tools`, `menu-tools`, `auto-navkit`, `stockmp-ai`, `culltree`, the test harness). OPFOR:
`src/tools/ww2` (the WW2 port: `wake_*`, `bf1943_*`, `ww2_*`), `src/tools/onslaught` (the console port's stages),
`src/dll` (the game module: hooks and patches in `BFBC2Game.exe`), `src/menus`, `src/tools/controller`.

## 1. The matrix

| System | Tools now | Rounds queued / on hold | Spec files (rules) | Before SPG | After SPG | After SPG2 |
|---|---|---|---|---|---|---|
| **Containers**: FBRB, resources, BIGF, STFS, XISO | bc2-formats, fbrb tool, bc2-cli | PR1b-1 (one reader) | formats/fbrb (27), res (8), bigf (3), stfs (4), xiso (2) | covered | covered | as after SPG |
| **DBX** binary / XML / types / references | bc2-formats, dbx tools, editor DBX inspector | PY1, PR1b-1 (one DBX document) | formats/dbx (28), xmlinst (4), model/types (27), references (19) | covered | covered | as after SPG |
| **Bundles, precedence, registration, checksum** | bc2-formats, bc2-install, levelwrite export | IN1, OPFOR in Rust 2 | model/bundles (16), registration (18), engine/loading (13) | covered | covered | as after SPG |
| **Level structure, modes, gameplay** | editor (E3, V1, VW2 mode layer), levelwrite, L1 study | E4, A1 | model/level (31), engine/levels (6), engine/gameplay (45), transforms/validation (59) | covered | covered | as after SPG |
| **Terrain** (height field, patches, masks, destruction layer) | bc2-formats terrain, bc2-render, editor | E5 sculpt and paint, E7 | formats/terrain (19), RNDR-TERR (5), DSTR-TERR (4) | thin (no writer side) | thin: E5 needs the terrain writer rules (TERR open questions) | covered for E5: transforms/terrain_edit (15): what an edit writes, the atlas, flags, donors; SPG2-rest: covered for E7 too - the main block's tail (TERR-HF-4, 9 - 11: depth map, min / max tree, lower envelope) and the material map (TERR-MM-2 - 4) decoded, 30 of 30 re-encoded byte for byte |
| **Water** | bc2-formats, bc2-render, wake_water | - | formats/water (8), RNDR-WATER (3) | covered | covered | as after SPG |
| **Collision / Havok** | bc2-convert havokport, bc2-formats havok | OPFOR in Rust 4 | formats/havok (24), collision (9) | covered | covered (the material hash closed, MAT-SLOT-1) | as after SPG |
| **Meshes, mesh sets** (PC and Xbox 360) | bc2-convert, mesh viewer (editor), console-convert | Dx9 port (hold) | formats/mesh (26) | covered | covered | as after SPG |
| **Textures** | bc2-convert, texqa, texwrite | E5 paint | formats/texture (18) | covered | covered | as after SPG |
| **Shader databases, Xenos** | bc2-formats shaderdb, shader-tools, bc2-shader | OPFOR in Rust 3 (Merger), Dx9 port (hold) | formats/shaderdb (30), RNDR-DB / DRAW (11) | covered | covered | as after SPG |
| **Shader translation** (BF1943 Xenos -> HLSL) | xenos.py, xenos_hlsl.py, pcbuild.py, OPFOR wake_translate | translate weapons and soldiers (hold), Dx9 port (hold) | XENOS (4) in shaderdb; none in transforms/ | thin | thin (plan: transforms/shaders.md, section 3) | covered: transforms/shaders (14) |
| **Rendering pipeline** (draws, order, post, shadows, LOD) | bc2-render (game-accurate view) | RenderDoc round | engine/rendering (45) | covered | covered | as after SPG |
| **Lighting and baked data** (`_Oc`, `_SkyOc`, irradiance volumes, cube maps, outdoor light) | bc2-render weather, levelwrite night | VW2e, E5 (paint layer, re-bake), RenderDoc | RNDR-LIGHT-1 (1) -> **engine/lighting (26)** | thin | covered | as after SPG |
| **Point lights** (ActAsPointLight emitters, `PointLightEntityData`) | editor Lights overlay | VW2c lamp tool, VW2e night conversion | none -> LIGHT-PT (9) | none | covered for VW2c / VW2e; the per-draw choice and the instance cap open (RE) | as after SPG |
| **Weather system** (states, switches, effects) | bc2-render weather, editor weather picker, levelwrite weather (VW2a-c) | VW2d, VW3 weather editor | RNDR-WTHR (9) -> **engine/weather (12)** | thin | covered | as after SPG |
| **Sky, moon, god rays** | bc2-render sky dome | VW2e (moon, god rays), RenderDoc (clouds) | RNDR-SKY (3) + LIGHT-SKY (4) | thin | covered (the panoramic clouds' composite still open, RNDR-SKY-3) | as after SPG |
| **Effects, emitters, emitter atlases** | editor Effects overlay, emitteratlas (suite), OPFOR emitter_atlas, E3d cross-set copy | VW2c, VW2d | none -> **engine/effects (11)** | none | covered for the atlas and placement; the processor chain is thin | covered: the processor chain FX-PROC (4: SPG2-rest2 adds the executable's enums; its semantics open); SPG2-rest4: the evaluator dispatch and the polynomial (FX-PROC-5); SPG2-rest5: the evaluators, inputs and particle record (FX-PROC-6 to 8) |
| **Weapons and projectiles** (firing, damage, sway, zoom, ammo, grenades, attachments) | OPFOR bf1943_weapons, weapons, rifle_grenades, rg_projectiles, scopes, attachments; dbxport | translate weapons (hold), air raid (hold) | none -> **engine/weapons (34)** | none | covered | as after SPG |
| **Vehicles** (entity, physics, seats, health, spawners) | OPFOR bf1943_vehicles, vehicle_input_profile, boatrelease (DLL), console_vehicles / console_markv | repair stations, air raid, Air Superiority (hold) | SND-VEH-1 (1) -> **engine/vehicles (22)** | none | covered | as after SPG |
| **Materials and the material grid** (part / hit material, penetration, impact effects) | OPFOR material_grid, physmat, wake_material_states; havokport | OPFOR in Rust 4 | HAVOK-MAT (2), GAME-DMG-1 -> **engine/materials (30)** | none | covered; the hit material of plain objects open (playtest) | as after SPG |
| **Destruction** (health states, parts, bangers, destroyed meshes, collapse) | bc2-render destruction states, editor (SG1 members), levelwrite members | E4 (M-COMs), the map editor's group edits | HAVOK-RUN-3, GAME-DMG-1 -> **engine/destruction (22)** | none | covered | as after SPG |
| **Grime, dirt, camouflage** | OPFOR grime, grime_models, grime_sheet | translate soldiers (hold) | none -> MAT-TEX (4) | none | thin: MAT-TEX-3 inferred; the grime pipeline's own rules not written | SPG2-rest: covered (data side): MAT-TEX (19): dirt slots and constants, terrain dirt triggers, vehicle / soldier / weapon camouflage per level, OPFOR's weathering pass; SPG2-rest2: the run-time build-up (MAT-TEX-20) |
| **Vegetation and undergrowth** (TreeMeshSet, undergrowth, billboards) | mesh readers, bc2-render, conversion (TreeMeshSet) | **E6 vegetation** | MSET-PART-2, RNDR-TERR-4 (2) | thin | thin (plan, section 2) | covered: engine/vegetation (8); SPG2-rest: the undergrowth entry is the engine's TerrainUndergrowthType (VEG-UG-3, 5), the NAM_MP_003 reader fix; SPG2-rest2: bc2-convert's reader fixed too, the loader not found (VEG-UG-6); SPG2-rest4: the loader and the generator read, the 12 numbers verified (VEG-UG-5 to 7); SPG2-rest5: the render settings and the wind (VEG-UG-8, 9) |
| **Animation and characters** (Granny, anim trees, aim, skeletons, soldiers, kits) | granny, aimanim, anim-tools, OPFOR anims / animswap / soldiers / kits / kitbags / skinning | translate soldiers (hold) | formats/granny (8), aimanim (3) | thin | thin (plan, section 2) | covered: engine/animation (17); SPG2-rest4: the compiled tree's three blocks (ATREE-BLK-1 to 3); SPG2-rest5: the 26 node types named, blocks B and C per animation node (ATREE-NODE-1, ATREE-BLK-4) |
| **Decals and roads** | bc2-render (baked strips) | E5 | RNDR-TERR-4 (1) | thin | thin (plan, section 2) | covered: engine/decals (6) |
| **Audio and voice-over** | console-convert XAS, OPFOR wake_vo / wake_ambience | E3c done | formats/audio (16), engine/audio (16) | covered | covered | as after SPG |
| **AI and navigation** (Kynapse) | auto-navkit, stockmp-ai, editor AI workspace, DLL navgen / navquery / forceai | **NKS (now)**, NK3-NK5, A1, WW2 navmesh (hold) | formats/hpd (36), navsample (8), engine/ai (16), engine/bot_ai (41, round AIS), model/ai_orders (16, AIS), transforms/navkit (8), stockmp (7) | covered (generator rules in NKS) | covered; NKS owns the rest | as after SPG |
| **Cull trees** | culltree, bc2-formats | PR1b-1 | formats/culltree (17), transforms/culltree (8) | covered | covered | as after SPG |
| **Menus, HUD, APT** | menu-tools, aptgraft, OPFOR menus, DLL menutree / onslaught_hud | IN1 diagnostics | formats/apt (31), engine/menus (4) | thin (runtime) | thin (the action-script runtime and the HUD patches) | SPG2-rest: covered (data side): engine/menus (30): the movies' action script measured, the interpreter's handler table, events and externs, the HUD movies, OPFOR's DLL patches, what a menu edit must keep; SPG2-rest4: the FSCommand callback and the event queue's drain (MENU-RT-6, MENU-EVT-4); SPG2-rest5: the loading and HUD roots, the screens' FSCommand handlers, no event delay (MENU-LOAD-2, MENU-RT-7, MENU-EVT-5) |
| **Networking and hosting** | DLL net / server_browser / patches | NET2 (2026-11-23); remote play studied, not queued | engine/network (9) | covered | covered | as after SPG; NET2: covered statically, engine/network (25): the transports, the connection layer, settings and start-up options, versions and disconnect reasons, PunkBuster, the server's listen path, the join by address, the Plasma back-end and its LAN mode; remote play in engine/network_feasibility (study); thin at run time (nothing NET2 adds was run) |
| **Persistence** (stats, ranks, unlocks, licences, awards) | DLL persistence, stats_screens; OPFOR ww2_awards | Pacific War awards (decided 2026-11-15) | GAME-AWARD-1 (1) -> **engine/persistence (7)** | none | thin: the game's path and OPFOR's hooks; the award engine and `Persistence/*.dbx` classes not written | covered: PERS-DATA / RANK / AWD / SCORE / WW2 (12 more); the award engine not built; SPG2-rest4: the award listener and grant (PERS-AWD-5), why the career save drops kills (PERS-SCORE-2); SPG2-rest5: criteria resets by type, the award stats are Final templates (PERS-AWD-6, 7) |
| **OPFOR's game module: patch points** (27 files, about 170 addresses in `BFBC2Game.exe`) | `src/dll` | air raid, repair stations (hold) | network, persistence, AI-NEED-5, GAME-AWARD-1, crashes index | thin | thin (plan, section 2) | covered: engine/module (15): 221 addresses |
| **Streaming** (`async`, `streaming_*`, `ondemand_*`) | bc2-formats | - | SND-WAVE-2, SND-OD-1 | thin | thin | covered: engine/streaming (7); the mount functions not traced |
| **BF1943 as a source** (bundles, Havok 4.6.1, DBX classes, Xenos) | bf1943-import, OPFOR ww2 | Pacific War (hold) | HAVOK-V461 (2), XWW2 (9), WPN-WW2, VEH-WW2, MAT-WW2, LIGHT-SKY-4 | thin | thin (a source-side file still missing, section 2) | covered: formats/bf1943 (7, an index of the rest) |
| **Xbox 360 / Onslaught DLC data** | console-convert, console-readers, OPFOR onslaught (14 stages) | Onslaught without Napisal (hold), OPFOR in Rust 5 | transforms/console (8), TEX-X / MESH-CONV / HAVOK-X360 / GRANNY-CONV, LIGHT-PT-3 / 4 | thin | thin (Onslaught's mode rules and the DLC's content list) | covered: engine/onslaught (14); the DLC's full content list per port open |
| **Install, export, project log** | bc2-install, levelwrite, editor | IN1 | transforms/install (20), export (12), projectlog (20), pack (9), dbx (8), ww2 (9) | covered | covered | as after SPG |
| **Options and settings** (`settings.ini`, `GameSettings.ini` / `.bin` and their defaults, the engine settings objects, the command line, the options screens) | OPFOR's DLL (the user-data redirect, `stats_screens`'s getters) | - | engine/settings (11) | none | none | none; **SPG3: covered** (the files, sections and keys, the defaults partition, the read / write paths, the menus' getters and commands); thin on `GameSettings.bin` and on what a bad value does (open) |
| **Input** (entry input actions, concepts, device bindings, mappings, the user's binding files, the controls screen, the pad path, tuning data) | OPFOR's DLL (`input`, `prompts`, `padaimer`), `src/tools/controller` | controller presets, rumble, PlayStation pads (OPFOR's CONTROLLER_PLAN) | engine/input (12), BOT-CTL-3, MOD-IN-1, VEH-SEAT-2 | none | thin (MOD-IN-1 only) | thin (BOT-CTL-3, MOD-IN-1); **SPG3: covered** (both enumerations, the three data layers, the user files' load / save and version gate, the controls screen, the PC pad path); the sensitivity sliders' consumers and the schemes open |
| **Scriptability** (the authoring layers, the level event graph's classes, data entities that reach the engine, consoles, server administration) | the data (editor, levelwrite), OPFOR's menu patches and DLL hooks | a modder-facing scripting layer (none queued) | engine/scripting (9), LEVEL-EVT (model/level), MENU-*, MOD-* | none | none | none; **SPG3: covered as a survey**: the 107 entity classes DICE's graphs never use and the console / settings entities' handlers open; the dedicated server is not on this PC |
| **Dependency graphs** (the asset graph; the WW2 rebuild's step graph) | levelwrite's closure (XEXP-CLO-1), OPFOR's `rebuild_all` / `stepstamps` | OPFOR in Rust: `rebuild_all` as a step graph | transforms/dependencies (11) | none | none | none; **SPG3: covered**: the asset graph measured over DICE's bundles (verified), the step graph from the records (observed) and a runner's constraints; resources' internal name references open |
| **Game folder files outside the bundles** (movies, the aerial height map, `database.dbmanifest`, `dfe`, the patcher's files, the DLLs) | none (copied) | - | formats/game_folder (6), bink (4) | none | none | none; **SPG4: covered**: every DICE file kind specified or out of scope (DirectX, `winui.dll`, `Support`), the census its gate |
| **Remaining resource types** (OccluderMesh, SootMesh, AnimTreeInfo, ImpulseResponse, RagdollResource, the shader types) | copied; `soot_mesh` converts | - | formats/occluder (3), soot (3), animtree (2), IR-HDR-1, HAVOK-RAG-1, SHDB-TYPE-1 | none | none | none; **SPG4: covered** (header and structure, verified on every DICE entry; opaque insides listed) |
| **The soldier** (character controller, stances, sprint, cameras, view recoil, FOV, aim assistance, spotting) | OPFOR's controller patch, kits | controller presets (OPFOR) | engine/soldier (9) | none | none | none; **SPG4: covered (data side)**; the controller's run-time open |
| **Teams and squads** | - | - | engine/squads (3), BOT-SQD | none | none | none; **SPG4: covered (data side)**; the squad manager's code open |
| **HUD data** (minimap, markers, nametags, screen effects, spectator, kill cam) | menu-tools, OPFOR's HUD patches | - | engine/hud (7), MENU-HUD | thin | thin | thin; **SPG4: covered (data side)** |
| **Single-player campaign** (objectives, saves, cutscenes, triggers, waypoints) | editor (event graph) | - | engine/campaign (6) | none | none | none; **SPG4: covered (data side)**; the save file open |
| **Text and fonts** (fonts, text databases, atlases, subtitles) | bc2loc (loc tables) | - | engine/text (4), formats/loc (7) | thin | thin | thin; **SPG4: covered** |
| **Sound graphs, voice-over trees, the mix** | console-convert (Waves), OPFOR wake_vo | - | engine/sound_graph (5), engine/audio | thin | thin | thin; **SPG4: covered (data side)**; node semantics open |
| **Engine internals** (typeinfo, memory, jobs, scenes) | the census | - | engine/internals (7) | none | none | none; **SPG4: covered**: the typeinfo records decoded (1,900, 22 modules); 1,101 records no rule names yet (CENSUS.md section 5) |
| **Every DBX class** (the long tail: emitter updates, tree parameters, vehicle physics parts, gadgets, AI constants, matchmaking, edge models) | the DBX readers | - | engine/data_classes (10) | - | - | **SPG4: 630 of 630 classes named by a rule** (304 before) |

**Totals** (36 systems). Before SPG: 769 rules in 46 files; covered 15, thin 13, none 8 (point lights, effects,
weapons, vehicles, materials, destruction, grime / camo, persistence - the last with one crash rule). After SPG: 933
rules in 54 files (164 new: 14 verified, 17 implemented, 114 observed, 19 inferred); covered 24, thin 12, none 0.

After SPG and AIS (round AIS added the bot AI's 57 rules): 1,051 rules in 59 files. After SPG2 (2026-11-22): 1,169
rules in 68 files (118 new: 22 verified, 42 implemented, 46 observed, 8 inferred; DSTR-TERR-4 observed -> verified by
the new game-tier check); covered 34, thin 2 (grime / camouflage, the menus' runtime), none 0.

After SPG2-rest (2026-11-22): 1,231 rules in 68 files (from 1,183 at its start, NKS-rest included: 48 new; 531 -> 605
verified, 116 -> 126 implemented, 487 -> 453 observed, 49 -> 47 inferred); covered 36, thin 0, none 0. The game-tier
check (`core/tests/test_spec_probes.py`) now holds every probe's stable counts: 172 checks, about 3.5 minutes.

After SPG3 (2026-11-23): 1,332 rules in 72 files with main's of the same day (43 new in four new files: 17 verified, 2 implemented, 16 observed, 8
inferred); 40 systems (four new: options and settings, input, scriptability, dependency graphs), all covered. The
game-tier check gained three probes' `checks()` (`options`, `scripting`, `dependencies`: about 40 seconds more).

After SPG4 (2026-11-23): 1,420 rules in 85 files (72 new: 13 new files, three extended); 50 systems (ten new rows: the
game folder's files, the remaining resource types, the soldier, squads, HUD data, the campaign, text, sound graphs,
internals, every DBX class). The game tier gained `resources.py`'s and `census.py`'s checks (about 3 minutes more).

## 2. What is left, in the order the queued rounds need it

Round SPG2-rest5 (2026-11-27, static only) worked SPG2-rest4's list below. Found:
- the render settings that scale the undergrowth, traced from `VisualTerrainSettings` to their use (density,
  view distance and its fade, the cell grid, the capacity: VEG-UG-8); `WindScale` is the third value of the
  undergrowth shader's `windParams` with the clock and `UndergrowthWindSpeed`; the second flag byte has no reader
  (VEG-UG-9);
- every particle evaluator but mesh and sample texture (FX-PROC-6: the polynomial operator's operands run from the
  linear term up, the reverse of the polynomial's); the inputs (FX-PROC-7: only `EfNormTime` is used, DICE's 40
  `EfVelocity` processors are turbulences without an evaluator, `EmitterParameter` picks one of the particle's four
  parameters); the particle record and what the spawn and force processors write (FX-PROC-8);
- the loading screen's root is the level's game mode's `LoaderName`, the HUD's its `HudRootName` (MENU-LOAD-2); the
  screens' FSCommand handlers and the shared one, 4 of 133 commands answered by nothing (MENU-RT-7); no code sets an
  event's delay (MENU-EVT-5);
- criteria resets by `CriteriaType` (PERS-AWD-6); every award code's `<Code>_00` / `_01` is a `Final` value template
  (PERS-AWD-7);
- the 26 anim-tree node types by the executable's own names (ATREE-NODE-1); blocks B and C hold one entry per
  animation node, B's first float its length, the node's +0x20 its index (ATREE-BLK-4).

Rules went from 1,506 to 1,518. Left (queued as SPG2-rest6):

1. **Undergrowth**: the shaders' use of `windParams`; `UndergrowthMaxHeight`'s use.
2. **Effects**: the mesh and sample-texture evaluators; the colour, transparency, alpha, texture-coordinate and
   clip-scale processors (the vertex fill 0xB27BF0 / 0xB280C0); the mesh particles' update 0xB328C0; the force
   vectors' set-up.
3. **Menus**: what each FSCommand handler case does.
4. **Persistence**: who makes the criterion objects; the event 0x51; OPFOR's PERS-SCORE-2 fix (an OPFOR round).
5. **Anim trees**: the node records' fields; block C's words beside its pointers.
6. **Playtests** (paused): as before, plus a save with an award (PERS-AWD-7) and a burst of one menu event
   (MENU-EVT-5).

The list below is SPG2-rest4's, as it was.

Round SPG2-rest4 (2026-11-23, static only) worked SPG2-rest3's items 1 and 3 below (the list after it is kept as it
was). Found:
- the undergrowth entry's loader: the factory 0xA2F440 and the loader 0xA2BA20, not the type's registration 0x1375A20,
  which has no factory. The 12 numbers are named by the code that uses them and verified against DICE's data
  (VEG-UG-5); the generator's formulas are VEG-UG-7;
- the particle evaluators' dispatch and the polynomial: c.x t^3 + c.y t^2 + c.z t + c.w, clamped, then scaled
  (FX-PROC-5);
- the FSCommand callback's ten own commands, checked by the menus probe (MENU-RT-6), and the event queue's drain:
  once a frame, with delayed events deduplicated (MENU-EVT-4; MENU-EVT-2's "target movie" corrected);
- the award listener and where a finished award is written: `<Code>_00` / `_01` stats (PERS-AWD-5);
- why the career save has no kills: the scoring record's stats pointer, which only EA's answer sets (PERS-SCORE-2);
  the fix is OPFOR's;
- the compiled anim tree's three blocks from the loader's relocation walkers, verified on all 1,926 trees
  (ATREE-BLK-1 to 3).

Rules went from 1,468 to 1,477. Left (queued as SPG2-rest5):

1. **Undergrowth**: `WindScale` and the second flag byte (no reader in the terrain module); which render settings
   scale density and view distance.
2. **Effects**: the other evaluators (box, random, sphere, rotate vector, colour interpolation), the polynomial
   operator, what each processor does with its value, the `EfVelocity` path.
3. **Menus**: the loading screens' root names (the screen classes' root-name slot; the main menu's is 0x753660);
   who sets an event's delay; the screens' FSCommand handlers (vtable +0x30).
4. **Persistence**: OPFOR's fix for PERS-SCORE-2 (attach the filled stats with 0x866240) and its playtest; the
   criteria classes; whether `<Code>_00` names are in DICE's `Final` templates.
5. **Anim trees**: the 26 node types' meanings and fields; block B's words and block C's records beyond their
   pointers.
6. **Playtests** (paused): as SPG2-rest3's item 4, plus a save with kills after the PERS-SCORE-2 fix.

Round SPG4 (2026-11-23) made the spec's coverage measurable: `spec/CENSUS.md` (generated by `spec/probes/census.py`)
lists everything in DICE's game files - 16 file kinds outside the bundles, 37 bundle entry types, 630 DBX classes,
1,900 typeinfo records in 22 modules - with the rules that cover each, and the quick tier's `spec-census` fails when
a file kind, entry type, class (10+ instances) or module has none. Before: 6 / 16 file kinds, 28 / 37 entry types,
304 / 630 classes, 19 / 22 modules; after: 13 + 3 out of scope, 37, 630, 22 (the ten rows above the totals). Left for
SPG4-rest, by value: the 1,101 typeinfo records no rule names (CENSUS.md section 5: `GameSharedResources`' value types
and enumerations, `GameDll`'s run-time entities), then the run-time semantics behind the new data-side rules (each new
file's open questions: the character controller, the squad manager, the kill cam, the sound nodes, the save file), then
the opaque insides of OccluderMesh's triangle records, AnimTreeInfo's blocks and the impulse responses' partitions.


Round NET2 (2026-11-23, static only) extended `engine/network.md` from 9 to 25 rules (23 observed, 2 inferred) and
wrote `engine/network_feasibility.md`, a study of locally hosted play, Direct IP, VLAN and LAN with no rules of its
own. Left for networking, in its order: one diagnostic run with the socket transport to settle why the in-process
server never listened, its port defaults and `Network.Version` (network.md's open questions); then, if the user takes
remote play up, the listen server and Direct IP as one milestone (the study's sections 4 and 5). Statically still
open: transport 2's path, the server's compare sites for the version, title and level digest, and whether Plasma's
GameManager is ever created on PC.
Round SPG3 (2026-11-23) added the four systems above (section 1's last rows); what it left is in the four files' open
questions and here:

1. **Settings**: `GameSettings.bin`'s layout; the parsers' behaviour on a bad value; whether `-<Object>.<Field>` sets
   the engine settings (SET-CLI-1); which slider writes `Sensitivity0` - `Sensitivity2` (INP-SET-1). The first two
   need a test start with copies of the files (paused).
2. **Scripting**: the handlers of `GameSettingEntityData` and `ConsoleCommandEntityData` and the events of the 107
   entity classes DICE's graphs do not use (SCR-EVT-2, SCR-CMD-1); whether the debug document server starts.
3. **Step graph**: the read-before-write steps are fixed (XDEP-STEP-3, round WW2-ORD1), the inputs no step writes
   are declared and the oracle regenerated (XDEP-STEP-5 / -6, round WW2-ORC1), and the rebuild runs as a step graph
   (XDEP-STEP-8 - 10, round OR7). Left: the install plan's porting as a rebuild step (the rebuild -> plan ->
   rebuild cycle, XDEP-STEP-5: proposed); the reads the hook cannot see, which are declared by hand (`READS`); the
   stale records of renamed steps.
4. **Asset graph**: references held inside resources (mesh sets' shader names, shader databases' texture names, the
   VO tables' waves), which the string-table listing does not see (XDEP-RES-1).

The rest of this section is SPG2-rest3's list, as it was.

Round SPG2-rest3 (2026-11-22, with AIS-rest2's static half) worked section 2c's item 3 and bot_ai.md's RE list; section
2c keeps SPG2-rest2's list as it was. Found: the ticket counter's and the life counter's state and what moves them
(BOT-GS-6, BOT-GS-8: a bot's death costs a ticket or a life; the Rush stage is the defenders' bases left), the death
and revive messages (BOT-GS-7), the mode classes corrected (BOT-GS-4), the bot object and its senses (BOT-RT-6), who
drains the mount table (STRM-MNT-3, its record's fields named and corrected), which entities run the dirt build-up
(MAT-TEX-20: client vehicles only). Left:

1. **Terrain, in game** (as 2c item 1) and **the undergrowth entry's parser**: not in `VisualTerrain`'s 19 slots
   (VEG-UG-6); next the resource factory registered with the type at 0x1375A20. VEG-UG-5 stays inferred.
2. **Bot AI, static**: the combat fire site (bot_ai.md RE item 8: narrowed to `BehaviourFire`'s action object at
   +0x10), the M-COM's armed state (BOT-GS-9, item 3), the bot's unnamed parts (item 4).
3. **RE not reached this round**: the particle update (FX-PROC-4: 0x11DD76A is the C runtime's `toupper`; the other
   switches of 36+ cases are not all read), the award listener classes and where a finished award is written
   (PERS-AWD-4), why the career save drops the round's kills (PERS-SCORE-1), the Flash loading roots, the menus' event
   queue and FSCommand callback (MENU-LOAD-1), the compiled anim tree's layout (ANIM-TREE-3).
4. **Playtests**: bot_ai.md's plan grew steps 8 - 10 (tickets, Rush lives and stages, the fire behaviour's action
   object); the rest as 2c item 4.

## 2c. What was left after SPG2-rest2 (kept as written)

Round SPG2-rest2 (2026-11-22) did section 2b's items 3 (in part) and 4 and wrote item 1's playtest plan; section 2b keeps
SPG2-rest's list as it was. Left (queued as SPG2-rest3):

1. **Terrain, in game**: the plan is written (`transforms/terrain_edit.md` section 5: depth map, stale and synthetic
   lower envelope and min / max tree, material sampling, collision, a new level); it waits on in-game tests. The
   suite's height-field readers still split the tail the old way (`Tail::Sections`): E7 reads the four parts.
2. **The undergrowth loader**: not found (VEG-UG-6: the type's offsets lead nowhere, the run-time keeps its own
   layout); next the `VisualTerrain` class's 19 slots (vtable 0x144FCF0) for the entry's parser. VEG-UG-5 stays
   inferred.
3. **RE still needed**: the particle update (FX-PROC-4 has the enums and fields, not the formulas); who drains the
   mount table (STRM-MNT-3); the award listener classes and where a finished award is written (PERS-AWD-4), why the
   career save drops the round's kills (PERS-SCORE-1); which entities run the dirt build-up (MAT-TEX-20); the Flash
   loading roots (MENU-LOAD-1 rules out the level description), the event queue and FSCommand callback; the compiled
   anim tree's layout (ANIM-TREE-3).
4. **Playtests found by this round's reading**: ONS-CAP-3's patch for "defenders cannot retake"; bots alone on a flag
   (BOT-GS-5); a RenderDoc capture of a vehicle before and after Mud (MAT-TEX-20); the kill probe (PERS-SCORE-1).

## 2b. What was left after SPG2-rest (kept as written)

Round SPG2-rest (2026-11-22) did section 2a's items 1 - 2, 4 and 5 and part of 3 (the undergrowth fields); section 2a
keeps SPG2's list as it was. Left:

1. **Terrain, in game**: E5's and E7's rules are written down to the bytes, but no edited terrain has been loaded:
   what the engine does with the depth map's values (TERR-HF-9) and a stale or synthetic lower envelope (TERR-HF-11),
   and how it samples the material map (TEDIT-MM-1), wait on a playtest; what reads the height field for collision.
   The suite's height-field readers still split the tail the old way (`Tail::Sections`): the round that writes main
   blocks (E7) reads the four parts.
2. **The undergrowth loader** (VEG-UG-5 is matched by data): the order inside the two-float scales and the two closing
   flags.
3. **RE still needed**: the emitter processors' semantics (FX-PROC-2), the mount functions (STRM-MNT-1), the
   `OnlyTakeableByTeam` reader (ONS-CAP-1), the award evaluator and whether bots' deaths are scored (PERS-SCORE-1
   against AI-NEED-5: a kill-probe playtest), the compiled anim tree's layout (ANIM-TREE-3); run-time dirt build-up
   (MAT-TEX-6, MAT-TEX-7: a capture); the menus' loading roots, event queue and FSCommand callback (engine/menus.md
   Open questions).
4. **Readers that keep the one-byte undergrowth reading**: `bc2-convert`'s visual-terrain walk and console-convert's
   `terrain.py` (Xbox and BF1943 inputs, whose second strings are all empty); a fix with the round that next touches
   them.
5. **Probes still printing only**: none of the probes' rules is observed for want of a check any more; the rules still
   observed rest on counts no check holds (per-class ranges in the weapon and vehicle outputs) or on the executable.

## 2a. What was left after SPG2 (kept as written)

1. **Terrain for E7**: the main block's tail (TERR-HF-4) and the material map's encoding (TEDIT-MM-1); what reads the
   height field for collision (TEDIT open questions). E5's sculpt and paint rules are in place; every one waits on a
   playtest of an edited terrain.
2. **The suite's undergrowth reader** misreads NAM_MP_003's visual terrain (VEG-UG-2): a reader fix for the round that
   next touches `bc2-render/src/terrain.rs` (QW-sized).
3. **RE still needed**: the undergrowth numbers' meanings (VEG-UG-3), the emitter processors' semantics (FX-PROC-2),
   the mount functions (STRM-MNT-1), the `OnlyTakeableByTeam` reader (ONS-CAP-1), the award evaluator (whether bots'
   deaths are scored was settled statically by AIS-rest, BOT-SCO-3; a kill-probe playtest confirms), the compiled anim tree's layout
   (ANIM-TREE-3).
4. **Grime / camouflage** (MAT-TEX) and **the menus' action-script runtime** stay thin.
5. **More probe counts as checks**: SPG's five probes (lighting, weapons, vehicles, materials, destruction) still back
   observed rules only; their stable counts can join `test_spec_probes.py` the same way.

## 3. What was left after SPG (kept as written)

1. **Terrain writer rules** (for E5, E7): the height field's main-block tail (TERR open questions), patch-map and mask
   atlas writing, the destruction layer when terrain is sculpted; the paint layer's ratio rule is LIGHT-BAKE-6.
2. **Vegetation** (`engine/vegetation.md`, for E6): `TreeMeshSet` levels and billboards (beyond MSET-PART-2), the
   undergrowth system (`MiddlegrowthModelEntityData`, the undergrowth cache, the terrain undergrowth masks), tree
   bangers (DSTR), DICE's densities per level measured over the stock levels.
3. **The shader translation** (`transforms/shaders.md`, for the Dx9 port and the weapon / soldier translation): the
   binding table's ids, the program key's mesh kind and layout GUID rule, the fetch-format match, the D3D9
   multiply rule, alpha test, the Wake carrier as its oracle (all in OPFOR's notes as prose today).
4. **The game module's patch points** (`engine/module.md`): one row per hook or patch (address, original bytes, what
   it changes, the rule it serves), from `src/dll`'s constants; the crashes index gets the rows it lacks.
5. **Animation and characters** (`engine/animation.md`): compiled anim trees (`AnimTreeInfo` and its baked hand IK),
   skeletons and kinds, AimAnimation use, soldiers and kits as DBX classes (OPFOR `research/WW2_KITS.md`).
6. **Decals and roads**: the baked road strips and decal entries (RNDR-TERR-4), impact decals per material (MAT-FX).
7. **Persistence, the rest**: `Persistence/*.dbx` (unlock tree, ranks, awards) as classes, the stats keys, then
   BF1943's award rules for the Pacific War career.
8. **BF1943 as a source** (`formats/bf1943.md`): its bundles and levels, the class differences DICE resolved, what
   DICE already ships of BF1943 in BFBC2 (`objects/weapons/handheld/beach/`, the 8 campaign vehicles).
9. **Xbox Onslaught** (`engine/onslaught.md`): the mode's rules (waves, checkpoints, the four levels), the DLC's
   content list against the PC Package port (WX1: the port lacks the night lamps, moon and night ambience).
10. **Effects' processor chain** (FX-CLS-1): the `Emitter.*Data` classes, if the editor previews particles.
11. **Streaming**: the mount order and precedence of `async`, `streaming_*` and `ondemand_*` bundles.

## 4. The verification census (round SPV1, 2026-11-22)

Round SPV1 checked the **observed** rules (seen in DICE's data or code, no test) against DICE's files and promoted to
**verified** every one whose claims a game-tier check now counts: 127 of 473. The checks: the Rust corpus test
`rust/bc2-formats/tests/spec_corpus.rs` (one pass over DICE's 1,021 bundles - every `.fbrb` of the game copy dated
before 2013 - for the container, resource, DBX, type, instance and reference, texture, mesh and BIGF rules; about 40 s
in release) and `core/tests/test_spec_probes.py` (two new probes, `probes/level.py` for the level partitions, cull
trees, vegetation and terrain splines and `probes/registry.py` for the stock `package.mft` / `bundleManifest`; SPG's
five print-only probes' output lines for the weapon, vehicle, material, destruction, lighting and weather rules). A
rule is verified only when every number it states is checked; nine rules whose numbers the data contradicted were
corrected in place (the wording, each saying so) and then verified: FBRB-HDR-10, FBRB-DEL-2, TYPE-ATTR-3, LEVEL-ENV-1,
WPN-DMG-3, WPN-AMMO-1, WPN-SWAY-2, VEH-HP-4, VEH-MAT-1, and the DICE-only counts that differ from the game copy's
(DBX-STR-3, MSET-LOD-2, VEG-CLS-1, the corpus rules) are new `[data: ...]` lines beside the old ones.

### 4a. Status per file, before -> after

Over the 1,269 rules SPV1 started from (round E4's five new rules, merged beside it, are verified and not counted).

| File | Verified | Implemented | Observed | Inferred |
|---|---|---|---|---|
| `engine/ai.md` | 0 | 0 | 15 | 2 |
| `engine/animation.md` | 3 | 5 | 8 | 1 |
| `engine/audio.md` | 3 | 0 | 13 | 0 |
| `engine/bot_ai.md` | 2 | 0 | 44 | 7 |
| `engine/decals.md` | 1 -> 2 | 1 | 3 -> 2 | 1 |
| `engine/destruction.md` | 14 -> 18 | 1 | 5 -> 1 | 2 |
| `engine/effects.md` | 8 -> 10 | 1 | 4 -> 2 | 2 |
| `engine/gameplay.md` | 9 | 1 | 39 | 0 |
| `engine/kynapse_runtime.md` | 0 | 0 | 17 | 1 |
| `engine/levels.md` | 0 | 0 | 6 | 0 |
| `engine/lighting.md` | 7 -> 8 | 2 | 13 -> 12 | 4 |
| `engine/loading.md` | 1 | 1 | 11 | 0 |
| `engine/materials.md` | 21 -> 25 | 8 | 11 -> 7 | 6 |
| `engine/menus.md` | 20 | 7 | 4 | 0 |
| `engine/module.md` | 0 | 14 | 1 | 0 |
| `engine/network.md` | 0 | 0 | 9 (NET2: 23) | 0 (NET2: 2) |
| `engine/onslaught.md` | 0 | 7 | 8 | 0 |
| `engine/persistence.md` | 3 | 3 | 14 | 0 |
| `engine/rendering.md` | 25 | 4 | 14 | 2 |
| `engine/streaming.md` | 0 -> 1 | 1 | 7 -> 6 | 0 |
| `engine/vegetation.md` | 3 -> 4 | 1 | 5 -> 4 | 1 |
| `engine/vehicles.md` | 5 -> 16 | 4 | 12 -> 1 | 1 |
| `engine/weapons.md` | 7 -> 30 | 2 | 23 -> 0 | 2 |
| `engine/weather.md` | 3 -> 6 | 0 | 8 -> 5 | 1 |
| `formats/aimanim.md` | 3 | 0 | 0 | 0 |
| `formats/apt.md` | 22 | 4 | 4 | 1 |
| `formats/audio.md` | 14 | 1 | 2 | 0 |
| `formats/bf1943.md` | 0 | 1 | 6 | 0 |
| `formats/bigf.md` | 2 -> 3 | 0 | 1 -> 0 | 0 |
| `formats/collision.md` | 4 | 1 | 2 | 2 |
| `formats/culltree.md` | 9 -> 12 | 1 | 7 -> 4 | 0 |
| `formats/dbx.md` | 20 -> 27 | 0 | 7 -> 0 | 1 |
| `formats/fbrb.md` | 16 -> 25 | 0 | 11 -> 2 | 0 |
| `formats/granny.md` | 6 | 2 | 0 | 0 |
| `formats/havok.md` | 17 -> 18 | 2 | 5 -> 4 | 0 |
| `formats/hpd.md` | 22 | 0 | 14 | 1 |
| `formats/kynapse.md` | 11 | 0 | 10 | 0 |
| `formats/loc.md` | 6 | 0 | 1 | 0 |
| `formats/mesh.md` | 15 -> 21 | 2 | 9 -> 3 | 0 |
| `formats/navsample.md` | 4 | 2 | 2 | 0 |
| `formats/res.md` | 2 -> 5 | 1 | 5 -> 2 | 0 |
| `formats/shaderdb.md` | 25 | 4 | 2 | 0 |
| `formats/stfs.md` | 4 | 0 | 0 | 0 |
| `formats/terrain.md` | 21 -> 23 | 0 | 4 -> 2 | 0 |
| `formats/texture.md` | 12 -> 16 | 1 | 5 -> 1 | 0 |
| `formats/water.md` | 5 | 3 | 0 | 0 |
| `formats/xiso.md` | 2 | 0 | 0 | 0 |
| `formats/xmlinst.md` | 3 | 1 | 0 | 0 |
| `model/ai_orders.md` | 3 | 0 | 12 | 2 |
| `model/bundles.md` | 5 -> 8 | 8 | 3 -> 0 | 0 |
| `model/level.md` | 14 -> 24 | 1 | 15 -> 5 | 1 |
| `model/references.md` | 5 -> 12 | 5 | 9 -> 2 | 0 |
| `model/registration.md` | 5 -> 9 | 3 | 9 -> 5 | 1 |
| `model/types.md` | 3 -> 19 | 6 | 17 -> 1 | 1 |
| `transforms/console.md` | 7 | 1 | 1 | 0 |
| `transforms/culltree.md` | 8 | 0 | 0 | 0 |
| `transforms/dbx.md` | 8 | 0 | 0 | 0 |
| `transforms/export.md` | 12 | 0 | 0 | 0 |
| `transforms/install.md` | 29 | 0 | 0 | 0 |
| `transforms/kynapse_generation.md` | 8 | 0 | 13 | 0 |
| `transforms/navkit.md` | 8 | 1 | 0 | 0 |
| `transforms/pack.md` | 9 | 0 | 0 | 0 |
| `transforms/projectlog.md` | 19 | 1 | 0 | 0 |
| `transforms/shaders.md` | 3 | 9 | 2 | 0 |
| `transforms/stockmp.md` | 7 | 0 | 0 | 0 |
| `transforms/terrain_edit.md` | 9 | 2 | 0 | 4 |
| `transforms/validation.md` | 70 | 1 | 0 | 0 |
| `transforms/ww2.md` | 10 | 0 | 1 | 0 |
| **all: 1269 rules** | **622 -> 749** | **127** | **473 -> 346** | **47** |

### 4b. The observed rules left, and what would verify each

346 rules stay observed. Kinds of evidence that would verify them:

- **P** (67): a data probe over DICE's PC files could count them (not written in SPV1: the shader
  databases' programs, animation and persistence partitions, the terrain probe's strip and tree sections, the
  weather states' classification, the levels' descriptions).
- **A** (23): the AI's data, measured by `rust/bc2-formats/examples/ai_probe.rs`: its value lists can
  become checks in `rust/bc2-formats/tests/ai_corpus.rs`.
- **L** (24): DICE's placement conventions measured by the map editor's L1 / A1 / A2 study
  (`bfbc2-map-editor/placement`): a re-run of the study as a check.
- **M** (23): a count over DICE's files backs part of the rule; its mechanism is the executable's (RE)
  or needs the game running.
- **X** (32): measured on another corpus than DICE's PC files - the Xbox 360 or BF1943 data, Napisal's
  or OPFOR's builds, OPFOR's sources, research rounds' scripts (WX0, WX1) - a check needs that corpus in the tests.
- **G** (21): only the game running settles it (playtests, RenderDoc captures, crash reproductions):
  in-game tests are paused.
- **R** (119): read from `BFBC2Game.exe` only; verified by a playtest or kept as RE.
- **N** (37): the Kynapse / NavKit files, left to round NKG2.

| File | Observed | By kind |
|---|---|---|
| `engine/ai.md` | 15 | P: AI-NEED-4; X: AI-NEED-1, AI-NEED-2; G: AI-NEED-3; R: AI-CRASH-1, AI-CRASH-2, AI-CRASH-3, AI-CRASH-4, AI-KY-1, AI-KY-2, AI-KY-3, AI-KY-4, AI-NEED-5, AI-START-1, AI-START-2 |
| `engine/animation.md` | 8 | P: ANIM-KIT-1, ANIM-KIT-3, ANIM-RAG-1, ANIM-RES-1, ANIM-RIG-1; G: ANIM-ZOOM-1, ANIM-ZOOM-2, ANIM-ZOOM-3 |
| `engine/audio.md` | 13 | P: SND-VEH-1, SND-VO-5, SND-WAVE-1, SND-WAVE-2; X: SND-OD-1, SND-VO-1; G: SND-LOAD-1; R: SND-EVT-1, SND-EVT-2, SND-EVT-3, SND-EVT-4, SND-NULL-1, SND-STRM-1 |
| `engine/bot_ai.md` | 44 | A: BOT-CMB-1, BOT-CMB-2, BOT-CMB-5, BOT-DIF-3, BOT-MOV-2, BOT-MOV-4, BOT-MOV-5, BOT-PER-1, BOT-PER-2, BOT-PER-5, BOT-SQD-1, BOT-VEH-1, BOT-VEH-2, BOT-VEH-4; G: BOT-DIF-4; R: BOT-CTL-1, BOT-CTL-2, BOT-CTL-3, BOT-CTL-6, BOT-CTL-7, BOT-CTL-8, BOT-GS-2, BOT-GS-3, BOT-GS-4, BOT-GS-5, BOT-PER-3, BOT-RT-1, BOT-RT-2, BOT-RT-3, BOT-RT-5, BOT-RT-6, BOT-RT-9, BOT-SCO-1, BOT-SCO-2, BOT-SCO-3, BOT-SCO-4, BOT-VEH-5 |
| `engine/decals.md` | 2 | P: DECAL-STRIP-1 |
| `engine/destruction.md` | 1 | R: DSTR-STATE-5 |
| `engine/effects.md` | 2 | P: FX-NS-1; R: FX-PROC-4 |
| `engine/gameplay.md` | 39 | P: GAME-LAYER-3, GAME-MODE-1; L: GAME-AREA-1, GAME-AREA-2, GAME-CQ-2, GAME-CQ-3, GAME-CQ-4, GAME-CQ-5, GAME-CQ-6, GAME-CQ-7, GAME-CQ-9, GAME-DMG-1, GAME-RUSH-2, GAME-RUSH-3, GAME-RUSH-4, GAME-RUSH-5, GAME-RUSH-6, GAME-RUSH-7, GAME-RUSH-8, GAME-SDM-2, GAME-SPAWN-1, GAME-SPAWN-2, GAME-SPAWN-3, GAME-TKT-1, GAME-VEH-1, GAME-VEH-2; X: GAME-AS-1; G: GAME-CQ-8, GAME-LAYER-1; R: GAME-AWARD-1, GAME-CQ-10, GAME-MODE-5, GAME-MODE-7, GAME-TKT-5 |
| `engine/kynapse_runtime.md` | 17 | R: KYRT-AREA-1, KYRT-CFG-1, KYRT-CRASH-1, KYRT-IGN-1, KYRT-LINK-1, KYRT-LOAD-1, KYRT-LOAD-2, KYRT-LOAD-3, KYRT-MOVE-1, KYRT-NODE-1, KYRT-NODE-2, KYRT-PATH-1, KYRT-PATH-2, KYRT-PATH-3, KYRT-SEC-1, KYRT-TILE-1 |
| `engine/levels.md` | 6 | P: LVL-DESC-1, LVL-MODE-1, LVL-MODE-2; G: LVL-DESC-2; R: LVL-RT-1 |
| `engine/lighting.md` | 12 | P: LIGHT-BAKE-1, LIGHT-BAKE-2, LIGHT-BAKE-4, LIGHT-BAKE-5, LIGHT-ENV-2, LIGHT-OUT-2, LIGHT-PT-1, LIGHT-PT-6; X: LIGHT-PT-3, LIGHT-PT-4, LIGHT-SKY-3, LIGHT-SKY-4 |
| `engine/loading.md` | 11 | R: LOAD-CSUM-1, LOAD-MG-1, LOAD-PART-1, LOAD-PART-2, LOAD-PART-3, LOAD-PART-4, LOAD-PART-5, LOAD-PART-6, LOAD-PH-1, LOAD-PH-2, LOAD-START-2 |
| `engine/materials.md` | 7 | P: MAT-TEX-2; X: MAT-WW2-2; R: MAT-SLOT-5, MAT-TEX-20 |
| `engine/menus.md` | 4 | R: MENU-CAT-1, MENU-EVT-1, MENU-RT-3, MENU-UNL-1 |
| `engine/module.md` | 1 | X: MOD-CNT-1 |
| `engine/network.md` | 9 (NET2: 23) | R: NET-JOIN-1, NET-SB-1, NET-SB-2, NET-SB-3, NET-SES-1, NET-SES-2, NET-SES-3, NET-SES-4, NET-SES-5; NET2 adds R: NET-AC-1, NET-CFG-1, NET-CFG-2, NET-CFG-3, NET-ONL-1, NET-ONL-2, NET-SRV-1, NET-SRV-3, NET-TRN-1, NET-TRN-2, NET-TRN-3, NET-VER-1, NET-VER-2, NET-VER-3 |
| `engine/onslaught.md` | 8 | X: ONS-LVL-1, ONS-PORT-1, ONS-PORT-3, ONS-REG-1, ONS-WAVE-1; G: ONS-CAP-1, ONS-CAP-3; R: ONS-MODE-1 |
| `engine/persistence.md` | 14 | P: PERS-AWD-1, PERS-AWD-3, PERS-DATA-1, PERS-DATA-2, PERS-DATA-3, PERS-RANK-1; R: PERS-AWD-4, PERS-JOIN-1, PERS-LIC-1, PERS-OPF-3, PERS-SAVE-1, PERS-SCORE-1, PERS-UNLK-1 |
| `engine/rendering.md` | 14 | P: RNDR-DB-2, RNDR-LIGHT-1, RNDR-SHAD-1, RNDR-SKY-3, RNDR-TERR-3, RNDR-WTHR-3, RNDR-WTHR-4; X: RNDR-DB-3, RNDR-WTHR-6; R: RNDR-CRASH-1, RNDR-CRASH-2, RNDR-DRAW-6, RNDR-DRAW-7, RNDR-POST-3 |
| `engine/streaming.md` | 6 | P: STRM-CNT-1; R: STRM-MNT-1, STRM-MNT-2, STRM-MNT-3 |
| `engine/vegetation.md` | 4 | P: VEG-TREE-1; X: VEG-UG-4; R: VEG-UG-3, VEG-UG-6 |
| `engine/vehicles.md` | 1 | R: VEH-HP-5 |
| `engine/weather.md` | 5 | P: WTH-FX-1, WTH-FX-2, WTH-STATE-2, WTH-STATE-4, WTH-STATE-5 |
| `formats/apt.md` | 4 | G: APT-FRM-2, APT-WR-4; R: APT-LOAD-1, APT-LOAD-3 |
| `formats/audio.md` | 2 | P: WAVE-SEG-2, WAVE-SEG-3 |
| `formats/bf1943.md` | 6 | X: B43-BNDL-1, B43-BNDL-2, B43-DBX-1, B43-HK-1, B43-LVL-1, B43-WPN-1 |
| `formats/collision.md` | 2 | P: COLL-ALIAS-2; R: COLL-MG-2 |
| `formats/culltree.md` | 4 | P: CULL-SLOT-1; X: CULL-SLOT-3; G: CULL-PLAT-1; R: CULL-TRV-1 |
| `formats/fbrb.md` | 2 | P: FBRB-LAY-1; G: FBRB-ENT-6 |
| `formats/havok.md` | 4 | P: HAVOK-MAT-1, HAVOK-PF-5, HAVOK-RUN-2 |
| `formats/hpd.md` | 14 | N: HPD-EDGE-2, HPD-NODE-3, HPD-NODE-4, HPD-NODE-5, HPD-NODE-6, HPD-NODE-7, HPD-RT-1, HPD-SET-1, HPD-TAB-3, HPD-USE-1, HPD-USE-2, KYM-AREA-3, KYM-DF-1, KYM-HDR-2 |
| `formats/kynapse.md` | 10 | N: KYN-AREA-1, KYN-B-1, KYN-C-1, KYN-DF-1, KYN-EDGE-2, KYN-EDGE-3, KYN-NODE-2, KYN-SEC-4, KYN-UNIT-1, KYN-WORLD-1 |
| `formats/loc.md` | 1 | X: LOC-KEY-2 |
| `formats/mesh.md` | 3 | P: MESH-PART-4, MESH-VTX-3, MSET-PART-2 |
| `formats/navsample.md` | 2 | P: NAVS-HDR-2, NAVS-HDR-3 |
| `formats/res.md` | 2 | P: RES-NAME-1; X: RES-TYPE-3 |
| `formats/shaderdb.md` | 2 | P: SHDB-PB-2, SHDB-RS-2 |
| `formats/terrain.md` | 2 | P: TERR-HF-6; G: TERR-VT-5 |
| `formats/texture.md` | 1 | X: TEX-X-8 |
| `model/ai_orders.md` | 12 | A: AIO-ASN-1, AIO-ASN-2, AIO-CLS-2, AIO-EVT-2, AIO-EVT-3, AIO-EVT-4, AIO-FLD-1, AIO-FLD-3, AIO-RDY-4; R: AIO-CLS-1, AIO-RT-1 |
| `model/level.md` | 5 | P: LEVEL-DATA-3, LEVEL-MG-4; G: LEVEL-MP-1; R: LEVEL-DESC-1, LEVEL-DESC-2 |
| `model/references.md` | 2 | P: REF-RES-3; R: REF-RES-5 |
| `model/registration.md` | 5 | G: CSUM-CHK-1, CSUM-CHK-2, REG-MFT-6, REG-NEW-5; R: CSUM-CHK-3 |
| `model/types.md` | 1 | P: TYPE-CPLX-1 |
| `transforms/console.md` | 1 | R: XCONS-STG-2 |
| `transforms/kynapse_generation.md` | 13 | N: KYG-AREA-1, KYG-AREA-2, KYG-AREA-3, KYG-DET-1, KYG-DF-1, KYG-DF-2, KYG-IN-1, KYG-NODE-1, KYG-NODE-2, KYG-NODE-3, KYG-NODE-5, KYG-SEC-2, KYG-SEED-1 |
| `transforms/shaders.md` | 2 | X: XSH-DX-1, XSH-DX-2 |
| `transforms/ww2.md` | 1 | X: XWW2-RB-2 |

