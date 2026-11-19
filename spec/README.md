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
| 1 | Container and file formats: byte layouts, encodings, platform variants | `formats/` | round SP1 (this round) |
| 2 | The data model: DBX types and values, instances, GUIDs and references, partitions and bundles, the level structure, registration, precedence | `model/` | round SP1 |
| 3 | Engine subsystems: what the game does with the data (level loading and the checksum, streaming, physics, rendering, AI / Kynapse, the menus' movies, sound) | `engine/` | later rounds (plan below) |
| 4 | The suite's transformations: each conversion and port as a function from input to output with its invariants (Xbox 360 -> PC, BF1943 -> BFBC2, level export, NavKit) | `transforms/` | grown by each porting round (plan below) |

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

`rules.json` lists every rule (ID, title, file, status, variants, evidence), written from the markdown by
`python spec/trace.py --emit` and checked by `python spec/trace.py --check` (in `run_tests.py quick`):

- every rule in the markdown is in `rules.json` and the other way round, with the same title, file, status and
  evidence;
- every status is backed by its evidence (table above);
- every cited test exists (the file and the `def` / `fn`), every cited code or doc file exists and has the line;
- every `spec: ID` comment in a test names an existing rule.

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
| **Navigation** | |
| [formats/hpd.md](formats/hpd.md) | Kynapse path data (`.hpd`) and its kymesh / graph blocks |
| [formats/navsample.md](formats/navsample.md) | the suite's navigation samples |
| [formats/culltree.md](formats/culltree.md) | static cull trees |
| **Menus, sound, text** | |
| [formats/apt.md](formats/apt.md) | APT / SwfMovie menu movies |
| [formats/audio.md](formats/audio.md) | Wave resources, EA-XAS audio |
| [formats/loc.md](formats/loc.md) | localization tables |
| [formats/aimanim.md](formats/aimanim.md) | AimAnimation |
| **The data model** | |
| [model/types.md](model/types.md) | the DBX type system and value encoding |
| [model/references.md](model/references.md) | instances, GUIDs, references in and across partitions |
| [model/bundles.md](model/bundles.md) | partitions and bundles, `*deleted*` markers, bundle precedence |
| [model/level.md](model/level.md) | the level: LevelData, its description, layers, Objects, event connections, model groups |
| [model/registration.md](model/registration.md) | `package.mft`, `bundleManifest`, the level checksum |

[rules.json](rules.json) is the index of every rule; [trace.py](trace.py) checks it.

## The plan

### Layer 1 and 2 (round SP1)

Written in this order, the containers and DBX first and deepest: FBRB and resource entries; DBX (binary, types,
references, XML); the data model and registration; terrain, water, collision / Havok; mesh and texture (PC and Xbox
360); shader databases; path data; APT; audio; loc / BIGF / STFS / XISO; cull trees; the rest. What this round left
for later is listed under **Remaining** at the end of this file.

### Layer 3: engine subsystems (later rounds)

One file per subsystem under `engine/`, each rule tied to `BFBC2Game.exe` evidence (`[exe: ...]`) or a playtest:

1. **Level loading and the checksum**: the order bundles are mounted, how a partition is resolved, what the
   checksum covers and what crashes it (0x6EFBC0 / 0x6EFCA4 / 0x6EFF8A); the server and client halves.
2. **Streaming**: `async` / `streaming_*` bundles, `ondemand_*`, what loads when.
3. **Physics**: the hit material and part material rules (OPFOR's MATERIAL_SLOTS_RE), destruction, vehicles.
4. **Rendering**: shader database lookup, the Dx9 / Dx10 / Dx11 paths, terrain and water rendering, the post chain
   (EDITOR_ARCHITECTURE 3.11 as the suite's reproduction).
5. **AI / Kynapse**: what the bots read from the path data and the level (KYNAPSE_SPEC.md).
6. **Menus**: the APT runtime, the movies' action script, the menu data.
7. **Sound**: banks, Wave graphs, voice-over.
8. **Gameplay data**: kits, weapons, vehicles, game modes as DBX classes (OPFOR's research notes).

### Layer 4: the suite's transformations (grown by each porting round)

One file per transformation under `transforms/`, written with the round that changes it; each rule states an
invariant between input and output and cites the test that proves it:

1. **Xbox 360 -> PC** resources (`bc2-convert`: Havok, textures, meshes, mesh sets, Granny, terrain, soot).
2. **BF1943 -> BFBC2** (the importer: DBX ports, Havok 4.6 -> 5.5, shader translation, emitter atlases).
3. **Level export** (`bc2-levelwrite`: the closure, cull trees, registration, the stage protocol).
4. **NavKit** (`bc2-navbuild`: the path data built from a level).
5. **Menus** (`aptgraft`, the HUD patches).

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
