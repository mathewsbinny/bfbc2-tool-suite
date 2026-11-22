# Specification coverage: every BFBC2 system against the tools, the rounds and the rules

Which of BFBC2's systems the specification covers, which tools and rounds touch each one (now or queued in
`ROADMAP.md`, and Project OPFOR BFBC2's WW2 / Onslaught pipeline and game module), and where the gaps are. Written by
round SPG (2026-11-22); keep it current when a round adds a spec file or a system gains a tool. Rule counts are
`rules.json`'s after SPG (`python spec/trace.py --coverage` for the live numbers).

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
| **Terrain** (height field, patches, masks, destruction layer) | bc2-formats terrain, bc2-render, editor | E5 sculpt and paint, E7 | formats/terrain (19), RNDR-TERR (5), DSTR-TERR (4) | thin (no writer side) | thin: E5 needs the terrain writer rules (TERR open questions) | covered for E5: transforms/terrain_edit (15): what an edit writes, the atlas, flags, donors; E7 still needs the main block's tail decoded |
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
| **Effects, emitters, emitter atlases** | editor Effects overlay, emitteratlas (suite), OPFOR emitter_atlas, E3d cross-set copy | VW2c, VW2d | none -> **engine/effects (11)** | none | covered for the atlas and placement; the processor chain is thin | covered: the processor chain FX-PROC (3; its semantics open) |
| **Weapons and projectiles** (firing, damage, sway, zoom, ammo, grenades, attachments) | OPFOR bf1943_weapons, weapons, rifle_grenades, rg_projectiles, scopes, attachments; dbxport | translate weapons (hold), air raid (hold) | none -> **engine/weapons (34)** | none | covered | as after SPG |
| **Vehicles** (entity, physics, seats, health, spawners) | OPFOR bf1943_vehicles, vehicle_input_profile, boatrelease (DLL), console_vehicles / console_markv | repair stations, air raid, Air Superiority (hold) | SND-VEH-1 (1) -> **engine/vehicles (22)** | none | covered | as after SPG |
| **Materials and the material grid** (part / hit material, penetration, impact effects) | OPFOR material_grid, physmat, wake_material_states; havokport | OPFOR in Rust 4 | HAVOK-MAT (2), GAME-DMG-1 -> **engine/materials (30)** | none | covered; the hit material of plain objects open (playtest) | as after SPG |
| **Destruction** (health states, parts, bangers, destroyed meshes, collapse) | bc2-render destruction states, editor (SG1 members), levelwrite members | E4 (M-COMs), the map editor's group edits | HAVOK-RUN-3, GAME-DMG-1 -> **engine/destruction (22)** | none | covered | as after SPG |
| **Grime, dirt, camouflage** | OPFOR grime, grime_models, grime_sheet | translate soldiers (hold) | none -> MAT-TEX (4) | none | thin: MAT-TEX-3 inferred; the grime pipeline's own rules not written | as after SPG |
| **Vegetation and undergrowth** (TreeMeshSet, undergrowth, billboards) | mesh readers, bc2-render, conversion (TreeMeshSet) | **E6 vegetation** | MSET-PART-2, RNDR-TERR-4 (2) | thin | thin (plan, section 2) | covered: engine/vegetation (8) |
| **Animation and characters** (Granny, anim trees, aim, skeletons, soldiers, kits) | granny, aimanim, anim-tools, OPFOR anims / animswap / soldiers / kits / kitbags / skinning | translate soldiers (hold) | formats/granny (8), aimanim (3) | thin | thin (plan, section 2) | covered: engine/animation (17) |
| **Decals and roads** | bc2-render (baked strips) | E5 | RNDR-TERR-4 (1) | thin | thin (plan, section 2) | covered: engine/decals (6) |
| **Audio and voice-over** | console-convert XAS, OPFOR wake_vo / wake_ambience | E3c done | formats/audio (16), engine/audio (16) | covered | covered | as after SPG |
| **AI and navigation** (Kynapse) | auto-navkit, stockmp-ai, editor AI workspace, DLL navgen / navquery / forceai | **NKS (now)**, NK3-NK5, A1, WW2 navmesh (hold) | formats/hpd (36), navsample (8), engine/ai (16), engine/bot_ai (41, round AIS), model/ai_orders (16, AIS), transforms/navkit (8), stockmp (7) | covered (generator rules in NKS) | covered; NKS owns the rest | as after SPG |
| **Cull trees** | culltree, bc2-formats | PR1b-1 | formats/culltree (17), transforms/culltree (8) | covered | covered | as after SPG |
| **Menus, HUD, APT** | menu-tools, aptgraft, OPFOR menus, DLL menutree / onslaught_hud | IN1 diagnostics | formats/apt (31), engine/menus (4) | thin (runtime) | thin (the action-script runtime and the HUD patches) | as after SPG |
| **Networking and hosting** | DLL net / server_browser / patches | - | engine/network (9) | covered | covered | as after SPG |
| **Persistence** (stats, ranks, unlocks, licences, awards) | DLL persistence, stats_screens; OPFOR ww2_awards | Pacific War awards (decided 2026-11-15) | GAME-AWARD-1 (1) -> **engine/persistence (7)** | none | thin: the game's path and OPFOR's hooks; the award engine and `Persistence/*.dbx` classes not written | covered: PERS-DATA / RANK / AWD / SCORE / WW2 (12 more); the award engine not built |
| **OPFOR's game module: patch points** (27 files, about 170 addresses in `BFBC2Game.exe`) | `src/dll` | air raid, repair stations (hold) | network, persistence, AI-NEED-5, GAME-AWARD-1, crashes index | thin | thin (plan, section 2) | covered: engine/module (15): 221 addresses |
| **Streaming** (`async`, `streaming_*`, `ondemand_*`) | bc2-formats | - | SND-WAVE-2, SND-OD-1 | thin | thin | covered: engine/streaming (7); the mount functions not traced |
| **BF1943 as a source** (bundles, Havok 4.6.1, DBX classes, Xenos) | bf1943-import, OPFOR ww2 | Pacific War (hold) | HAVOK-V461 (2), XWW2 (9), WPN-WW2, VEH-WW2, MAT-WW2, LIGHT-SKY-4 | thin | thin (a source-side file still missing, section 2) | covered: formats/bf1943 (7, an index of the rest) |
| **Xbox 360 / Onslaught DLC data** | console-convert, console-readers, OPFOR onslaught (14 stages) | Onslaught without Napisal (hold), OPFOR in Rust 5 | transforms/console (8), TEX-X / MESH-CONV / HAVOK-X360 / GRANNY-CONV, LIGHT-PT-3 / 4 | thin | thin (Onslaught's mode rules and the DLC's content list) | covered: engine/onslaught (14); the DLC's full content list per port open |
| **Install, export, project log** | bc2-install, levelwrite, editor | IN1 | transforms/install (20), export (12), projectlog (20), pack (9), dbx (8), ww2 (9) | covered | covered | as after SPG |

**Totals** (36 systems). Before SPG: 769 rules in 46 files; covered 15, thin 13, none 8 (point lights, effects,
weapons, vehicles, materials, destruction, grime / camo, persistence - the last with one crash rule). After SPG: 933
rules in 54 files (164 new: 14 verified, 17 implemented, 114 observed, 19 inferred); covered 24, thin 12, none 0.

After SPG and AIS (round AIS added the bot AI's 57 rules): 1,051 rules in 59 files. After SPG2 (2026-11-22): 1,169
rules in 68 files (118 new: 22 verified, 42 implemented, 46 observed, 8 inferred; DSTR-TERR-4 observed -> verified by
the new game-tier check); covered 34, thin 2 (grime / camouflage, the menus' runtime), none 0.

## 2. What is left, in the order the queued rounds need it

Round SPG2 (2026-11-22) wrote items 1 - 11 of SPG's list (section 3 keeps that list as it was) and the probes' first
game-tier check (`core/tests/test_spec_probes.py`, `run_tests.py game`: 14 rules verified by it, 13 of them new, DSTR-TERR-4 observed -> verified). Left:

1. **Terrain for E7**: the main block's tail (TERR-HF-4) and the material map's encoding (TEDIT-MM-1); what reads the
   height field for collision (TEDIT open questions). E5's sculpt and paint rules are in place; every one waits on a
   playtest of an edited terrain.
2. **The suite's undergrowth reader** misreads NAM_MP_003's visual terrain (VEG-UG-2): a reader fix for the round that
   next touches `bc2-render/src/terrain.rs` (QW-sized).
3. **RE still needed**: the undergrowth numbers' meanings (VEG-UG-3), the emitter processors' semantics (FX-PROC-2),
   the mount functions (STRM-MNT-1), the `OnlyTakeableByTeam` reader (ONS-CAP-1), the award evaluator and whether bots'
   deaths are scored (PERS-SCORE-1 against AI-NEED-5: a kill-probe playtest), the compiled anim tree's layout
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
