# Partitions, bundles, `*deleted*` markers and precedence

A **partition** is one DBX file: one asset's instances. A **bundle** (`.fbrb`, `formats/fbrb.md`) is an archive of
partitions and resources. A level loads a **stack** of bundles, its own and shared ones, and a name means the first
copy the stack offers, unless a `*deleted*` marker hides it. This file states those rules; the stack itself is
declared by `package.mft` (`model/registration.md`).

## 1. Partitions

### PART-NAME-1: A partition is a `.dbx` entry

Status: **implemented**

Each `.dbx` entry of a bundle is one partition (its entry type `<non-resource>`). Its name is the entry name without
`.dbx`, with `/` separators (`Levels/MP_005`, `Objects/Vehicles/M1A2/M1A2`), compared without case: entry names mix
case, references use lower case (REF-FORM-2).

[code: rust/bc2-levelwrite/src/export.rs:1610-1615] [code: rust/bc2-levelwrite/src/export.rs:48]
[data: the game copy's 1,086 bundles: 156,533 .dbx entries, all of type non-resource; entry names in the mp_0* level bundles 22,541 lower case, 3,024 mixed]

### PART-NAME-2: Resources belong to partitions by name

Status: **implemented**

A resource (`.res`) that belongs to an asset sits beside its partition and is found by name, not by reference: a
blueprint `objects/x/y` has its collision in `objects/x/y_havok.res` (or `y_havok_havok.res`); a terrain
`terrains/<t>/<t>` its height field in `terrains/<t>/<t>.heightfield-000.res`; a model group partition
`levels/<lv>/<Group>_HavokAsset.dbx` its packfile `..._HavokAsset.res`.

[code: rust/bc2-editor/src/package.rs:586-588] [code: rust/bc2-levelwrite/src/export.rs:528-550]
[test: rust/bc2-editor/src/package.rs::registry_and_paths]
[data: Dist level bundles: 146 _HavokAsset.dbx entries, each with a HavokPhysicsData _HavokAsset.res of the same stem]

### PART-NAME-3: Files the engine finds by the level's name

Status: **verified**

Some partitions and resources are found through the level's name rather than by reference: the shader database
collection and its databases (`Levels/<LV>/Shaders.dbx`, `Levels/<LV>/Shaders/*.res`), the emitter system and its
atlases, the model groups (`*ModelGroupEntity*_HavokAsset`, `ForestEntity_HavokAsset`), the static cull trees and
the animation sound map. A level under a new name needs its own copies of them. DICE's mode level MP_005GR carries
all of them under its own name, with markers hiding MP_005's.

[code: rust/bc2-levelwrite/src/export.rs:457-480] [test: rust/bc2-levelwrite/src/export.rs::root_files_and_names]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:816-820]

### PART-EMPTY-1: An empty partition file is a marker

Status: **implemented**

A zero-byte `.dbx` is never a partition: it is a `*deleted*` marker written out as a file by older unpacks. Readers
report it as such (`DeletedEntry`), not as a damaged file.

[code: core/bfbc2core/dbx.py:135-141] [code: rust/bc2-scene/src/scene.rs:110-111]

## 2. Bundles

### BNDL-KIND-1: A level's bundles

Status: **observed**

A level `<lv>` has up to five kinds of its own bundle, each `levels/<lv>/<kind>-00.fbrb`:

| Kind | Holds |
|---|---|
| `level` | the level partition `Levels/<LV>.dbx`, its static groups, root files, level-only assets |
| `terrain` | the terrain's resources (height field, materials, water) |
| `ingame` | the in-game UI the level adds (minimap) |
| `loader` | the loading screen's data (loading music) |
| `vo/<lang>` | voice-over per language (`en`, `de`, `fr`, `it`, `es`, `pl`, `ru`) |

The shared bundles have the same kinds under `levels/<x>_common/` (`mp_common`, `sp_common`, `mp1_common`,
`mp2_common`, `nam_common`, and OPFOR's `ww2_common`).

[data: stock package.mft: 74 levels with level, terrain, ingame and loader blocks each, and 74 x 7 vo blocks]
[code: rust/bc2-levelwrite/src/export.rs:40-44]

### BNDL-LOC-1: Dist and Package

Status: **observed**

Bundles live in two trees: `game\Dist\win32\` (the original install) and `game\Package\` (patches and downloadable
content). Of the stock levels, 33 have their bundles in both (the Package copy an update), 41 only in Package
(Vietnam, the Bad Company 1 maps, mode layouts such as MP_005GR, the `MP_SP_*` levels).

[data: stock package.mft level sources: level / terrain / ingame / loader each 33 in Dist and Package, 41 in Package only, 0 in Dist only]

### BNDL-LOC-2: Registry names: `game:` and `package:`

Status: **verified**

The registry names a bundle `game:<path>` (under `game\Dist\win32\`) or `package:<path>` (under `game\Package\`),
`<path>` with `/` separators and the `-00.fbrb` file name.

[code: rust/bc2-editor/src/package.rs:118-126] [code: rust/bc2-levelwrite/src/export.rs:224-231]
[test: rust/bc2-editor/src/package.rs::registry_and_paths]

### BNDL-LOC-3: Registered bundles may be absent

Status: **observed**

The stock registry names bundles that are not in the game copy: every level's and common folder's `vo/<lang>`
bundle (518 sources) and six of the seven `streaming_vo_<lang>` bundles (only `en` installed). Stock levels load
with them absent.

[data: stock package.mft: 518 vo/lang sources with no file in Dist or Package; streaming_vo de, es, fr, it, pl, ru absent; OPFOR's English copy]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:830-831]

## 3. `*deleted*` markers

### BNDL-DEL-1: What a marker is

Status: **verified**

A `*deleted*` marker is a bundle entry of type `*deleted*` and size 0 (flags 0 in the game copy). It carries no data;
it says "this name does not exist for this bundle's level".

[code: rust/bc2-formats/src/fbrb/header.rs:11-12] [code: rust/bc2-formats/src/fbrb/header.rs:32-37]
[test: rust/bc2-formats/src/fbrb/header.rs::names_skip_markers_and_later_wins]
[data: the game copy's 1,086 bundles: 296,376 markers, every one size 0 and flags 0]

### BNDL-DEL-2: Markers hide shared data

Status: **verified**

A marker hides the same name in every bundle after it in the level's stack (PREC-ORD-1): the name resolves to
nothing, even though a shared bundle has it. DICE uses this to take shared assets away from a level (mp_005's level
bundle has 4,334 markers among 6,692 entries) and to let a level's own copy replace a common one.

[code: rust/bc2-levelwrite/src/export.rs:254-273] [code: rust/bc2-editor/src/package.rs:448-470]
[test: rust/bc2-editor/tests/g3.rs::stand_in_package_levels]
[doc: bfbc2-fbrb-tool/README.md:29-31] [doc: rust/BENCHMARKS.md:3720-3722]

### BNDL-DEL-3: A live entry beats a marker of the same bundle

Status: **implemented**

Within one bundle, a name that is both a live entry and a marker is live. (No game bundle has a name twice; the rule
matters for tools that pack such bundles.)

[code: rust/bc2-editor/src/package.rs:468] [data: the game copy's 1,086 bundles: no name listed twice in one bundle]

### BNDL-DEL-4: A level made from another must re-provide what the base hides

Status: **implemented**

When a new level loads a base level's bundle, it inherits that bundle's markers. Anything the new level needs that
the base hid (SP_01's level bundle hides `mp_common`'s neutral flag and spawn camera) must be copied into the new
level's own bundle, which comes before the base's in the stack.

[code: rust/bc2-levelwrite/src/export.rs:1085-1101] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:846-847]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]

## 4. Precedence

### PREC-ORD-1: First bundle wins

Status: **verified**

A level's stack for a bundle kind is ordered; a name resolves to the entry of the **first** bundle of the stack that
lists it: its data when the entry is live, nothing when it is a marker. Later bundles are not consulted for that
name.

[code: rust/bc2-levelwrite/src/export.rs:254-273] [code: rust/bc2-editor/src/package.rs:6-12]
[test: rust/bc2-editor/tests/g3.rs::stand_in_package_levels]
[data: G3, all 45 Package levels: markers hid 1 name a later bundle had on 9 levels; each level's opened scene equal to a whole unpack in that order]

### PREC-ORD-2: The stack's order

Status: **implemented**

A level's stack for a kind is: its own bundle, the Package copy before the Dist copy; then the `BundleTarget`s of its
`package.mft` block in the block's order (the base level's bundles for a mode layout, then the common bundles, for
example `nam_mp_002cq`, `nam_common`, `mp_common`). Duplicates are visited once.

[code: rust/bc2-levelwrite/src/export.rs:277-297] [code: rust/bc2-editor/src/package.rs:106-114]
[test: rust/bc2-editor/src/package.rs::registry_and_paths]
[data: stock level blocks: own bundle as package: then game: in 99 of 222 three-target and 33 of 74 two-target blocks with an own target]

### PREC-ORD-3: The Package copy of a file wins over Dist

Status: **implemented**

Where a Dist level has a Package copy, the Package copy of each file wins (the level partition, static groups):
the registry lists `package:` before `game:`, and the suite's loaders follow it.

[code: core/bfbc2core/level.py:146-161] [code: rust/bc2-levelwrite/src/export.rs:277-284]

### PREC-ORD-4: Within one bundle, the later entry of a name

Status: **implemented**

The suite's name index of one bundle keeps the later entry when a name is listed twice (the reference reader's
dictionary). No game bundle lists a name twice, so the game's choice is not known.

[code: rust/bc2-formats/src/fbrb/header.rs:61-71] [test: rust/bc2-formats/src/fbrb/header.rs::names_skip_markers_and_later_wins]

## Open questions

- Whether the game resolves names across the stacks of different kinds (a level partition referring to a partition
  only the level's `ingame` or the startup bundles hold); the export check counts the startup bundles in.
- The game's choice when one bundle lists a name twice.
- Whether markers act on resources (`.res`) the same way as on partitions; Vietnam's terrain markers say yes, but the
  streaming case (waves hidden by markers crash at 0xFF2AC2, EDITOR_ARCHITECTURE 8) shows at least one kind of
  resource reacts differently.
- The `async` and `streaming_*` bundles: when they are mounted and how their precedence relates to a level's stack.
