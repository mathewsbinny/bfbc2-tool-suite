# Changelog

What changed for the suite's users, version by version. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/): each version lists what was **Added**, **Changed**,
**Fixed** and **Removed**, newest first.

**Versions.** The suite uses [Semantic Versioning](https://semver.org/) and is before 1.0, so a version is
`0.MINOR.PATCH`:

- **MINOR** goes up for a batch of features a user can see (a new tool, command, editor workspace or format).
  Before 1.0 a minor version may also change how a tool is called or what it writes; such changes are listed under
  **Changed** or **Removed**.
- **PATCH** goes up for fixes only.
- **A merge that removes something bumps MINOR at once** (round QW2, audit AU3 AR-9): a command line, a subcommand
  or option, a Python function or module another project imports (Project OPFOR's), a Rust public API, or a file
  format or folder a tool reads or writes. The merge itself writes the new `VERSION` and turns **Unreleased** into
  that version (`RELEASING.md`, "A merge that removes something"), so Project OPFOR's version pin
  (`opforcore.paths.SUITE_REQUIRES`, at least one version and below the next minor) sees every breaking change.
- 1.0 is the full suite release (`ROADMAP.md`, priority 8).

The version lives in one place, the `VERSION` file at the repository's root. `bfbc2core.__version__`,
`bfbc2.exe --version`, `bfbc2-editor --version`, the editor's Help window and the hub's title all read it. How to
release a version: `RELEASING.md`.

Versions 0.1.0 to 0.8.0 were assigned afterwards (round P1, 2026-11-19), from the commit history and
`rust/BENCHMARKS.md`; they were not tagged at the time. Each date is the day the last change of that version
landed on `main`.

## [Unreleased]

### Added
- **An XMA2 decoder in Rust** (round PR1b-1c, `rust/bc2-xma`): FFmpeg's `xma2` decoder ported, so the Xbox 360's
  EA-XMA sounds (BFBC2's console files, Battlefield 1943's) decode in process. It gives `ffmpeg -cpuflags 0`'s
  samples to the bit on all 4,657 XMA streams of both sound sets (spec rule XMA-DEC-1). The crate is under FFmpeg's
  licence, LGPL-2.1-or-later (its `README.md`). `audio.xma_decode` / `bc2_native.audio_xma_decode` expose it.
- Three project caches under the budget (`python -m bfbc2core.caches`, `bfbc2 caches`): `dbxcatalogue`
  (`work\tools_out\dbx_catalogue.pkl`, `BFBC2_DBXCATALOGUE_CACHE_MB`, 256), `ww2qa` (Project OPFOR's
  `work\ww2\_qa\cache`, `BFBC2_WW2QA_CACHE_MB`, 4096) and `ww2install` (its `work\ww2\install_*.pkl`,
  `BFBC2_WW2INSTALL_CACHE_MB`, 256).

### Changed
- **Converting sounds needs no ffmpeg** (`audio.convert`, `audio.decode`, `convert_many`, `bfbc2 audio`, Project
  OPFOR's WW2 sound port). Their samples are ffmpeg's C path's; ffmpeg's default run used CPU-dependent SIMD
  transforms that differ from it in the last bit of about one sample in 6,000, so a converted Wave can differ in a
  few bytes from one made with ffmpeg before. `audio.py` runs ffmpeg (with `-cpuflags 0`) only as the tests'
  reference (`decode_xma` / `decode_reference` / `convert_reference` with `ffmpeg=True`). `AudioError`'s advice no
  longer mentions ffmpeg.
- The cull-tree row cache and the shader-coverage index are keyed by content (round PR1b-1c): a reference level and
  its tree by their size and SHA-256 (`culltree` `CACHE_VERSION` 2), a level bundle by its SHA-256
  (`shaderdb.COVER_VERSION` 2), so a copied, restored or touched file keeps its entries and a file rewritten with the
  same size and time no longer keeps stale ones. Every entry is made again once.
- `mapexport build`: the level's closure and its shader reach walk the level's partitions once (round PR1b-1c,
  ALG-5); the outputs are the same bytes.

### Removed
- `bc2_convert::wavport::Ffmpeg` and the `ffmpeg` argument of `bc2_native.audio_convert` / `audio_decode` (and their
  `'ffmpeg'` answer): the decoder runs in process.

### Fixed
- **The same answers on every machine** (round CI1, GitHub's `checks` run): `texture.check`'s correlation (used by
  `convert.py verify-texture` / `bfbc2.exe convert verify-texture`) was NumPy's `corrcoef`, whose last bits follow
  the CPU's OpenBLAS kernel; it is now computed exactly in the Rust core (`bc2_convert::texcheck::correlation`, the same value
  in the module and `bfbc2.exe`). `navpath.route`'s `straight` distance was `np.linalg.norm` (a BLAS dot, the same
  issue) and is now the route estimate's own formula (`navpath.distance`). NavKit's route estimate and hull dots use
  a pure-Rust correctly rounded fma (`libm`) instead of the C runtime's. Spec rule XCONS-PAR-2.
- The test runner's green-crate fingerprints no longer fail when the temporary folder is on another drive than the
  checkout (`tests/test_rust.py`, GitHub's runner: D: and C:).
- Goldens no longer hold the recording PC's temporary folder: stockmp's validate log records compare it as `<temp>`
  in its long or 8.3 short form (`test_cli_o5.py`); `fbrbtool_golden_game.json`'s informational paths likewise.

## [0.18.0] - 2026-11-22

### Added
- **`bfbc2 mods`** (round MM1): Project OPFOR's mod manager on the install ledger - `list`, `status`, `enable`,
  `disable`, `install`, `uninstall`, `restore-original`, `verify`, `baseline`, `migrate`, `resume`. The mods: the
  Pacific War (`ww2`), its menus (`pacific-menus`), the Vietnam opt-in (`vietnam`), each Onslaught console port
  (`onslaught:<level>`, or `onslaught` for all), the map editor's exports (`mapedit`), NavKit's path data and stock-MP's
  AI per level (`navkit:<level>`, `stockmp:<level>`), OPFOR's import (`base`). A disabled mod's files are kept aside
  (`<game>\opfor\mods`: hard links, its level folders moved), so `enable` puts them back byte for byte with their
  write times, without a rebuild; the mods built on one (or needing it) are disabled with it after a confirmation
  (`--yes`) and come back with it; `restore-original <file> ... | --all` gives DICE's files back and keeps the installs;
  `verify` checks every file the ledger holds, the level folders and the kept copies; `migrate` is OPFOR's
  `migrate.py` in Rust (the plan first, `--apply` after). Any operation killed part way is finished by the next command.
  Spec `transforms/install.md` section 5 (XINST-MOD-1 to -9); `BC2_MODS_CRASH_AT` for the tests.
- **The hub's Mods tab**: the mods and their state, Enable / Disable / Uninstall / Verify / Restore DICE's files /
  Migrate old markers / Resume, a confirmation for what else changes.
- `bfbc2.exe`'s `mods` tool group (builds alone; in CI's list).

- `bc2_formats::shaderdb::merge`: `Merger::clone_entry` / `clone_entry_from_self`, `add_part` / `add_part_from_self`,
  `add_donor_kept` (a donor read for one call), `finished`, `current`, `retexture`, `name_guid` and the `Store` trait
  (where the records a merge makes live).

### Fixed
- The migration of the markers from before the ledger, killed after the first old backup was archived, left that file
  refused for good on the next run (OPFOR's `migrate.py` too: fixed on its side, branch `mm1-migrate`).

### Removed
- **`shaderdb.Merger`** (`bfbc2-shader-tools/shaderdb.py`'s Python merge class, round OR3a), with `shaderdb.ProgRef`
  and `Merger._shader_key` (now the module's `_shader_key`): the merge is the Rust core's. `shaderdb.merger(base,
  need=(), all_names=False)` makes a `bc2_native.ShaderMerger` (`bc2_formats::shaderdb::merge::Merger`) with the same
  calls (`add_donor`, `clone_entry`, `add_part`, `finish`) and lists (`entry_names`, `part_names`, `added`, `report`,
  `conflicts`, `part_conflicts`, `shared_programs`, `out`); `shaderdb.merge` uses it. The same answers as the Python
  class on its recorded ones (`tests/test_shaderdb_merge.py`: 271 built, damaged and random cases, 1,709 calls; the
  game copy's ONS_MP_008 + mp_008cq, 114 calls). Project OPFOR's 21 merges moved onto it (its `SUITE_REQUIRES` 0.18.0
  to below 0.19.0). It needs the Rust core: there is no Python twin.

### Changed
- The merge no longer keeps its donors: a donor is read from its bytes and only what the merge takes from it is kept,
  so a caller may let a donor go between calls (OPFOR's clone pass reads one donor level at a time). A name that is
  not latin-1 (an entry's or a part's) is refused with a `ShaderDbError`; the Python raised `UnicodeEncodeError` for
  an entry and appended a part it could not write. A donor that only exists in memory (its bytes do not read back:
  a shader naming a table past its end, a short record) is refused whole.
## [0.17.0] - 2026-11-22

### Removed
- **`bfbc2-auto-navkit/tools/navbuild.py`** (round NK5-rest), with its command line (`python navbuild.py <samples.bin>
  <forbid_tris.npy> <XOffset> <ZOffset> <out.hpd> ...`): NavKit's whole build is the Rust core's since round NK5, and
  `navkit.py build` / `apply` / `all` (or `bfbc2.exe navkit build`) build a level from its prepared inputs. Its thin
  calls moved to `navwalk.py` (`navwalk.build`, `water_tops`, `outside_bounds`, `deep_water`); its `BuildSettings`
  and module switches are the Rust defaults (`bc2_navbuild::stage::BuildSettings` / `Switches`), which a caller
  overrides with a dict (`navkit.py --no-detail`: `{'detail_features': False}`; `navvehicle.vehicle_settings()`);
  `bc2_native.navkit_settings()` lists them. Its Python stage, `merge_detailed`, `detail_pass`, `drop_one_way`,
  `restore_one_way` and `half_cells` twins went with it. No other project imported it (Project OPFOR has no caller).
  Every regression level builds the same bytes.
- Four NavKit tests became Rust tests (`rust/bc2-navbuild/src/stage_tests.rs`): `test_column_sectors.py`,
  `test_area_link.py`, `test_portal_symmetry.py` and `test_bridge_union.py` (their runner entries `column-sectors`,
  `area-link`, `portal-symmetry`, `bridge-union` with them); the detail pass's and route lines' checks of
  `test_detail_features.py` and the settings' checks of `test_tool_edges.py` too.

- `bc2_formats::dbxedit::EditFile` and `dbxedit::Element` (the one document is `EditDbx`), `dbxedit::run` /
  `run_many` / `Outcome` / `write_atomic` / `expand_commands` (now `bc2_tools::dbxedit`).
- `bc2_formats::{atomic, gameledger, meshindex, sha256}` and `fbrb::cache` (now `bc2_gamefs::{atomic, ledger,
  meshindex, sha256, bundles}`; `gameledger::Error` is `ledger::LedgerError`, `find_game` is `game::find_game`,
  `pid_alive` / `host_name` are `liveness::`), `fbrb::{read_path, extract_path, read_many, extract_many, pack_path,
  pack_many}` (now `bc2_gamefs::bundles::`; `bc2_formats::fbrb` keeps `read_path_uncached` / `extract_path_uncached`),
  `fbrb::{tool, unpack_path, unpack_bytes, UnpackError, UnpackStats}` (now `bc2_tools::`).
- `bc2_formats::bf1943::Payloads::read`'s folder argument: it takes a `PointsStore` (`bc2_gamefs::points::Folder`).
- `bc2_native.xas_encode` (no caller; `xas_encode_many` does its work).

### Changed
- `bc2_native.navkit_build`, `navkit_stage_grid` and `navkit_write_stage` take the build's settings and switches as
  overrides of the Rust defaults (`None`: the defaults), and refuse an unknown key; new: `navkit_settings`,
  `navkit_stage_answer` (a whole stage's answer, for the tests) and `NavLevel.drop_one_way` / `detail_pass` (the Rust
  stage's own, for the tests). `test_navbuild_native.py` checks the Rust stage's whole stages against answers recorded
  from navbuild.py's stage before it went (the same on all 10), and its E3c tracing through the Rust stage's
  `drop_one_way` and detail pass against the retired Python's recording (456 of 456, unchanged).
- Rounds NK2-NK4's parity scripts (`rust/parity_nk2_file.py`, `parity_nk3_detail.py`, `parity_nk4_walk.py`) hook into
  navbuild.py's stage, so they run against pinned checkouts (at `bda5b8d`, the last with navbuild.py, or earlier) and
  refuse a checkout without it.
- NavKit's path data is closer to DICE's in structure (round NKG1): each sector's table bounds are the box of its
  route nodes (KYN-SEC-2), no edge is written twice (KYN-EDGE-4), and border portals beyond a sector's square link to
  the diagonal neighbour (KYN-LINK-1; 5,922 such link entries on the 15 regression levels, DICE 5,682). Every navparity
  measure is unchanged; built files differ in bytes (`navbuild_golden.json` re-recorded). Edges only on owner border
  copies are written behind `navbuild.OWNER_EDGES`, off: with NavKit's single border row they would cut routes into
  the lower sector (KYRT-PATH-3).

- **One editable DBX document** (round PR1b-2, PR1b-1's decision): DbxEdit's model (`dbxedit::EditFile`) merged into
  `bc2_formats::dbx::edit::EditDbx`, the arena of shared chunks; DbxEdit's commands, the DBX inspector, its panel and
  the Compare panel run on it (`dbxedit::read` / `serialize` keep DbxEdit's reading and failure rules). Every output
  the same: DbxEditGUI's recording 1,232 of 1,232 steps, 31,037 prepared-level files back byte for byte, the export
  canaries. A text value's string index is now kept as the file stored it by the dbxbin reader too.
- **One varint** (`bc2_formats::varint`): the six copies are its callers.
- **The caches' budget, one implementation** (`bc2_gamefs::budget`): `bfbc2.exe caches` and `bfbc2core.caches`
  (its prune, `maybe_prune`, `prune_total` and `clear`, through `bc2_native.caches_prune` / `caches_prune_total` /
  `caches_clear`) prune with the same code; the Python stays for a missing module.
- **The mesh index is built on all cores**, one read of a bundle for all its shader databases, merged in the
  bundles' order (the same index).
- **The map editor's background jobs run on one `Task` type**: the start screen's catalog, a level's load and its
  render meshes, the export's profiles, preview and run, NavKit's comparison read (a worker that dies is an error
  naming the job).
- NavKit's path data is closer to DICE's in structure (round NKG1): each sector's table bounds are the box of its
  route nodes (KYN-SEC-2), no edge is written twice (KYN-EDGE-4), and border portals beyond a sector's square link to
  the diagonal neighbour (KYN-LINK-1; 5,922 such link entries on the 15 regression levels, DICE 5,682). Every navparity
  measure is unchanged; built files differ in bytes (`navbuild_golden.json` re-recorded). Edges only on owner border
  copies are written behind `navbuild.OWNER_EDGES`, off: with NavKit's single border row they would cut routes into
  the lower sector (KYRT-PATH-3).

### Added
- **Two new Rust crates** (round PR1b-2, audit AU5 ARC-2): `bc2-gamefs`, the game layer over the formats - a game
  copy found (`game::Game`, the seam for the later Frostbite games; `find_game`), every file written safely
  (`atomic`), the install ledger (`ledger`), process liveness (`liveness`), the bundle cache with its cached reads
  and the bundle writer (`bundles`), restart points on disk (`points`), the mesh index (`meshindex`), the caches'
  budget (`budget`) and the worker pool (`workers`); and `bc2-tools`, FbrbTool's and DbxEdit's command lines as a
  library (`bfbc2.exe fbrbtool` / `dbxedit` and `bc2_native` run it).
- **One worker knob, `BFBC2_WORKERS`**: sizes the Rust core's one pool in `bc2_native`, `bfbc2.exe`, the map editor
  and `mapexport.exe` (else `RAYON_NUM_THREADS`, else every core); `pcbuild`'s compiler threads take it when
  `BFBC2_HLSL_WORKERS` is not set (`bfbc2core.workers`).
- **The native census is a check** (`python rust\native_census.py --check`, `run_tests.py` quick, `native-census`):
  an export of `bc2_native` that no tool, test or Project OPFOR file calls fails; `--record` keeps OPFOR's callers
  (`rust\native_census.json`) for a checkout without OPFOR beside it.
- **The bot AI specification's static RE, finished as far as it goes without the game** (round AIS-rest;
  specification only, no tool changed): the AI body - the proxy's three interfaces and its seat requests
  (BOT-CTL-6); who writes which slot of the AI input object (BOT-CTL-7: reload and fire as digital actions, the
  infantry's move target, sprint, weapon slot and aim, the vehicle controls' analog throttle, brake, yaw, pitch and
  roll); stance, aim state, weapon slot, vehicle weapons and seats set directly (BOT-CTL-8); the creators of goals,
  behaviours, plans and controls (BOT-RT-9); the vehicle controls (BOT-VEH-5); bots as scored and named players
  (BOT-SCO-1 to BOT-SCO-4: the name at ServerPlayer +0xC2C, the three tests that keep bots from scoring); a capture
  point's owner and progress (BOT-GS-3) and the counters' and objectives' classes (BOT-GS-4); where a session's AI
  difficulty comes from (BOT-DIF-4); one server class for every order and the `IssueOrder` event hash (AIO-RT-1).
  The bot module's implementation interface is updated and the in-game test of the AI input object is written as a
  checklist. 12 new rules (69 in the two files).
- navparity's directed measures, the route graph as the game's runtime walks it (KYRT-PATH-3): `runtime_connectivity`
  and `strong_share` (navregress compares them); `kynapse_probe` counts one-way edges by their ends, C records against
  the square lines and the strongly connected share; two game-tier corpus tests (`one_edge_per_source_and_target`,
  `border_ids_have_edges_in_two_sectors`).

- **The terrain's last undecoded bytes, specified** (round SPG2-rest; specification and probes): a main height-field
  block's tail is a run-length coded depth map (the editor's `DepthMap.png`), a min / max tree of the heights down to
  16-sample cells and a lower envelope of grids 2 to n / 8 + 1 a side (TERR-HF-4, TERR-HF-9 - 11); the physics
  material map is 512 x 256-cell blocks of 4-bit cells, a repeated byte starting a run, over the main block with rows
  along z, its names resolved in the level's material grid by name (TERR-MM-2 - 4). Both re-encode byte for byte on
  DICE's 30 terrains, so the map editor's terrain rounds can write a new main block (E7) and paint footstep and impact
  materials (TEDIT-HF-3, TEDIT-MM-1). The undergrowth entries are the engine's `TerrainUndergrowthType`
  (VEG-UG-3, VEG-UG-5: density, variation, mask range, scales, wind, view distance).
- **Grime, camouflage and the menus' runtime in the specification** (`spec/engine/materials.md` MAT-TEX-5 - 19,
  `spec/engine/menus.md` MENU-AS / RT / EVT / HUD / ED, 41 rules) with two new probes, `spec/probes/grime.py` and
  `spec/probes/menus.py`.
- **Every specification probe is a game-tier check**: `core/tests/test_spec_probes.py` (`run_tests.py game`) now also
  holds SPG's five probes' counts (materials, destruction, lighting, vehicles, weapons), the two new probes' and the
  terrain tails and material maps: 172 checks; 74 rules move to verified.

### Fixed
- **CI's per-group clippy:** `bfbc2.exe`'s navtools and harness groups did not build alone since round NK6 (they
  called the navkit group's error mapping); the mapping now sits in `navshared`, which all three groups build.
- **CI without a game copy** (red since 2026-11-21; the development PC never saw it):
  - the editor's start screen offers the DBX inspector without a game copy too (it showed only "Open project" and the
    inspector, which needs no game, could not be opened);
  - the F4 hkpack test's built packfiles have their own random generator: they followed the damaged copies of the
    game's packfiles, so without a game copy they were other files and never matched their golden (re-recorded);
  - test_cli_o3 always runs from an isolated copy of the checkout: a `rust\target` build folder (CI's) added a line
    to `caches --report` that the goldens do not have.
- **The bot-kill scoring contradiction** (AI-NEED-5 against PERS-SCORE-1): the player-killed handler returns only for
  a null victim, so with OPFOR's offline-scoring patch a human's kill of a bot scores; what blocked it offline was the
  4-client scoring flag. AI-NEED-5, PERS-SCORE-1 and the open-question lists now say so (BOT-SCO-3); a playtest still
  has to confirm it.
- **More of DICE's Kynapse generator decoded** (round NKS-rest2; specification and probes only, no tool changed): the
  generator's rectangle is the box of the level's soldier and vehicle spawns grown by 25 m (KYG-RECT-1, verified on
  the 13 campaign levels by a new game-tier test, `rust/bc2-scene/tests/kynapse_rect.rs`); some route nodes come in
  pairs exactly one step of GraphAccuracy + 0.35 m apart along an axis (KYG-NODE-4, verified by a new test in
  `kynapse_corpus.rs`); every point of DICE's ground sees a node within half of DistEdgeMax (KYG-NODE-5, 96.5 % coarse,
  98.3 % detailed, 95.2 % vehicle). Node placement is not from the Havok collision, detailed sectors follow no
  level-data rule (best 92 %: designer zones), and the midline cut and the spanner's unexplained pairs are narrowed
  further. Two new probes: `rust/bc2-formats/examples/kynapse_rules.rs` (DICE's files) and
  `rust/bc2-scene/examples/kynapse_geom.rs` (DICE's files against the levels' collision, height field and entities).
- **NAM_MP_003's terrain in the game-accurate view**: the visual-terrain reader took an undergrowth entry's second
  string as one byte and refused that terrain, so the view of `nam_mp_003cq` / `r` / `sdm` / `sr` drew no terrain
  patches or road strips. The reader now reads the string and keeps the undergrowth entries
  (`bc2_render::terrain::PatchShader::meshes`); the other 33 visual terrains of the game copy read as before.

## [0.16.0] - 2026-11-22

### Added
- **The Kynapse specification's open rules, fitted** (round NKS-rest; specification only, no tool changed): DICE's
  route edges form a greedy 1.2-spanner (an edge is left out when the graph already has a route at most 1.2 times as
  long; KYG-EDGE-3) and the graph is symmetric by node id, a sector's one-way edges being edges into non-owner
  border copies (KYG-EDGE-4), both checked on DICE's 35 files by two new game-tier tests in `kynapse_corpus.rs`;
  from the executable, the node lookup's box query and its use of the lookup lists in order (KYRT-NODE-2), where the
  owner bit is read (KYRT-PATH-3) and the local-pathfinding layer beside the path data (KYRT-DYN-2). A new probe,
  `rust/bc2-formats/examples/kynapse_fit.rs`, records the per-feature fits.
- **Restart points in a deflate stream** (round PR1b-1-rest, `bc2_formats::restart`): a payload decompressed once
  is read anywhere after by inflating at most a span (1 MiB) from the nearest block boundary (zlib-rs through
  `libz-rs-sys`: `Z_BLOCK` stops, `inflatePrime`, the 32 KiB window as the dictionary, as zlib's `zran.c`). Each span
  keeps its CRC-32 and is checked on every read: bytes that changed are refused, never read as other bytes.
- **One BF1943 bundle reader** (`bc2_formats::bf1943`): `fbrb1943.Bundle`'s check (moved from `bfbc2.exe`'s
  `bf1943src`) and a compressed payload's reads through restart points kept on disk by the payload's content
  (`%LOCALAPPDATA%\bfbc2-tool-suite\cache\bf1943source\*.points`) and in the process (`BF1943_RESTARTS_MB`,
  `BF1943_SOURCE_MEMORY_MB`). `bf1943import.Source` and `RESTARTS` read through it (`bc2_native.bf1943_check` /
  `bf1943_read` / `bf1943_restarts`); the Python reader stays as the reference (`BFBC2_PURE_PYTHON=1`).
- **renderqa's truth index is a cache** (`renderqa`, `BFBC2_RENDERQA_CACHE` / `BFBC2_RENDERQA_CACHE_MB`, default
  64 MB): listed, pruned and cleared by `bfbc2 caches` and counted in the overall budget.
- **The auto layout's vehicles** (round A2, `bc2_levelwrite::vehicles`): a proposal places the mode layer's vehicles
  by team and class from the level's own vehicle spawners (moved, then copies), in DICE's per-mode counts (a Conquest
  base 4, 8 on WW2; a flag 2; Rush's first attackers' and defenders' bases 3 and 2; Squad Deathmatch's one IFV), each
  with its room, a drivable way out, water access for a boat or an open launch area for an aircraft, joined to its
  capture point's `Spawners`; committed with the layout as one operation. On 16 held-out stock levels: 0 errors,
  0 warnings, 446 of 446 statistics inside DICE's ranges (A1: 379 of 382; Squad Deathmatch's vehicle now placed).
- **Three new placement rules** (XVAL-PLACE-6 to 8): a land vehicle shut in (no 20 m way out), a boat without 24 m of
  open water, a helicopter under collision or a plane without a 30 m take-off run; 0 findings on DICE's 707 vehicle
  spawners of 27 stock levels.
- **Several proposals side by side** and **pins**: the Auto layout panel proposes 1 to 4 layouts at once, each in a
  column with its warnings, statistics and vehicles, one drawn in the views; "Pin selection" keeps objects where they
  stand and the rest is laid out around them (`Op::Pin`).
- **The export's path-data step** (opt-in: the Export window's "Stage the level's path data for bots",
  `ExportSpec.path_data`, `bc2_levelwrite::pathdata`): the base level's NavKit build (or DICE's file) staged with the
  level as `ai/infantry/pathdata.hpd` when the project changes nothing a bot walks on, refused in words otherwise;
  `mapexport check` holds it to XVAL-AI-1 (recorded, Kynapse, sorted nodes) and XVAL-PLACE-5; the AI workspace reads a
  stage's path data. Sampling and building an edited level waits for NavKit's port (NK5).
- **NavKit's build runs in Rust end to end** (round NK5): `bc2_navbuild::stage` (sector discovery, the areas, the
  border-node runs, the seam nodes carried between pitches, the one-way portal pieces, the detail pass's glue, the
  meshes' assembly, the route graphs and their ids, `merge_detailed`) and `bc2_navbuild::build` (walkability to the
  file, the build's log lines). `bfbc2.exe navkit build`, `all` and `apply` on a NavKit base no longer hand off to the
  Python (`--no-detail` included; `all` runs only the game's sampling as a `navkit.py sample` child);
  `navbuild.build` and `navvehicle.stage_on_grid` / `write` are thin calls of the module. Every regression level's
  file and log lines are the same bytes as before (15 of 15; the 4 golden builds as recorded); whole builds 234 ->
  151 s over the 15 levels, the largest build's peak memory 11.8 -> 6.9 GB.
- **The mesh viewer's index under the cache budget** (round PR1b-1, audit AU5 ARC-4): `work\tools_out\meshview_index.pkl`
  is the project cache `meshview` (cap `BFBC2_MESHVIEW_CACHE_MB`, default 256): listed, pruned and cleared by
  `bfbc2 caches` / `python -m bfbc2core.caches` and counted in the overall budget.

### Changed
- **The Kynapse specification, corrected by round NKS-rest**: `SectorOverlap` is the path-finding type's
  `DistEdgeMax` (15 m infantry, 50 m vehicle; KYRT-CFG-1); the search's step cost and heuristic are both the f32 3-D
  distance (`Kaim::CConstraintShortestPath`, `CHeuristicEuclidianDistance`) with 100 expansions per call
  (KYRT-PATH-1); the node resolver tests candidates in lookup-list order, not by distance, so the lists' tie order is
  read (KYRT-NODE-1, HPD-NODE-6); DICE's 13 % one-way edges are the border convention, not a directed graph
  (KYN-EDGE-3); the midline cut's "longer side" holds for 67 % of cuts over all files, not 325 of 326 (KYG-AREA-2);
  open questions OQ1, OQ2, OQ6 and OQ8 updated, and NavKit's gap list given NKG2's and NKG3's targets.
- **The bundle cache keeps restart points, not payloads** (`bc2_formats::fbrb::cache`): a large bundle read again
  gets a `.points` file (about 1 % of its payload; the WW2 replay's 8.09 GiB of payload files became 0.1 GiB), keyed
  by the bundle's content, so every copy of the same bundle shares one. A payload past the in-process budget is
  decompressed once per process, its points made in memory (audit AU5 ALG-5: the disk-off replay 150.6 s and
  68.9 GiB inflated before, 46.5 s and 28.0 GiB after); a read's spans are decoded on the worker threads.
- **Less memory** (round PR1b-1): NumPy's OpenBLAS runs one thread unless `OPENBLAS_NUM_THREADS` says otherwise
  (`suite_paths`, `bfbc2core`): each Python process importing NumPy committed about 490 MB for its 16 threads' buffers.
  Test peaks: geometry readers 2.9 -> 1.0 GB, NavKit cells and routes 5.4 -> 1.6 GB, texture helpers 5.2 -> 4.6 GB, model tools 5.5 -> 5.0 GB; the WW2 rebuild's steps each about 480 MB less (level builds 4.35 -> 3.87 GB).
  The batch readers of `bc2_native` (`dbx_parse_many`, `dbx_to_xml_many`, `fbrb_read_many`, `xenos_parse_many`,
  `granny_roots_many`, `granny_to_pc_many`, `texture_convert_many`) decode in a bounded window on all cores while
  the results are built in order, instead of decoding everything first: the Granny trees' peak 1.5 -> 0.9 GB, the
  Xbox shaders' 33 -> 16 s; every result the same.
- **The map editor builds NavKit in process** (round NK6): the AI workspace's **Build with these edits** runs NavKit's
  apply in the editor (`bc2_navbuild::apply` on a worker, its lines shown as they come) with a **Cancel the build**
  button; the files and lines are the `bfbc2.exe navkit apply` child's byte for byte (15 of 15 regression levels, NK1's
  33 apply files, 3 NavKit-base applies with region, seed and graph edits).
- **The export's path data built for the edited level** (round NK6): the path-data step builds the level with NavKit
  in process - the project's spawns and objectives as the build's seeds and objective floors, its navigation edits
  applied (traversal annotations staged beside the file) - and a project that moves static objects or edits static
  group members is sampled offline on its stage first (`navsample_offline.py stage`, into
  `work\navkit_stage\<level>`). On the auto layout's proposals for MP_002 and MP_005 every spawn, flag and M-COM is on
  the staged path data's main route network (XVAL-PLACE-5). Project OPFOR's export install puts the staged path data
  in the game copy through the install ledger.

- **NavKit's level driver is a library** (round NK6): `bc2_navbuild::inputs` (the build's inputs, the detailed
  sectors and their cache, the terrain unpack, DICE's extent, `original_file`) and `bc2_navbuild::apply` serve the
  command line, the editor and the export; `graphedit` no longer depends on `bc2-pyrt` (`bc2_navbuild::pyjson`).
  `bfbc2.exe navkit build` reads the objectives of a level prepared before `area_orders.json` from its XML instead of
  handing the run to the Python. `bc2_levelwrite::pathdata::ready` returns the step's plan (`Plan`: build, sample,
  copy).

### Removed
- **The bundle cache's whole-payload files** (`.payload`, format `BC2FBPL1`): no longer read or written. `bfbc2
  caches --prune` lists them as superseded and deletes them; they count against the cap until then.

## [0.15.0] - 2026-11-22

### Added
- **The Python inventory** (round PYI): `PYTHON_STATUS.md` and `python_status.json`, every `.py` file of the suite
  (358 on the QW3 merge, 104,486 non-blank lines) with its kind, status, Rust counterpart, callers (the suite and
  Project OPFOR BFBC2), retiring round and a note, grouped by folder with totals by status and folder: 22 files live
  (6,105 lines: only Python does the job), 21 partly ported (18,479), 55 twins (19,395), 20 ported thin callers
  (2,503), 3 in retired tools' folders (310), 50 that stay (benchmarks, parity and dev scripts: 7,975), 187 tests and
  helpers (49,719). Generated by `python rust\python_status.py` from its one table (the same bytes on every run); it
  also lists the 23 Python files `bfbc2.exe` embeds with `include_str!`. `tests/test_repo.py` (quick tier) fails when
  a `.py` file has no row or a row names a file that is gone. ROADMAP "Python left" and the README point to it.
- **The rest of the specification's gap pass** (round SPG2): 118 rules in nine new spec files and two extended ones -
  `spec/transforms/terrain_edit.md` (what a terrain sculpt, paint or new terrain must write: heights and their range,
  the patch map cells, the mask-tile atlas and its layouts, the donor rules, what the game computes; for E5 / E7),
  `spec/engine/vegetation.md` (forests, middlegrowth and the terrain's undergrowth; for E6), `spec/transforms/shaders.md`
  (BF1943's Xenos shaders translated for Dx10 / 11, the merge, Dx9's differences), `spec/engine/module.md` (every one
  of the 221 addresses OPFOR's game module patches or calls in `BFBC2Game.exe` build 795745, and how it checks them),
  `spec/engine/animation.md`, `spec/engine/decals.md`, `spec/engine/onslaught.md`, `spec/engine/streaming.md`,
  `spec/formats/bf1943.md`; `spec/engine/persistence.md` (the `Persistence/*.dbx` classes, BFBC2's and BF1943's ranks
  and awards) and `spec/engine/effects.md` (the emitters' processor chain). Two read-only probes
  (`spec/probes/terrain.py`, `spec/probes/effects_chain.py`) and the first game-tier check of probe counts,
  `core/tests/test_spec_probes.py` (`run_tests.py game`, entry `spec-probes:game`), which verifies 14 rules.
  Corrected in place from DICE's 30 terrains: TERR-VT-3, TERR-VT-4, TERR-VT-7, RNDR-TERR-5; LIGHT-PT-3's attribution
  of the game copy's ONS_MP_002 (OPFOR's console port, not Napisal's).
- **The bot AI specification** (round AIS): what a bot AI decision-and-control implementation needs in BFBC2, in
  two new spec files - `spec/engine/bot_ai.md` (41 rules: the server's AI frame and its time-sliced sensing / decision /
  behaviour passes, the per-player AI proxy and the bot object, goals / behaviours / plans / squad behaviours; the
  control boundary - an AI soldier is a server player with the AI flag, driven through an `EntryInputState` its AI
  input object pushes into the player's input handler like a remote human's, with the 13 actions DICE's AI sends and the
  ones it never does; perception, combat, movement, vehicles, squads, the game state a bot reads and the difficulty
  data; an implementation interface for OPFOR's bot module and the reverse engineering still needed in priority order)
  and `spec/model/ai_orders.md` (16 rules: the fifteen order classes, their fields, whom an order commands, issuing and
  chaining, the readiness and urgency data). Measured on DICE's AI partitions and 13 campaign levels by
  `cargo run --example ai_probe`; five rules verified by the game-tier tests `rust/bc2-formats/tests/ai_corpus.rs`.
- **The Kynapse specification** (round NKS): navigation meshing as BFBC2 ships and runs it, in three new spec files -
  `spec/formats/kynapse.md` (what path data means: worlds per pathfinding type, sector kinds and bounds, the resolution
  field as one over the tile size, areas, portals, links, node ids, edges, D / F by type, units and frames),
  `spec/engine/kynapse_runtime.md` (what `BFBC2Game.exe` does with it, from reverse engineering) and
  `spec/transforms/kynapse_generation.md` (DICE's generator as rules measured on its 35 files, with where NavKit
  deviates today and the gaps in priority order). `formats/hpd.md`'s open questions are tracked items, five answered.
  A game-tier corpus test checks the verified rules on DICE's files; `cargo run --example kynapse_probe` measures
  DICE's files or NavKit's builds.
- **The map editor's core before E4** (round PR1b-0, audit AU5 section 6): the views' editing tools are types behind
  one trait (`bc2_editor::tool::ViewTool`: input, click, drawing, keys, Esc, and what each asks for as an outcome),
  so E4's and E5's tools land as small types; background jobs are one type (`bc2_editor::task::Task`: a named
  thread, a guarded body, one answer, a dead worker reported). `cargo run --release -p bc2-editor --example
  editor_core_bench` and `--example preview_bench` measure the undo, memory, member-edit and preview numbers.
- **Auto layout** (round A1): the Gameplay workspace's Auto layout panel proposes where the target mode's flags,
  bases, M-COMs, spawns and combat areas go on the level, in DICE's style - the mode's layer (DICE's own layout,
  from the target's donor) turned, scaled and placed where the level's ground, water, rocks and buildings suit it,
  then held against DICE's statistics from the stock maps. The proposal is drawn over the views with its findings and
  statistics; nothing changes until you accept it, and an undo takes it back. A seed gives other proposals; the same
  seed gives the same one.
- **Placement checks on the ground** (round A1), live in the Problems panel: spawn points over water, inside rocks or
  walls or under the ground (XVAL-PLACE-2), a vehicle spawner without room for its vehicle or out of its domain - a
  tank in the sea, a boat on land (XVAL-PLACE-3), gameplay objects outside every combat area (XVAL-PLACE-4), and,
  once the AI workspace has read the level's path data, spawns, flags and M-COMs the bots cannot reach from the main
  route network (XVAL-PLACE-5). Each bound lies past what DICE's own maps do. `mapexport validate` checks the first
  three on the level's height field and water.
- **The specification's coverage matrix** (`spec/COVERAGE.md`, round SPG): every BFBC2 system against the suite's and
  Project OPFOR BFBC2's tools, the queued rounds and the spec rules that cover it, with what is left in priority order.
- **Eight engine specifications** (`spec/engine/`, 164 rules): `lighting.md` (baked occlusion, irradiance volumes,
  reflection cubes, the outdoor light, point lights and `PointLightEntityData`, the moon and god rays, DICE's night
  recipe), `weather.md` (the weather system's classes, DICE's 355 states and their value ranges, rain and snow
  effects), `effects.md` (effects, emitters and the per-level emitter atlases), `weapons.md`, `vehicles.md`,
  `materials.md` (the material grid, part and face materials, penetration, impact effects, grime and camouflage),
  `destruction.md` and `persistence.md`; the wrapper's material hash is now known (the name's 33-xor hash).
- **Read-only probes** (`spec/probes/`): the scripts that measured DICE's data for these rules (lighting, weapons,
  vehicles, materials, destruction); they print counts and ranges only.

### Changed
- **Undo, redo and History in the map editor are 3-4 times faster, the first ray after them 200-300 times, and 200
  operations' checkpoints hold a quarter of the memory** (round PR1b-0; SP_01: +279 MB -> +73 MB): the terrain, its
  ray grid, the height field and the level's files are shared between the base scene and each undo's, and the
  checkpoints share every part of a file their edits did not touch. A static group member's delete costs about 4 ms
  instead of a sixth of a second on SP_01 (its refusal check is not applied twice and the group is found without
  walking the partition per member type); object rays test only the parts of an object their path crosses; the asset preview's orbit is drawn on all cores (the same pictures). The DBX inspector's undo keeps
  at most 256 MB of steps. Every result is the same as before, bit for bit (`rust/BENCHMARKS.md` "PR1b-0").
- **Rust API** (round PR1b-0): `bc2_scene::scene::Scene::terrain` and `heightfield` are `Option<Arc<..>>`;
  `bc2_levelwrite::files::LevelFiles::partition` and `groups` hold `Arc<LevelWriter>`s; `bc2_scene::raygrid::RayCache`
  is shared by a scene's clones and keys the terrain grid by its mesh's `Arc`; the asset preview's reader answers
  through `Task`.

- **NavKit's walkability runs in Rust, end to end** (round NK4): a build stage hands the Rust core
  (`bc2_navbuild::walk`) the sample files and its inputs once, and the core reads the run, makes it coarse when
  needed, applies the hand edits, the water and reference-extent rules, the slope, clearance and structure rules, the
  seeds, floods, ladders, unseeds and roof islands. The same path data byte for byte on all 15 regression levels of
  the development PC (every stage's run, kept surfaces, gradients and stair flags bit for bit; the 4 golden builds as
  recorded). Walkability is 2.7 times faster over those levels (234 -> 88 s; the largest detailed pass 39.7 -> 8.9 s)
  and its memory peak 2.2-2.5 times lower (largest 12.8 -> 5.6 GB); the largest whole build's peak fell from 15.2 to
  11.9 GB. `navsample.load` reads a version 3 sample file a block at a time instead of whole. Spec XNAV-BLD-4 records
  the precision and order rules the port keeps.

### Fixed
- **A ported shotgun keeps DICE's shotgun body set** (round QW3): the BF1943 import's weapon conventions moved every
  `AnimBaseSet wabsShg` to `wabsRif`; that is right for BF1943's knife, the only BF1943 weapon with it, but DICE uses
  `wabsShg` on 20 of its 42 shotguns (all pump-actions) and on no other class. The rule now leaves it on a weapon
  whose `WeaponClass` is `wcShotgun` (spec WPN-WW2-8). BF1943 has no shotgun, so no WW2 build changes; its snipers
  keep `AlwaysAimHead` false as DICE's 32 do (WPN-WW2-4).
- **hkpack read an array of a type without a size as a list in a list** (round QW3): through the Rust core,
  `Packfile.read_object` / `read_value` returned `[[('?', n)]]` where the Python reads `[('?', n)]` (since round F4);
  `hkpack dump` in Python printed `m_odd: [[(?, 3)]]` in O2's live leg. The core's answer is the Python's again.
- **Tests that shared a fixed file in `%TEMP%`** (round QW3) - the argparse width test's trace
  (`argfmt_trace.txt`: two worktrees' runs at once failed with FileNotFoundError), the APT round-trip's movie
  (`aptgraft_test.res`), O4's prepare stand-in (`o4prep`) and O5's stock-MP stand-in (`o5s`) - now use a folder of
  the run's own; F4's shared mesh-viewer index cache is copied in whole before it is named.

### Removed
- NavKit's Python for walkability (round NK4): `navwalk.walkable`'s NumPy glue, `near_points`, `ladder_ends`,
  `objective_floor`, `coarse`'s loop and `_column_picks`; `navedits.region_mask`'s NumPy and matplotlib body and
  `edits_for`'s mask loop (navedits no longer needs matplotlib); the bodies of `navbuild.water_tops`, `deep_water`,
  `outside_bounds` and `stage`'s walkability block. The functions stay as calls into the Rust core with the same
  arguments and answers; `navedits.regions_of` and `navwalk.walk_inputs` / `walk_answer` / `run_dict` / `samples_of`
  are new. No command line or option went, and nothing outside NavKit used them. A version 1 sample run (no hit
  words), which no build could use before, now builds with every surface taken as terrain.
- **Rust API** (round PR1b-0): `bc2_editor::app::OutlineDraw`, `ViewState::outline` and `Measures::ruler` - the
  polygon's corners and the ruler's points are the tools' own (`bc2_editor::tool::Tools::polygon` / `ruler`).

## [0.14.0] - 2026-11-21

### Added
- **The export's rule registry** (round V1): every way an exported level can fail in the game, or be refused by the
  export, is a named rule with a code (`XVAL-REG-1`), a severity (error, warning or note), a plain-language message
  and the specification rules it rests on (`spec/transforms/validation.md`). `mapexport rules` lists them;
  `mapexport validate <project> --game <game>` checks a project as the editor does (exit code 1 on an error).
- **Live validation in the editor's Problems panel**: after every change the project is checked against the
  export's rules and DICE's placement tendencies, errors first, each row with its rule's code (its message and
  spec citations on hover) and a "Go to" link that selects the object and moves the views to it. Notes can be
  hidden.
- **Placement warnings from DICE's stock maps** (round L1's study): flag spacing, HQ distances, capture zones,
  spawn counts and distances, combat-area margins, Rush stage spacing and spawn distances, Squad Deathmatch spawns
  and vehicles - 49 rules, each finding quoting DICE's range ("Stage 3 is 40 m from stage 2; DICE's Rush stages are
  93-316 m apart"). Warnings and notes only, never errors; no stock level raises a warning.
- New export checks: a deleted marker hiding a sound (0xFF2AC2), the terrain bundle the base's own (black tiled
  cells), Xbox 360 textures or Havok physics in a stage, an edited static group losing a kind or gaining one (0xAB4E58; SG1's layout lines carry the same rule), the
  40 km/h road sign drawn with another level's entries (0x98BD6E), a note that the server and every client need
  the install; the build refuses an object moved or placed beyond +-2048 m in words.
- **Static group members are editable in the map editor** (round SG1). Most of a WW2 map's objects (Wake Island:
  2,999 group members against 1,329 placed objects: its buildings, fences, trees and props) live in model groups; they
  are now selected, moved and turned with the gizmo or the properties' numbers, deleted, duplicated and pasted like
  any object, and the asset browser's new **Place into** list adds an object of a kind a group holds to that group
  (any other kind is placed as a loose object, and the editor says why). Members keep their names across deletes and
  adds (`<group>#<k>` for the base's, `#+<n>` for added ones), every edit is one History step, and the last member of
  a kind in its group is refused in words. The export writes the group files, the group's physics and the client cull
  tree in DICE's layout: a moved member keeps its cull leaf, moved with it; an added one takes a deleted member's
  leaf or a new one. `mapexport check` now holds every model group of a level to the rules all 131 of DICE's groups
  keep.
- **Project format 7** (round SG1): member deletes and adds. Version-6 projects load as they are and are written
  with `min_reader` 6 until they hold one.
- **`bfbc2 install`: the game-copy installer** (round OR1, "OPFOR in Rust" stage 1; new crate `rust/bc2-install`).
  Project OPFOR's WW2 install (`wake_install.py --plan / --apply / --apply-staged / --revert`,
  `ww2_level_install.py`) builds the staging in Python as before, then writes an install plan (its decisions as
  data) and runs `bfbc2 install plan | apply | revert | marks`: the backup guard, each level's bundles and record,
  ww2_common, the DLL's markers, the shared bundles and ini files through the install ledger, the registration last,
  the verification, the opt-out and `--apply-staged`'s record and input fingerprint, with the records, files and
  printed lines of OPFOR's Python installer (proved on scratch copies of the game's affected files). It takes the WW2
  rebuild's lock (`work\ww2\_rebuild\rebuild.lock`, exit code 3 while a rebuild or another install holds it) and
  rolls an interrupted commit back before it reads anything; `BC2_INSTALL_CRASH_AT=<n>` (tests) stops it before its
  n-th write step. The ledger (`bc2_formats::gameledger`) gained OPFOR's stage operations: `install_keep` (an
  install keeping the owner's other files, in OPFOR's record order), `restate`, `file_layers`, `slot_holders`.
- **One whole-file writer for every Rust tool** (round PR1a, audit AU5 ARC-8): `bc2_formats::atomic::write` (a
  temporary file beside the target named per process and per write, flushed to disk, moved over it with retries) is
  what FbrbTool's and DbxEdit's commands, the install ledger, the installer, the caches, the logs' archive, the
  harness, stock-MP's report, texqa and the editor's saves now call; two threads or processes writing one path no
  longer share a temporary file (the ledger's was a fixed `.opfor-ledger-tmp`), and a recovery removes a killed
  writer's temporary files. `bfbc2core.atomicio` names its temporary files the same way.
- **Determinism as a rule** (round PR1a): the Rust workspace lints loops over hash maps (`clippy::iter_over_hash_type`;
  each remaining one says why its order cannot reach an output); the repository rules refuse a new unsorted JSON write
  in the tools' Python (`atomicio.dumps_stable`); `rust\native_census.py` lists who calls each `bc2_native` export.
- **Stable test ids** (round PR1a): every `run_tests.py` entry has an id; its resources, CI skips, durations and the
  round-code allowlists are keyed by it (`--list` shows it, `--only` matches it). A durations file keyed by display
  names is still read.

### Changed
- Every `mapexport check` line names its rule (`ok     XVAL-REG-1  registration: ...`), and every refusal of
  `mapexport build` starts with its rule's code. `CheckReport` has the codes beside its lines (`codes`, `lines()`,
  `findings()`).
- **`bfbc2.exe navkit apply` answers a DICE base in Rust** (round NK1): the side file's links, route edges and
  traversal annotations go into a copy of DICE's path data without Python (`bc2_navbuild::graphedit`), the same
  files, report and output as before (252 of 252 recorded command lines; every DICE file and NavKit regression build
  of the development PC byte-identical), 7-8 times faster and in a tenth of the memory. A NavKit base still rebuilds
  the level through `navkit.py` (its build is Python), whose edit stage now runs in the same Rust core. The map
  editor's AI workspace runs it as before.
- **`bfbc2.exe`'s usage and the crates' descriptions name tools by what they do** (round PR1a, audit AU5 NAM-2), not by
  the round that wrote them; the map editor's export hints and Help text too.
- **The editor's prepared-copy record lists its files in a fixed order** (round PR1a): before, the order of
  `prepared.json`'s files varied from run to run (hash order); `bc2_native`'s scene collision dict is in name order;
  `mapexport validate`'s and the Problems panel's vehicle-spawner findings and the export check XVAL-GRP-1's list of
  problems come out in the same order on every run.
- The Rust crates' Python-runtime emulation (`bc2-pyrt`) is no longer a dependency of the map editor; the mesh index
  cache is `bc2-convert`'s `pickle-cache` feature and NavKit's apply edits `bc2-navbuild`'s `graphedit` feature
  (round PR1a, audit AU5 ARC-1).
- Spec HAVOK-WRAP-1 and the console converter's FORMAT.md call the wrapper's n words part words: part-name hashes in a
  blueprint, the owners' leaves in a model group (round PR1a, audit AU5 NAM-4).
- **NavKit's build writes its path data in Rust** (round NK2): the build's last stage - the shadow-island rule, the
  order nodes' pruning, each sector's node and edge tables, the neighbour links of the runtime meshes and the file -
  runs in the Rust core (`bc2_navbuild::graphfile`). The same files byte for byte (all 15 regression levels of the
  development PC, and the 4 golden builds of `test_navbuild_native.py`), the stage 2.4 times faster over those levels (1.4-3 times per level) with a third less memory. The rest of the
  build (walkability, areas, tracing, D lines and F cells, the stage's glue) is still the Python driving the Rust
  core (rounds NK3-NK5).
- **NavKit's D lines and F cells are made in Rust** (round NK3): `navdetail` (each area's broken sides, D lines
  and F cells) runs in the Rust core (`bc2_navbuild::detail`). On every call of the 15 regression builds it gives
  the old Python's answers, except where the Python left a choice to its sets' iteration order. Those choices are
  now a spec rule (KYM-DF-2), for shapes DICE's own files never contain. An F polygon vertex in an empty 45-degree
  bridge cell, with no cell beside it on its record's side, takes the mean height of the nearest F cells toward that
  side. The Python took whichever nearest cell its set gave first. This moves 13 F vertex heights in 7 regression
  builds by 0.01-0.69 m; nothing else in any file changes. The 4 golden builds are unchanged. The detail step is
  5.9 times faster within a build (17.6 -> 3.0 s over the 15 levels).

### Fixed
- **Test O2 no longer trips Microsoft Defender** (round AV1). Defender's machine-learning scan blocked reads of the
  synthetic build-795745 exe that O2's aptrefs cases write (Trojan:Win32/Bearfoos.A!ml). Round QW2's layout passed
  for a few hours, then was flagged too. The cases now write that stand-in with `SI` in place of its `MZ` and
  `PE\0\0` signatures, so no scanner sees a program. Under `BFBC2_TEST_STAND_IN_EXE=1` (tests only), `bfbc2.exe`'s
  menu tools put the signatures back as they read it, so aptrefs reads the same bytes as before and gives the same
  answers.

### Removed
- `bfbc2-auto-navkit/tools/navlinks.py` and `navtraversal.py` (round NK1): navkit apply's links, route edges and
  traversal annotations are the Rust core's (`navkit.graph_edits` / `write_traversal` call it). No command line or
  option went; nothing outside NavKit imported them.
- Rust public API (round PR1a): `bc2_convert::meshview::cache::{locked, write_atomic}` (now
  `bc2_formats::atomic::{locked, write}`), `bc2_menu::common::STAND_IN_VAR` (the test switch moved to `bfbc2.exe`'s
  menu tools; `PeImage::open` has none, `PeImage::from_bytes` is new), `bc2-cli`'s copies of the shader-database
  helpers; `bc2_convert::meshview::cache` and `bc2_navbuild::graphedit` now need their crate's feature. Test files and
  `bc2-py` modules named by round were renamed by function (`rust/BENCHMARKS.md` "PR1a").
- NavKit's Python for a build's last stage (round NK2): `navislands.components` / `shadowing` (`navislands.prune`
  stays, a call into the Rust core), `navbuild.prune_order_nodes`, `finalize_graph`, `border_cell`,
  `same_grid_resolver` and `cross_grid_resolver`; `navbuild.sector_links` now takes the stage's sector and a
  `link_scope` (the resolvers are the Rust core's), `write_file` the build's anchors. No command line or option
  went; nothing outside NavKit used them.
- NavKit's Python for the detail features (round NK3): `navdetail.broken_sides`, `f_cell_sets`, `d_sides`,
  `_unit`, `corner_cuts`, `_cut`, `d_lines`, `_components`, `_quarters`, `_loops`, `_turn`, `_simplify`,
  `_f_point`, `SIDES` and `F_DIRECTION`. `navdetail.detail_features` and `f_cells` stay as calls into the Rust core. The new
  `navdetail.features` also gives the D sides and the F cells of each axis, which `navbuild.detail_pass` reads. No
  command line or option went, and nothing outside NavKit used them.

## [0.13.0] - 2026-11-21

Released by round GR4 under the rule above: DbxEditGUI, the suite's last C# tool, was removed (the map editor's DBX
inspector replaces it). Project OPFOR needs `SUITE_REQUIRES = ('0.13.0', '0.14.0')`; it calls no DbxEditGUI.

### Added
- **The map editor's DBX inspector** (round GR4: DbxEditGUI folded in): any binary `.dbx` - a file (typed or dropped),
  an entry of a bundle (read in memory), one of the open level's files, or the open project's level partition - as
  DbxEditGUI showed it: the tree, the attributes and value edited and applied (typed like the field, a rounded
  decimal said), Go to reference, Undo, Find / Next, the Instance menu (remove, graft, copy field, add item, copy
  GUID), the Edit menu (replace, replace prefix, remove a field everywhere, set a bool everywhere, the partition's
  GUID), the Level menu (check, against a cull-tree level, fix slots), Save (`.bak` first), Save as, Export / Import
  XML, Save edit script; new: Save into bundle (a copy of the entry's bundle), a field edit of the project's partition
  as an operation of the project (History, Ctrl+Z), a note under a number the XML prints as a whole number that may
  be a tiny decimal (its bits always kept). `bfbc2-editor --dbx <file>`, the start screen's "Inspect a DBX file", the
  command palette's "DBX inspector", the hub's More tools > DBX inspector. Identical to DbxEditGUI's recorded answers
  on 19 real files, 1,232 steps (`rust/BENCHMARKS.md`, "GR4").

### Changed
- The hub's More tools: "DBX inspector" (the map editor on a chosen `.dbx`) replaces "DbxEditGUI".
- `bc2_formats::dbxedit::encode_value_rounded`: `encode_value` and whether a decimal was rounded.

### Removed
- **DbxEditGUI** (`bfbc2-dbx-tools\DbxEditGUI.cs`), the suite's last C# tool, with `DbxEdit.cs` (the C# DBX classes it
  was built on), `build.bat`, `tests\DbxTests.cs`, `tests\run_tests.bat` and `tests\build_if_stale.py`; the test
  runner's "DbxEditGUI build (when stale)", "DbxEditGUI and the DBX classes it is built on" (quick) and "DbxEditGUI's
  DBX classes (DbxEdit.cs) on the corpus" (full) entries. Its window is the map editor's DBX inspector (above); the
  corpus round trip moved to `bc2-editor`'s game tier (`dbxedit_parity.rs`). `RETIRED.md`.

## [0.12.0] - 2026-11-21

Released at round OP1's merge under the rule above: OP1 removed Rust public API of the editor (`set_pending_target`,
the Export window's `project` / `bound` fields, `Install.ok`, `nav::Job.done`; `Then::Create` changed shape). Project
OPFOR needs `SUITE_REQUIRES = ('0.12.0', '0.13.0')`.

### Added
- **Map editor projects can edit any file of a level** (round OP1, audit AU1 R18). An operation names the files it
  changes - the level partition, a static group file, a height field, a paint layer, the navigation side file, or
  the project itself - and the new `files` operation edits several of them as one undo step: the partition and the
  static group members' transforms (Wake keeps 2,999 of its objects in group files). It applies whole or not at all;
  the editor keeps a writer per file in its checkpoints, so undo, redo and History bring every file back together.
  The export writes the edited group files in place of the base's. Editing members in the views is round SG1's.
- **Brush strokes stored as tiles beside the project** (round OP1, audit AU1 R24, for E5's terrain and paint
  brushes). A `stroke` operation names only the 64 x 64 tiles it changed, by sha1; the tiles live once each in
  `<project>.mapedit.json.tiles`. Undo and redo touch only those tiles. On a 2048 x 2048 height field a stroke's
  commit takes about 2 ms and an undo about 3 ms (`rust/BENCHMARKS.md` "OP1"). The export refuses projects with
  strokes until E5 writes terrain.
- **A journalled autosave** (round OP1, audit AU1 R17 / P3). The autosave is now an append-only journal, one
  checksummed line per change instead of the whole project after every edit; a crash (tested by killing the writer
  mid-line) recovers every whole line, and the journal is compacted when it grows. Older whole-project autosaves still
  read.
- **Closing with unsaved edits asks first** (round OP1, audit AU1 R7 / G-N20). Close map and closing the window
  ask "Save your changes?" - Save, Don't save (the edits and their autosave let go), Cancel - after the question about
  running work. A failed save keeps the map open and says why.
- **The game-accurate view loads on a worker** (round OP1, DF1's left item, audit AU4 CQ-6): the editor keeps drawing
  while a large level's shaders, textures and terrain are read; only the Direct3D device is made on the window's
  thread.

### Changed
- **Typed errors in the map editor and the renderer** (round OP1, audit AU1 D13 / AU2 Q3): `bc2_render::RenderError`
  (device, device removed, data, shader, unsupported) and `bc2_editor::error::EditorError` (project, level, render,
  GPU, input, file, tool, worker) replace the 155 `Result<_, String>` of the two crates; the words shown are the same.
  The Problems panel's entries are typed (`Problem`: severity, code, operation, objects): a changed base is a warning,
  an operation that did not apply an error.
- **The editor's side state as explicit states** (round OP1, replacing round DF1's side fields): the close question
  (`Closing`), the Export window's project (`ExportFor`), the preview (`PreviewState`: none, opening, shown), the
  level being prepared with what follows (`Mode::Preparing { job, then }`), the new project's era and mode carried by
  its Create (`Then::Create(name, folder, target)`), the install script's run (`InstallRun`), a navkit job's end, and
  the game-accurate view (`GameSlot`: off, loading, ready).
- **The editor's two god objects split** (round OP1, audit AU1 D9 / AU4 CQ-7): `ViewState` went from 74 fields to 45
  (eight focused parts: `browser`, `menus`, `outline`, `measures`, `inspect`, `props`, `pointer`, `game_view`) and
  `EditorApp` from 37 to 22 (`command`, `new_project`, `start_ui`, `catalog_state`). Behaviour unchanged: the editor's
  tests pass and seven screenshots of the main views (start screen dark, light, WW2; the demo level dark, light, with
  a selection, the navigation view) are byte for byte the same before and after.
- **Project format 6** (round OP1). A project records the sha1 of each level file it edits besides the partition
  (`base.files`) and `min_reader`, the lowest editor version that reads it (the newest operation kind in its log);
  a newer file is read when its `min_reader` allows. Projects of versions 1 - 5 load and are migrated as they load;
  their logs replay to the same partition bytes. A missing or damaged `version` is refused in words instead of being
  read as 0 or cut to 32 bits (audit AU4 BUG-13).

### Fixed
- **Project saves are durable** (round OP1, audit AU4 DS-20 / BUG-12): the temporary file is synced before it is
  moved over the project, the move is retried while another process holds the file for a moment, and the temporary
  file is removed when anything fails.

## [0.11.0] - 2026-11-21

Released by round GR3 under the rule above: the mesh viewer's page for a browser was removed (its command line now
writes the picture). Project OPFOR needs `SUITE_REQUIRES = ('0.11.0', '0.12.0')`; its WW2 tools keep working (they
call `meshview.page()`, which stays, deprecated).

### Added
- **The map editor's Asset preview** (round GR3: the mesh viewer folded in). Click a tile in Place Objects, or a mesh
  in the asset browser's new **Meshes** mode (every mesh of the game copy by name, as `meshview.py find`), and the
  Asset preview panel shows it: drag to turn, right-drag to move, the wheel to zoom (perspective or orthographic);
  **Side, Top and Front** are the mesh viewer's pictures pixel for pixel; every **level of detail**; the **parts**
  (switch each on or off, click one in the picture to select it, drawn in the viewport's selection orange); a
  blueprint's **destruction states**; every **texture** of a part's shader **by channel** (colour, normal, specular,
  mask, occlusion, detail, environment); **wireframe** and edges, **normals**, the **bounds** with their size and
  centre in metres, triangle and vertex counts; Shift+click adds a mesh below the others to **compare**; **Save
  picture** writes `work\tools_out\meshview\<names>_<view>.png` (a preset's is `meshview.py --png`'s). It reads the
  same index and cache as the mesh viewer (`work\tools_out\meshview_index.pkl`, built in Rust when stale).
- **`bc2_convert::meshview`**: the mesh viewer's index cache, `load_mesh`, `stack_models` and `render` in Rust,
  identical to the Python on 18 levels of detail of 8 real meshes (94 parts, 55 pictures; `rust/BENCHMARKS.md` GR3).

### Changed
- `bc2_render` (Rust API, round R5e): `frame::cbuffer`, `live::plan` and `live::plan_with` take the pixel shader's
  block window (`Option<[u8; 2]>`, before `extra`); `material::Draw` has a `ps_window` field; new
  `frame::material_window` and `bc2_formats::shaderdb::query::block_registers`. No caller outside `bc2-render`.
- **`meshview.py <mesh>...` writes the picture** (`<names>_<view>.png`; `--png` is still accepted) instead of the
  page; `--view`, `--size`, `--lod`, `--out` and `find` are unchanged (`bfbc2.exe meshview` too).

### Removed
- **The mesh viewer's page for a browser** (round GR3): `meshview.py` no longer writes `<names>.html` (three.js, the
  models embedded). The editor's Asset preview replaces it (`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 9.3 lists
  where each feature went). `meshview.page()` stays, deprecated, only because Project OPFOR's WW2 tools
  (`attachments.py`, `scopes.py`) still write their preview pages with it; `RETIRED.md` has the record.

### Fixed
- **The editor tests' hang** (round HG1): two tests making GPU devices at once could deadlock for good on NVIDIA's
  driver (a Vulkan device made while another thread exited through the driver's `DllMain` holding Windows' loader
  lock; 4 hangs in 9 loaded runs). The headless device is made once per process and shared, on Vulkan / Direct3D 12
  only (0 hangs in 30). A test held past `BC2_TEST_HANG_S` seconds now ends the editor test binary with the running
  tests' names and, with `BC2_TEST_HANG_STACKS=1`, each thread's stack (`rust\hang_stacks.py`, from a minidump); the
  quick tier sets both.
- **Exports without the bundle cache** (round HG1): the shader reach and the closure's copies read one partition at a
  time, each a decompression of a bundle payload of hundreds of MB; they now read a step's partitions together. A WW2
  canary export with `BFBC2_FBRB_CACHE=0`: 1,887 -> 96 s, the stage byte for byte the same.
- **The walk camera and surface placement on large levels** (round HG1): a ray tested every one of the terrain's two
  million triangles; the scene now keeps a plan grid of them and each blueprint's radius (about 0.5 ms a ray instead
  of 35 on SP_01, the same hits bit for bit).
- **EX1's preview test** (round HG1) prepares its canary under an id of its own per run, so two runs at once no longer
  share OPFOR's `work\editor\levels` folder.
- **Map editor: the WW2 levels' open sea is drawn** in the game-accurate view (round R5e). Past the shallows, Wake
  Island, Guadalcanal, Iwo Jima and Coral Sea showed a black (or dark grey) plane to the horizon; they now show the
  deep blue sea with the sun's highlight. The view wrote a material's constants only into a shader variable named
  `constants`; OPFOR's translated BF1943 water names its material `waterMaterial`, so its depth factors stayed 0 and
  the water drew nothing at all. A program's material now goes to the registers its parameter block's header names
  (spec RNDR-DRAW-8, RNDR-WATER-3). DICE's own levels draw exactly as before.

## [0.10.0] - 2026-11-21

Released by round QW2 under the rule above, for what rounds RT1 to RT3, GR1, GR2, EX1 and FM1 removed since 0.9.0
(command lines Project OPFOR called among them). Project OPFOR needs `SUITE_REQUIRES = ('0.10.0', '0.11.0')`.

### Added
- **A project lock and a check before every save in the map editor** (round QW2, audit AU1 R4 / G-N17). An open
  project holds `<project>.mapedit.json.lock` (the PC, the process, since when); a second editor refuses it in
  words ("... is already open in another editor (on this PC, process N, since ...). Close it there first ..."),
  a lock whose process is gone is taken over, and the lock goes when the project closes. Each save first checks that
  the file is still the one this editor read or last wrote (its modified time and length); if another editor or
  tool wrote it, nothing is saved, the message says so, and the changes stay in the autosave.
- **`mapexport check` refuses streamed wave data** (`_waveDataSns_` entries) in any staged bundle (round QW2, audit
  AU1 RISK-4; spec SND-STRM-1: the load crashes at 0x49EDF4).
- **Exit codes documented** (round QW2, audit AU2 Q7): README "When something goes wrong".
- **CI builds each of `bfbc2.exe`'s tool groups alone** (round QW2, audit AU2 A18; `checks.yml`); seven groups
  failed to (dead-code warnings in the shared argument formatter), now fixed.
- **Faster test tiers** (round TS1). The quick tier's single cargo entry (fmt, clippy, the workspace's tests: about
  53 minutes alone, every CPU) is four entries - the test build, the quick tests, clippy, fmt - and the tests run
  beside clippy and the Python entries. The whole-level editor, levelwrite and renderer tests (35, marked
  `#[ignore = "game tier: ..."]`) moved to the game tier's new entry; the quick tier keeps one real level per crate.
  Dependencies are optimised in debug and test builds. Crates unchanged since their last green run on this PC are
  left out of clippy and the quick tests, and said (never on CI, in the full tier or with `--no-skip`). Every Rust
  test's own time is kept (`%LOCALAPPDATA%\bfbc2-tool-suite\rust_test_times.json`) and the slowest are printed.
  `tests\test_rust.py list` names every test with its tier.
- **An optional compile cache**: `BFBC2_COMPILE_CACHE=1` with sccache installed runs every cargo of the suite through
  it. Off by default: it serves rebuilds of one build folder but not other worktrees' (cargo's per-folder `OUT_DIR`
  stays in sccache's key; README, "The Rust core").
- **`run_tests.py --timeout S`** and a default time limit per test (4 times its kept duration, at least 30 min /
  3 h / 8 h by tier): a hung test is ended with its processes and fails. The run's results and total go to the day
  log, not only "started".
- **Three test files in a tier at last**: `test_aimanim.py`, `test_column_sectors.py` and `test_trees.py` (quick, and
  game with `--game`); `tests\test_repo.py` fails on a test file no tier runs, and on a new runner entry or
  environment variable named by a round code.
- **Map editor: typed gameplay operations** (round XF1): set a team, link or unlink a soldier spawner and a capture
  point, set a team's tickets, insert or remove an outline point (the `Next` / `Previous` chain and the area's point
  kept). The project format is 5 (VW2 took 4); a version-4 project loads as it is, a newer one is refused in words.
- **`mapexport check`: shader and cull-shape lines** (round XF1): every mesh the level reaches has entries in its
  shader databases; every re-slotted object stands in its own blueprint's shape; no new object at slot 0 that some
  level slots.
- **The game copy's install ledger** (round DF2): every tool that writes the game copy records what it installed in
  one ledger (`<game>\opfor\ledger`, Project OPFOR's `opforcore.ledger` format): what each install replaced is kept
  once by its sha256, a revert gives back exactly what was there before that tool in any order, and an install or
  revert cut short (a locked file, a full disk, a kill) is rolled back by the next command. `bfbc2 gameledger
  status|holders|original|recover <game>` shows it and finishes an interrupted one. The suite's half:
  `bc2_formats::gameledger` and its Python twin `bfbc2core.gameledger`.
  Project OPFOR's half (its own DF2 branch): every Onslaught stage, the Pacific War install and menu mode, the map
  export's install and the import scripts write through `opforcore.stages` on the same ledger, and
  `opforcore\migrate.py` moves a game copy's old `.opfor-*` backup markers into it without losing an original.
- **Map editor: the mode view layer** (round VW2): show every object, the game mode layer only or the level without
  it, and a Rush level one stage at a time as it is played (L1's simulation of the stages' events: MP_005GR,
  MP_005SR, NAM_MP_002R and BC1_OASIS_GR give L1's bases per stage), in both views, the outliner and the overlays.
- **Map editor: weather in the views** (round VW2): every state of the level's weather system in a picker (night,
  dawn, fog, rain and cutscene states marked; the spawn-time switch and ActiveState named); the game-accurate view
  draws a state's light, exposure and sky and opens on the state players see (the spawn switch: MP_005's `day_1`,
  not its dark `default`); the simplified view is tinted by the state's look.
- **Map editor: weather in exports** (rounds VW2a-c; project format 4, `set-weather`): the Export window's Weather
  section makes the level start in one of its own states (ActiveState and the spawn switch set together), in another
  level's state (appended: this level's sky; or that level's whole system: its sky), or at night (DICE's
  `ons_mp_002_night` as the template, the reflection cube and the sky panorama darkened). Every new partition lies
  under `weather/<level>/` in the level's own bundle; `mapexport check` checks the weather. Night and rain are off
  by default and marked "not yet seen in game".
- **`xisolib` reads disc images held in memory** in the Rust core (round RT3: `find_partition_bytes`, `walk_bytes`;
  an `io.BytesIO` goes through them), so the Xbox disc reader has no Python-only path left.
- **`run_tests.py --help`** (also `-h`, `/?`) prints the runner's text and runs nothing (round RT3); an argument the
  runner does not know is a usage error (exit code 2) instead of being ignored.
- **A repository rule** (round RT3, `tests\test_repo.py`): every `tests\*.rs` of a Rust crate with `autotests = false`
  must be a `mod` of its test binary, so a test file can no longer silently never run.
- **Map editor: every name of an exported level** (round EX1). The Export window's **Names**: the level id (what the
  game loads, `Levels/<ID>`, and where its `<ID>_NAME` / `<ID>_DESC` texts come from), its folder under
  `Package\levels\` (the id in lower case, as on every level of the game: renaming the folder renames the id), the
  **server-browser name** (`maps.ini`: the launcher's row and the browser's `OPFOR - <name> (<mode>)`), and the
  **on-screen name and description per language** (the menus, the loading screen, the browser's map column), with
  each language's own letters (French, Russian, Japanese ... stored through that language's text table). The names
  are checked together and refused in words: a stock level's id or another project's installed export, an id
  holding `_NAME` (the loading screen would look for another picture: a black screen), a text or loading picture the
  game already has (`SP_05_A` would rename Heavy Metal), a folder that is not the id, a browser name another map has
  or too long for the row, a letter a language cannot show. They are kept in the project's log (`Op::SetNames`,
  "Keep in project", and on Export). `mapexport names <export.json>` checks them; `mapexport build` refuses names
  that fail; `mapexport check` repeats the rules on a stage and proves every place names the same level (the
  description, the menus' list items, every `package.mft` block and `bundleManifest` line, the name key's picture).
- **Map editor: Preview export** (round EX1). After an export, the stage opens read only beside the original base
  level, with the same camera: T switches between them, Close preview gives the project back. The Diff panel lists
  what the export changed (the mode layer in and out, the campaign content stripped, objects from other sets).
- **Map editor: the Diff panel** (round EX1, LevelDiff folded in). **Compare with base** (a project against its base
  level, again by itself after each edit) and **Compare with level** (the open level against any other): every
  object added, removed, moved or changed, with filters (kind, group - mode layer, from other sets, campaign
  content, world -, text, placed objects only); a click selects it and flies there; its fields as LevelDiff prints
  them (Inspect opens All fields). In the 3D view the differing objects are tinted and a box marks where an object
  of the other level stands, with a line from a moved object's old place: 4 new colour tokens (`view-diff-added`,
  `-removed`, `-moved`, `-changed`; never orange, the selection's).
- **`bfbc2.exe leveldiff`** (round EX1): LevelDiff's command line in Rust - the same arguments, text, `--json` file
  and exit codes, proven byte for byte on LevelDiff.exe's recorded answers on real level pairs - and 4 to 40 times
  faster. The hub's Level diff tab and Project OPFOR's `opforcore.paths.leveldiff()` run it.
- **Map editor: the AI workspace's navigation** (round GR2, the Navigation Studio folded in). The navmesh view mode
  and the AI workspace draw a level's path data: NavKit's newest build and DICE's file side by side (read on a worker
  thread), the walkable ground coloured by island, sector, pitch, difference (NavKit's extra ground, DICE's ground
  NavKit lacks, ladder ground) or anomaly, and each file's outlines, portals, border portals, route graph, ladder
  ground and ladder links as layers. Panels: **Navigation** (the files and their figures, the colouring, the layers,
  the selected area or spawn, the path test, the anomaly overlay, Compare with a reference AI layer - `level:`,
  `xbox:` or a partition - with filters, the nearest NavKit node of a marker and Export JSON), **Islands and
  spawns** (largest first, the spawns off the main network, each to fly to) and **Navigation edits**. The **path
  test** (Shift+P, or "Path test from here" on the ground): each file's cheapest route between two clicks, or why
  there is none. **Navigation edits** in a project: walkable and unwalkable regions, seeds, unseeds, linked areas,
  route edges added or removed, traversals and notes, drawn with tools (and the AI ground menu), each an operation
  of the project's log (undo, redo, saved with the project); the level's side file (`navtune.json`) saved and
  reloaded; **Build with these edits** (`bfbc2.exe navkit apply`), Open the result, **Install into the game** and
  **Restore DICE's original**, each showing what it writes in the game copy's AI path data before it runs. The
  hover shows the area under the pointer. 25 new 3D colour tokens (`view-nav-*`, `view-ref-*`).
  `bfbc2-editor <level> --navmesh` opens a level's navigation; the hub's Levels tab has **Open navigation**.
- **Map editor: the Map Inspector's views** (round GR1). Properties' **All fields**: every field of the selected
  object, nested, with its type (`int32`, `float32 x16`, `bool`, `text`, `reference`, `array [n]`, `struct`) and its
  value as DbxEdit's XML writes it; references lead to their object. A **Links** panel: what the object references,
  what references it, and the event connections it sends and receives (from the level's `EventConnections`); a row
  goes to that object, Back returns. The links are drawn in the views in the new `view-link` colour (references
  solid, event connections dashed, arrows by direction; K shows or hides them). **Measure** (M): clicks add points;
  the last segment's length, horizontal distance, height and slope, the path's total, the height above the terrain
  and below the water. A project's links and fields are read from its edited partition. The plan for the other
  GUIs is `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` section 9.
- **Specification:** the engine specification's first two layers, the file formats and the data model (`spec/`,
  439 rules with their evidence; `spec/trace.py` checks the index).

- **`bfbc2.exe`:** the shader tools (`shaderdb` with `merge`, `renderqa`, `xdiff`), the console readers (`apt`,
  `stfs`, `xiso`), `bf1943import`, and stockmp's `generate` / `build` / `validate` / `all` (round O5); NavKit's
  `detailed` and `navkit status` answered in Rust. `stockmp build` about 2.5 times faster, `shaderdb needs` about 8
  times.
- **Map editor:** any map exported for any era and multiplayer mode (round E3d). The era (BFBC1, BFBC2, Vietnam,
  WW2) and the mode are project settings (`Op::SetMode`, set in the New project form and the Export window): the
  modes offered are those the era's stock levels have (WW2: Conquest), the donor's mode layer comes into the project
  as editable objects (flags, bases, M-COMs per Rush stage, spawns, combat areas) and the base's own layer goes
  out, one undo step. "Use selection" moves a flag, base or M-COM with what belongs to it.
- **Map export:** the level's closure resolves under the target era's bundles: what another era's commons, another
  level's bundle or a deleted marker would leave missing is copied into the level's bundle and listed with its
  source (`stage.json`'s `closure`, `closure.txt`); a WW2 base's own BF1943 voice banks give way to the target
  era's; the map's terrain bundles stay whatever the era.
- **`mapexport check`:** the closure, the map's height field, and the mode layer's completeness against the donor's
  ("Rush stage 2 has one M-COM (the donor's has 2)").
- **Map editor:** Install and Uninstall in the Export window (Project OPFOR's `mapedit_install.py`: the plan first,
  then the install on confirmation).
- **Map editor: view toggles** (round VW1). The View dropdown's **Overlays** draw what the level's data holds, each
  with labels: **Effects** (each placed effect's emitters and their spawn volumes), **Lights** (emitters that act as
  point lights: their radius and colour; BC2 has no spot lights), **Sounds** (emitters with their minimum distance -
  BC2's sounds have no maximum, the label gives the falloff slope - and sound areas with their fade edge), **Combat
  areas** (the out-of-bounds edge as a wall, the outer edge, the team they bound), **Capture areas** (the flags'
  polygon and height or capture radius, HQs) and **Spawn protection** (each spawn point's enemy-check radius). Three
  new view tokens: `view-fx-effect`, `view-fx-light`, `view-fx-sound`.
- **Map editor: player's-eye camera** (View dropdown, Alt+W): the 3D view at a soldier's eye height (1.6 m standing,
  1.0 m crouching, from the game's soldier physics) with the level's field of view (55 degrees, x 0.82 on the
  campaign), walking on the ground with W A S D (Shift sprints, C crouches); Esc returns to the camera before.
- **Map editor: collision per destruction state.** The scene reads each blueprint's Havok parts
  (`bc2_formats::havok::parts`, `Collision::parts`); the Display section's State picker also chooses the collision
  drawn (Collision, Visual + collision), gains the states between intact and destroyed, up / down arrows and Page Up
  / Page Down, and says when a blueprint's collision is one piece in its files.
- **`requirements.txt`** (round QW1): the Python packages pinned to the versions CI uses; CI installs it, and so
  does README's first step (`python -m pip install -r requirements.txt`).
- **One registry of environment variables** (round QW1): `bfbc2core.envvars` lists every `BFBC2_*`, `BC2_*`,
  `BF1943_*` and `OPFOR_*` variable the suite reads, with what it does (`python -m bfbc2core.envvars [--set]`); a
  quick-tier test fails on a variable read but not registered.
- **`BFBC2_STRICT_NATIVE=1`** (round QW1): a stale or missing `bfbc2.exe` is an error for the ported command lines
  instead of a fall-back to their Python. `build_native.py --fingerprint [module | bfbc2 | bfbc2-editor |
  mapexport]`; `bfbc2-editor --source-hash` and `mapexport --source-hash`.

### Changed
- **Code moved between crates** (round QW2, audit AU2 A5): `guid::new_guid` into `bc2-formats` (from `bc2-stockmp`,
  which re-exports it), so `bc2-levelwrite` and with it the editor no longer build `bc2-stockmp`; the 32-bit PE
  reader `pe` into `bc2-formats` (from `bc2-harness`, which re-exports it), so `bc2-menu` no longer builds the
  harness. Same answers: the moved modules' tests pass unchanged.
- **Map editor: the Game-accurate view's sky, levels of detail and labels** (round R5d). The weather's sky dome is
  drawn behind the level (DICE's `EnvironmentMain` with the weather state's sky, sun and halo values: a blue sky over
  MP_005 instead of the flat fog colour; the panoramic clouds are left out until their composite is proven). Far
  terrain patches are drawn coarser (every 2nd to 8th sample past 200 m, their edges kept whole so no cracks open)
  and objects switch to their further levels of detail at the distances their mesh sets' LOD settings give (MP_005's
  wide view: terrain indices 10.7 M to 1.7 M, object indices 6.2 M to 2.9 M). Labels whose point lies behind what the
  game frame drew are hidden (the selection's never). Package levels (Vietnam, the Pacific War) open in 1.4 - 4.6 s
  instead of 14 - 18 s: their bundles are read at once and through the suite's bundle cache.
- **`bfbc2.exe` answers argparse's option abbreviations** as Python 3.13 does (`--fo` for `--format`, `--fo=v`, `--he`
  for `--help`) and its `ambiguous option: ... could match ...` error, in caches, convert, audio, shaderdb, renderqa,
  xdiff, dbxparity, portparity and texqa, instead of handing those command lines to the Python (round RT3). A
  `nargs='*'` option takes `--x=v`; texqa answers its help when it is the first argument. Single-dash abbreviations,
  `--` and texqa's argparse errors still go to the Python.
- **One rule when the Rust port cannot answer** (round RT3, audit AU2 A17; README "bfbc2.exe"): while a tool's Python
  is alive it answers (the command line handed to it, or a binding's Python reference); once it is retired, a plain
  error, never a silent fallback. The retirement error of `bfbc2.exe` names the round that retired the tool.
- **`hpdtool.parse` and `audio.read_wave` / `read_sps` / `read_snr` run in the Rust core only** (round RT3): a damaged
  file raises the error and message the Python raised (`ValueError` or `struct.error` for path data, `AudioError` or
  `struct.error` for sounds); a NaN in a file is read as Python's `struct` reads it instead of going to the Python.
- **`navkit.py`'s usage** says a level's navigation side file is made in the map editor's AI workspace (the
  Navigation Studio was retired in GR2).
- **Map editor: the simplified view draws the objects' render meshes** (round SV1). Trees, bushes and palms now look
  like trees, bushes and palms instead of "spike balls": the view draws each object's render mesh (the one the
  Game-accurate view draws) with its leaves cut out by their texture and a muted tint from its texture (vegetation
  green, rocks grey), instead of its collision. Invisible walls, occluders and see-through-only meshes (light shafts,
  vista smoke) are hidden at the start. New in the View dropdown: **Show objects as** Visual, Collision, Visual +
  collision, Wireframe or Bounds (`C` cycles them) and **Invisible objects** Hidden / Outline / Solid; the object
  menu's **Show as** and **Show every one like it as** (Visual, Collision, Both, Bounds, Hidden) for the selection or
  its blueprint; Properties' **Display** section: Show as, the mesh's **level of detail** (Auto / LOD 0 - n, where its
  mesh set has more) and a destructible object's **State** (Intact / Destroyed / All parts, from its blueprint's
  health states). These are view settings, kept per level with the editor's settings and never in the project's
  log; **Reset view overrides** clears them. A destructible building shows its intact parts (its mesh also holds its
  destroyed pieces). Clicking picks what is drawn; snapping and placing still use collision.
- **The map editor's own folder** (round FM1): `bfbc2-map-editor\` holds the editor's README (what it does, how to
  build and run it, its command-line switches, `mapexport.exe`, its tests) and its design document,
  `EDITOR_ARCHITECTURE.md` (moved from `bfbc2-map-inspector\` with its history; new section 10: the planned AI
  workspaces). The root README keeps a summary. `RETIRED.md` is the one record of retired tools, with the
  retirement checklist (`RELEASING.md` points at it). The editor's Help guide has an Export and install step; the
  hub's Xbox files tab runs `bfbc2.exe stfs` / `xiso` (the Python without the binary); `bfbc2.exe caches` and the
  Python no longer class OPFOR's deleted `work\import_backup` and `work\ledgertest`.
- **Map projects are format version 2** (round GR2: the navigation edits are new operation kinds). Version-1
  projects load unchanged (`bc2_levelwrite::project::migrate`, one step at a time; 1 to 2 changes nothing); a
  project from a newer editor is refused in words ("made by a newer editor version (project format N); this editor
  reads up to 2"), and an operation of a kind the editor does not know is named, not a bare JSON error. An editor
  before GR2 refuses version-2 projects the same way.
- **`bfbc2.exe culltree add`** reads the reference levels one at a time: about 80-110 MB instead of 1.65 GB, and
  0.6-0.9 s with a warm cache instead of 7 s. Its row cache is shared with `culltree.py` (either side's warmed cache
  serves both) and is pruned within the cache budget.
- **The last Python format paths run in the Rust core** (round F4; the same results, the Python kept as the
  fallback): `audio.convert` / `decode` whole (the ffmpeg glue included; `convert_many` converts side by side
  without the interpreter's lock), `texqa`'s texture readers (about 9 times faster), NavKit's offline sampler's
  collision assets (`asset_bodies`, its convex hulls; about 6 times faster), `hkpack.Packfile` as a handle (its
  dicts built only when read; `read_object` in Rust), `xenos.disasm`'s header (parse and listing about 8 times
  faster than the Python), `meshview`'s part reader and the cull tree tool's string table.
- **`python rust\build_native.py --download`** also downloads `bfbc2.exe` into `bin\` (checked by its SHA-256 and
  its source fingerprint), so a PC without Rust gets the command lines whose Python was retired (round RT1). A
  release made before RT1 has no `bfbc2.exe`: the module is installed alone and the download says so.
- **The map editor** prepares a level with `bin\bfbc2.exe prepare_level` only; without the binary the start screen
  says how to build or download it.
- **Retired command lines** (`graphstats`, `prepare_level`): a damaged or unusual input their Rust port does not
  read now ends with an error naming the retirement (exit code 1) instead of being handed to the Python.
- **Aim animations and navsample files** (`aimanim`, `navsample_offline.file_bytes`): read and written by the Rust
  core only. Inputs it refuses now raise (`AimError`, `ValueError`) where the Python twin used to answer some of
  them: an aim animation holding a NaN, samples of another dtype.
- **Shader databases are opened in the Rust core end to end** (round SD1; the same results, the Python kept as the
  fallback): `ShaderDb.parse`'s choice of byte order and layout and its messages, `pick_database` and
  `dx_from_name` are one Rust implementation shared by `shaderdb.py` and `bfbc2.exe shaderdb`; `load_levels` reads
  each level bundle's database list once instead of once per Dx.
- **`bfbc2.exe` answers `--help` and usage errors in a console and with `COLUMNS` set** (round RT2): `caches`,
  `convert`, `audio`, `shaderdb` (and its subcommands), `renderqa`, `xdiff`, `dbxparity`, `portparity` lay their
  help out at the console's width as Python's argparse does, instead of handing the command line to the Python;
  `texqa --help` too.
- **More of the suite reads and converts in the Rust core only** (round RT2: their Python twins retired against
  their recorded answers): the console converter's terrain, soot, mesh header / console layouts / decode / relayout,
  mesh set reader, Havok wrapper and class scan, texture levels / decode / check; the menu tools' BIGF reader and
  executable reader (`menucommon.parse_bigf`, `PEImage`); `navsample.load` (every version); the path-data mesh
  walk (`hpdtool.mesh_layout` / `mesh_coverage`); the EA-XMA parts, XMA2 RIFF and EA-XAS blocks of `audio.py`; the
  BF1943 importer's Havok port and MOPP code (`havokport`, `mopp`); `navtune`, `xmlinst`, NavKit's vehicle obstacle
  distance and offline sampler's terrain blocks. They now need the Rust core (`BFBC2_PURE_PYTHON` no longer switches
  them to Python). An input the core does not read (damaged data, a value of an unusual type) raises
  `bfbc2core.native.Refused` (a `ValueError`, and the module's own error class where it has one) with a message in
  the module's words; real files read as before.
- **Error messages kept:** the path-data mesh walk raises the Python's own `ValueError` / `struct.error` messages
  ("mesh trailer not found", "tile table at ...", reads past the end) from the Rust core, so `hpdtool coverage`
  prints them as before (now without the Python); the EA framing raises `AudioError`'s messages.
- **Each built artefact has its own source fingerprint** (round QW1): the module, `bfbc2.exe`, `bfbc2-editor.exe`
  and `mapexport.exe` are stale only when a crate they link (or a file they include) changes, so an edit to the
  editor or the renderer no longer makes the module and `bfbc2.exe` stale. `build_native.py --if-stale` rebuilds
  only `bfbc2.exe` when only its crates changed. The release tag stays the module's fingerprint.
- **A stale `bfbc2.exe` is said** (round QW1): the tools still fall back to their Python command lines, now with one
  warning per process naming the fix (`python rust\build_native.py`). `BFBC2_CLI_PYTHON=1` asks for the Python
  with no warning.
- **`mapexport.exe` and the map editor are found with a staleness check** (round QW1): Project OPFOR's
  `mapedit_install.py` refuses a stale `mapexport.exe` with the fix; the hub opens a stale editor with a note.
- **`caches --prune` deletes the bundle cache's superseded payloads** (round QW1), and the Rust core deletes a
  bundle's old payload when it writes the new one (a bundle rewritten at the same path left its old payload behind
  until the size cap reached it).
- **Tests** (round QW1): the map editor's, levelwrite's and the renderer's tests run once per quick tier (in the
  workspace's cargo test; `tests\test_editor.py` stays for a run by hand); each of those crates' integration tests
  is one test binary (`cargo test -p bc2-editor --test editor g2::` runs a file's tests); debug and test builds keep
  line tables only; the test runner counts a test that ran nothing as skipped, not passed (and names the tests that
  skipped parts), schedules each test with at least its last recorded memory peak, keeps matplotlib's font cache in
  `%LOCALAPPDATA%\bfbc2-tool-suite\matplotlib`, and forgets durations of tests that no longer exist.

### Removed
- **The Python command lines `aptrefs.py`, `xiso.py`, `culltree.py`, `stockmp.py` and `crashreader.py`** (round
  RT3). `bfbc2.exe` runs them alone (`python rust\build_native.py --download` fetches it; the hub and Project OPFOR's
  testrun use it); their libraries are `aptrefslib.py`, `xisolib.py`, `culltreelib.py`, `stockmplib.py` and
  `crashreaderlib.py` (`exeref.py` imports the last). A damaged or unusual input that used to go to the Python now
  ends with an error naming the retirement (exit code 1). `RETIRED.md` has the last commit with each.
- **The Python references `hpdtool.parse_reference`, `audio.read_wave_reference`, `read_sps_reference` and
  `read_snr_reference`** (round RT3), after their errors were ported; their recorded answers are the tests' reference.
- **`core\bfbc2core\viewer.py` and `pick.py`** (round RT3), the Python 3D view and its level picker that no tool has
  used since GR1 / GR2; with them `moderngl`, `glfw` and `imgui-bundle` leave `requirements.txt`.
- **LevelDiff.exe** (round EX1): `bfbc2-diff-workbench\` (`level-diff\LevelDiff.cs`, `build.bat`, its tests, and the
  folder's README of planned tools, which `DESIGN.md` 4.7 keeps). Run `bfbc2.exe leveldiff` (above) with the same
  arguments; its answers on recorded real level pairs are LevelDiff.exe's, byte for byte. `RETIRED.md` has the last
  commit with it.
- **Navigation Studio** (round GR2): `bfbc2-navigation-studio\studio.py` and `editing.py`, their tests, the hub's
  "Open in Navigation Studio" and its NavKit-file field, `tests\test_viewers.py` and `bench_sweep.py`'s
  `studio_load`. Everything it did is in the map editor's AI workspace (above; `EDITOR_ARCHITECTURE.md` 9.2), proven
  on its recorded answers. `overlay.py`'s headless exports (`--reference`, `--anomalies`) stay.
- **Python twins retired (round RT1):** the command lines `bfbc2-auto-navkit\tools\graphstats.py` and
  `rust\bc2-editor\prepare_level.py` (run `bfbc2.exe graphstats` / `prepare_level`; `BFBC2_CLI_PYTHON=1` no longer
  brings them back), and the Python references `aimanim.parse_reference` / `write_reference` / `to_pc_reference`
  (with aimanim's `use_native` switch) and `navsample_offline.file_bytes_reference`. The answers they gave stay
  recorded as the tests' reference data.
- **Python twins retired (round RT2):** the `*_reference` functions of `terrain`, `soot`, `mesh` (with
  `mesh.use_native`), `meshset.parse`, `havok` (`classes_reference`, `convert_wrapper_reference`,
  `use_native_wrapper`), `texture` (`levels`, `decode`, `check`), `menucommon` (`parse_bigf_reference`, `PEImage`'s),
  `navsample` (`load_reference`, `use_native`), `hpdtool` (`mesh_layout_reference`, `mesh_coverage_reference`),
  `audio` (`xma_parts_reference`, `xma2_riff_reference`, `_xas_blocks_reference`), `havokport` (14), `mopp` (5),
  `navtune` (3), `xmlinst` (2), `navvehicle.obstacle_distance_reference` and
  `navsample_offline.terrain_blocks_reference`. Their answers are recorded (`*_replay*.json` beside the tests,
  `tests\f3_golden_*.json`) and the tests check the Rust core against them (`tests\replay.py`).
- **The Map Inspector** (`bfbc2-map-inspector/inspector.py`, round GR1): its 3D view, layers, search and Selection
  panel were already the editor's; its property tree, "referenced by", link lines and Measure are the editor's All
  fields, Links and Measure (above). The hub's Levels tab opens levels in the map editor; the folder keeps
  `EDITOR_ARCHITECTURE.md` and a README saying where each feature went.
- **`bfbc2-map-inspector\`** (round FM1): its README's table is `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 9.1, the
  plan moved to `bfbc2-map-editor\`. **The placeholder folders** `bfbc2-ai-behavior-simulator\`,
  `bfbc2-bot-ai-debugger\` and `bfbc2-tactical-map-authoring-tool\` (a README each, never built): they are planned
  map editor workspaces (`EDITOR_ARCHITECTURE.md` section 10). `RETIRED.md` has the last commits that had them.

### Fixed
- **O4's texqa goldens failed on every run since round DF2** (round QW2): texqa's cache keys hash texqa.py's own
  source, so DF2's edit of `texqa.Dice` re-keyed the cached rows of three cases; the digests were not random (one
  per texqa.py revision). The test now compares the cache with that hash taken out of each key it can rebuild, so
  an edit of texqa.py no longer breaks it and a wrong key still does.
- **O2 on PCs with Microsoft Defender** (round QW2): the stand-in `b.exe` of the aptrefs cases is a loadable image
  (no holes between its sections, an import table), which Defender's machine-learning scan no longer blocks as
  `Trojan:Win32/Bearfoos.A!ml` (Errno 22). No exclusion or security setting is needed.
- **`texqa.Dice`'s reading of the install ledger's originals has a test** (round QW2): a stand-in game copy with a
  ledger.
- **Tests that passed by skipping** (round TS1, audit AU3 TS-7): the simplified-view, view-geometry and view-toggle
  tests turned a level that did not open, a game view that did not start or render meshes not read into a "skipped"
  that passed; with the level in the game copy each is now a failure. The Rust tests' "<level>: skipped, ..." lines
  are counted and named per test.
- **The test runner** (audit AU4 DF4): `--only` matching nothing is exit code 2 (it said "0 of 0 passed", exit 0);
  every test's processes end with the runner however it ends - Ctrl+C, a crash, killed - (its job objects are
  kill-on-close), and a passing test's leftover child process no longer outlives it.
- **`build_native.py`'s install** retries a target held for a moment (a scanner), and when the new file cannot go in
  puts the live one back instead of leaving it moved aside.
- **Map editor: side state no longer outlives its project** (round DF1, audit AU4-SM-1 / -2 / -4): a project
  generation is counted each time a level or project is shown or the map is closed. A Create that fails no longer
  leaves its era / mode target for the next project opened (it was committed and saved into that project). The
  Export window lets go of its project when the map closes (it wrote the old project's names into the next one and
  exported the old one) and never writes into a project it was not opened for. Close map ends a preview first, and a
  preview answered after the project changed is dropped.
- **Map editor: Export, Install and the navigation commands wait for each other** (round DF1, audit AU4-CON-1 / -2):
  Export is disabled while an install or uninstall runs and Install now / Uninstall now while an export runs, each
  with the reason. Install now installs the stage its plan was shown for and refuses one exported again since
  (AU4-SM-3); the stage path is computed in one place from the trimmed level id. Navigation Install, Restore and
  Build wait for a running navkit command, and Close map waits too; install and restore run in the background like
  the build (the window no longer freezes); a navkit command still running when the editor closes is stopped and its
  half-written file removed.
- **Map editor: Export reads the project's own saved state** (round DF1, audit AU4-DS-4): not the autosave file's
  absence (a failed autosave writes none, so an unsaved session could be exported as the file on disk).
- **Map editor: a worker that fails is reported** (round DF1, audit AU4-BUG-4): an export, profile scan, preview or
  install whose worker panicked now ends with an error line (its panic's text) instead of "Exporting..." forever.
- **Map editor: closing the window asks while work runs** (round DF1, audit AU4-DS-13): an install, export or navkit
  command running when the window is closed is named, with Wait or Close anyway; the install's output goes to a log
  file, so a closed editor no longer breaks its pipes mid-install.
- **Map editor: the game-accurate view after a GPU reset** (round DF1, audit AU4-BUG-9 / -15 / -26, CON-11): a
  removed Direct3D device is an error (it was reported as a finished frame), the view is turned off and loads anew
  when turned on again; a failed game-view pick picks nothing and says why (it fell back to the hidden simplified
  view's object); the read-back refuses a non-RGBA8 pass; each shared frame is written only after the editor's
  device finished the frames that sampled it.
- **Map editor: textures freed with their views** (round DF1, audit AU4-CQ-2): closing a map or a preview frees its
  views' and the game view's textures in egui's renderer (they stayed in video memory for the session).
- **Map editor: small fixes** (round DF1): the recent projects compare paths as Windows does (case and slashes,
  AU4-CQ-8); the export settings are written whole and a failed write is said (AU4-BUG-14).
- **Map export: no voice-over without its bundleManifest line** (round DF1, audit AU4-BUG-10, spec XEXP-REG-2): a
  `bundleManifest` that does not read stops the export (it was read as empty: `package.mft` blocks without their
  lines, the checksum crash); a donor with voice-over on a game copy without one is refused. The startup, overlay
  and awards patches remove their scratch unpack on errors too (AU4-CQ-10).
- **Map export: objects from other levels and sets get their shader entries** (round XF1): the export merges into
  the level's Dx9 / 10 / 11 databases the entries of every mesh it reaches that the base lacked (placed objects,
  another mode's layer, another era's kits), from the levels that draw them. Without them the game's renderer reads a
  null program (0x98BD6E).
- **Map export: each object's own cull shape** (round XF1): placed, copied and grafted objects take their own
  blueprint's shape (from the base or another level), never their template's or another level's slot number.
- **Map editor: undo, redo and History no longer replay the whole log** (round XF1): checkpoints and a cached donor
  level; on Aurora as WW2 Conquest with 40 moves an undo went from 9-11 s to under 0.1 s.
- **Map editor: Paste copies what an object owns** (round XF1): a pasted flag has its own capture area and marker,
  so editing the copy leaves the original alone.
- **NavKit and stock-MP could lose DICE's path data** (audit AU4 DS-1): both installed `pathdata.hpd` with their own
  backups and did not see each other; installing NavKit, applying stock-MP, restoring NavKit and reverting stock-MP
  left NavKit's build where DICE's file was, and DICE's original was reported as NavKit's. Both now install through
  the ledger (owners `navkit:<lv>`, `stockmp:<lv>`): a restore under another tool's install takes NavKit's layer out
  from under it, every order of the two ends with DICE's file, and `original_file()` (the NavKit driver, `bfbc2.exe`,
  the map editor) answers the ledger's bottom layer. Their markers of before the ledger are adopted on first use;
  another tool's are refused in words.
- **stock-MP install** (audit AU4 DS-5, DS-6, DS-9, DS-19): `--apply` is one journalled commit (a failure part way,
  the old `Fail::Python` early return included, can no longer leave Dist and Package from different builds); an
  interrupted `--revert` is finished by running it again; the revert reads the ledger and the level's own folders,
  never the whole game folder (junctions and odd names elsewhere no longer refuse it); a stale backup is never
  restored over a newer file; the advice to delete markers is gone.
- NavKit: `tasklist` given 30 s (a hung one counts as the game running, AU4 BUG-20); a level's unpack goes to a
  temporary folder swapped in under a lock, never `rmtree(ignore_errors=True)` under a stamp that says clean (DS-14,
  Python and Rust); the harness, stock-MP report and texqa JSON written through per-process temporary names and the
  retrying replace (CQ-18); one SHA-256 in Rust (`bc2_formats::sha256`, CQ-9).
- **Map editor: the game-accurate view no longer draws the objects a project deleted or hid** (round VW2).
- **Map editor: Wake Island's and Coral Sea's black ground in the Game-accurate view** (round R5d). Three causes in
  the renderer, none in the data: the terrain's height scale was read as the database's placeholder 500 (Wake's
  terrain spans 1,000 m, so BF1943's sea-floor layers drew the beach and shallows as deep water: black); the water did
  not read the terrain's height map; and every terrain cell stretched its whole mask tile, where a tile spans a block
  of 8 x 8 or 4 x 4 cells (each cell repeated the block's mask: the ragged, blocky shorelines and sand of the WW2
  levels, and of DICE's own levels' tiled layers). Wake's lagoon view: 52.8 % of its pixels black before, 0.0 % after;
  Coral Sea's 10.2 % to 0.3 %. The deep sea past the reefs stays dark until the haze over the scene is drawn.
- **O2's command-line test on a PC running Microsoft Defender** (round RT3): Defender's machine-learning scan took
  the menu group's stand-in executable for a trojan and blocked every read of it (`OSError 22`); the stand-in now
  carries a text mark no tool reads, and a blocked file is reported as an antivirus block.
- **`run_tests.py --help`** ran the quick tier (round RT3).
- **Map editor: the cull tree view mode** called its no-slot colour orange (the selection's); it is yellow. Objects
  with a slot of their own are now green, and the slots and spheres use the 3D view's status tokens (round VW1).
- **`shaderdb.ShaderDb.parse`** with an unusual kind (a float, a negative or a very large `dx`) raised `TypeError` /
  `OverflowError` from the Rust core instead of reading the file as the Python does (round SD1).
- **The map editor's game-accurate view opens OPFOR's WW2 levels** (round GW1): Guadalcanal and Iwo Jima failed with
  "no WeatherSettings / WeatherSystem partition under weather/wake_island/". The view now finds a level's weather
  the way the game does: the placed weather entity (`WeatherSystemEntityData`) names its `WeatherSystem` and
  `Settings` partitions, read wherever the level's bundles hold them, instead of looking for partitions named
  `weathersettings*` in a guessed folder (the WW2 levels name their settings after the level). A level with no
  weather data opens under a default sky and the status line says so, instead of failing.

## [0.9.0] - 2026-11-19

### Added
- **Map editor:** the start flow (round G4): Open project, Edit existing level (multiplayer maps of each set:
  BFBC1, BFBC2, Vietnam, WW2) and Create level (from any existing level, or from a height map).
- **Map editor:** context menus per workspace, edit modes, sinking an object into the surface with Alt+wheel, deep
  duplicate, copy and paste, polygon selection, and the asset browser over the whole content set with its Spawners
  and Objectives modes (round E2b).
- **Map editor:** the game-accurate view is interactive and editable: edits show at once and picking works in it,
  with the picture unchanged (round R5c).
- **Map editor:** export profiles for BFBC2, Vietnam, WW2 and BFBC1; an export gets its own name and description in
  every language and its own loading and map-list pictures; the campaign strip checklist for exports from a
  campaign level (round E3b).
- **Map editor:** an export's announcer voice-over follows its profile: WW2 exports use BF1943's voices, Vietnam
  the Vietnam DLC's, BFBC1 and BFBC2 their own; `mapexport check` verifies the voice-over registration (round E3c).
- **The suite's icon** (the author's M67 render) on `bfbc2.exe`, `bfbc2-editor.exe`, `mapexport.exe`, the editor's
  window and the hub's window (round I1).
- **`bfbc2.exe`:** more of the tools' command lines in Rust, with the same arguments, output and files as their
  Python: `aptcheck`, `aptrefs`, `bc2loc`, `exeref`, `crashreader`, `hpdtool`, `hkpack`, `convert`, `aimanim`
  (round O2); `caches`, `stockmp`, `aptgraft`, `audio`, `dbxcatalogue`, `dbxparity`, `portparity`, `meshview`,
  `culltree` (round O3); `navkit`, `prepare_level`, `levelobjects`, `ladders`, `ordercheck`, `navcompare`,
  `graphstats`, `hpdcheck`, `hpdview`, `detailed`, `navparity`, `navregress`, `sample_all`, `harness`, `texqa`
  (round O4).
- **Versions:** the `VERSION` file, this changelog, `ROADMAP.md` and `RELEASING.md`; `bfbc2.exe --version`,
  `bfbc2-editor --version`, `bfbc2core.__version__`, the version in the editor's Help window and window title and in the hub's title
  (round P1).
- **Tests:** `python run_tests.py quick --no-game` runs only the entries that need nothing but the checkout and
  names each one it skips, with the reason; `--ci-steps` leaves out the entries CI runs as steps of their own
  (cargo, ruff, mypy) (round P1).
- **CI:** `.github/workflows/checks.yml` checks every push and pull request on Windows: the module's build, cargo
  fmt, clippy and the workspace's tests, ruff, mypy and the quick tier's game-free entries (round P1).

### Changed
- Cargo builds go outside the checkout (`rust-targets\<checkout>`, or `BFBC2_RUST_TARGETS`), the built binaries to
  `bin\`, and every cache shares one size budget (round D1).
- The prebuilt Rust module's releases: only the newest is kept, with a plain title and notes.
- `harness compare` lists the tests and levels missing from or new in a run sorted by level, scope and test.
- The module's source fingerprint includes `VERSION`, so a new version is a new build of `bfbc2.exe` (round P1).

## [0.8.0] - 2026-11-18

### Added
- **The map editor**, `bfbc2-editor.exe` (rounds G1-G3, E2, E3, R5, R5b): a read-only map view of any level (egui
  on wgpu, the suite's design language in dark and light), team labels by faction, a searchable Help built from the
  action registry; the levels that live only in `game\Package` (Vietnam, the Pacific War's WW2 maps, BFBC1) and a
  "Choose mode" start screen; map projects with an operation log (save, undo / redo, autosave and recovery) and the
  Objects workspace; export of an edited level as a new multiplayer level; a game-accurate view drawing the whole
  level with the game's own Direct3D 11 shaders, pixel for pixel against the C++ render harness.
- **`bfbc2.exe`**, the suite's command lines in one Rust binary: `fbrbtool` and `dbxedit` first (round O1).
- Rust ports of the stock-MP AI layer (E4a), the shader database logic (E4b), aptgraft, aptcheck and aptrefs
  (E4c), dbxport (E4d), NavKit's tools around the build (E4e), the test harness and the crash-reader tools (E4f),
  the file formats that had no Rust (F0: path data, localization, BIGF, STFS / XISO, Wave, mesh headers), the
  console converter's rest, the model tools and Granny writing (F2a), the BF1943 importer's remaining Python, the PE
  reader, xmlinst and navtune (F2b), and the remaining format readers and writers (F3).
- Streamed bundle packing, process-wide BF1943 restart points and the shader coverage index (round S1).

### Removed
- **DbxEdit.exe and FbrbTool.exe** as tools: their commands run in process on the Rust core
  (`bfbc2core.dbxedit`, `bfbc2core.fbrbtool`, `bfbc2.exe dbxedit / fbrbtool`), checked against the exes' recorded
  answers (round R4).
- The ported Python of NavKit (R1), the converters (R2), the shader translator's HLSL emission (R3) and the core
  format readers and writers (F1a): the Rust core is required for them.
- NavKit's research switches (BRIDGE_UNION and the others): the chosen behaviour is the only one.

### Fixed
- dbxport's Rust port read its data tables built in instead of as OPFOR changes them (round E4d's fix).

## [0.7.0] - 2026-11-17

### Added
- **The Rust core** (`core\bfbc2core\bc2_native.pyd`, `rust\`), with a prebuilt module from GitHub Releases
  (`python rust\build_native.py --download`) and a source fingerprint so a stale module is never used silently.
  In Rust: the DBX and FBRB readers (E0); the level loader, terrain, water and collision, the writers (zlib, DBX),
  shader databases and cull trees, texture conversion and NavKit's grid walks, the mesh alias table (E1a-e);
  NavKit's walkability rules and the rest of its generator (E2a, E3c, E3h); the EA-XAS audio encoder (E3a); DbxEdit's
  and FbrbTool's commands in process (E3b, E3k); the bundle cache (E3e); the Xbox 360 shader translator (E3f); the
  BF1943 model, physics and effect ports (E3g); Granny animations (E3i); the stock-MP AI layer's unpacking (E3j);
  the APT decoder (E3n); navdata, kymesh, navpath, havok.verify and texqa (E3o).
- HLSL compiles cached and on all cores; dbxport's template ranking from an index (E3d).
- `run_tests.py` runs tests side by side (`-j N`), each declaring its CPU, memory and shared resources; `--plan`
  prints the schedule.
- Cache housekeeping (`python -m bfbc2core.caches`): one size budget, least recently used first (E3p).
- Logs capped by size and day, archived before they are pruned (E3q).

### Changed
- Large speed-ups across the suite, each byte-identical to the Python it replaced (`rust/BENCHMARKS.md`).
- Whole-file writes wait out a moment's lock on the target (Windows).

### Removed
- The Python of texture conversion and NavKit's grid walks (round E1d's retirement): the Rust core is required.

## [0.6.0] - 2026-11-15

### Added
- Xbox 360 shaders on PC: per-texture sampler routing, terrain strips and terrain patches with the engine's named
  constants.
- Sub-terrain heightfields (the ring around the main block).
- Console texture format 9 (big-endian A8R8G8B8) converted to PC.
- `skinmesh`: a new part in a given vertex layout.

### Changed
- The quick tier builds DbxEdit, FbrbTool and LevelDiff only when a source is newer than the .exe.

### Fixed
- The Xenos translator: MAXA / MAXAS set the address register after the instruction.
- effectport passes its ported partitions' guids (dbxport had nulled valid references); NavKit and stock-MP AI QA
  fixes (2026-11-14).
- The deep scan (2026-11-15): the stock-MP install applies as one transaction, installs and writes are atomic,
  caches are honest, damaged files are reported.

## [0.5.0] - 2026-11-10

### Added
- The BF1943 importer's ground truth and checks: `dbxcatalogue` (BFBC2's own partitions as dbxport's reference),
  the dbxport rewrite with ranked templates, `dbxparity`, `portparity`, `effectport`, `emitteratlas`.
- `mopp.py`: Havok's MOPP byte code decoded.
- `aimanim.py`: generated aim poses read and written.
- `texqa`: offline QA of converted textures; `renderqa`: an offline Direct3D 11 render harness; `xenos_exec` and
  `xdiff`: a Xenos pixel-shader interpreter and the translator's differential test.
- Console texture format 10 (L8), DXT1 with 1-bit alpha in `texwrite`, mesh relayout, mesh rename in meshport,
  physics material slots in havokport, third-person undeploy tracks (`granny.add_tracks`).
- Lint and types: ruff and mypy on every folder in the quick tier (`pyproject.toml`).
- `suite_paths.py`, the one `sys.path` bootstrap, and `bfbc2core.paths`, the one path finder.

### Fixed
- xiso extract stopped looping on a truncated image; stfs follows the live copy of each hash table.
- aptcheck verify takes `--brief`; hpdtool roundtrip counts a truncated file as one failure.
- culltree: a tree reference without a `levels/` segment, and verify / add reading the target as its own reference.
- The test harness logs a file on another drive; the crash reader's code view decodes from the function start.
- NavKit: the detailed-sector cache keyed by sector offsets, apply keeps its report on any failure, the grid
  settings restored after a failed detailed pass.
- The mesh viewer rebuilds a corrupt index cache.

## [0.4.0] - 2026-11-07

### Added
- Console conversion, Xbox 360 to PC: terrain (heightfield, water mesh, material map, visual terrain and water),
  soot meshes, Frostbite Wave sounds (EA-XMA to EA-XAS), textures (DXT5A, RGB565, RGBA16F).
- `shaderdb`: Xbox 360 shader databases read byte for byte.
- Model tools: `texwrite` (PC textures from pictures, DXT1 / DXT5) and `decimate` (quadric edge collapses).
- Menu tools: `aptgraft` frame-label aliases and HUD icons of their own.
- The Xbox 360 shader translator: `xenos.py` (microcode decoder) and `xenos_hlsl.py` (HLSL for Dx10/11), with
  `shaderview` for drawing.
- dbxport: renamed fields, pruning and required instances; havokport's velocity limits from the game's table.

### Fixed
- BF1943 import fixes found by Wake Island's first loads; shape subparts in Havok 5.5.0's placement order.

## [0.3.0] - 2026-10-31

### Added
- The console converter, the shader, cull-tree, mesh viewer, model and animation tools.
- Granny resources read big-endian (Xbox 360) and ported to PC.
- The BF1943 import pipeline: models (`meshport`), rigid mesh sets and vehicles, physics from Havok 4.6.1 to 5.5.0
  (`havokport`), the DBX writer and partition port (`dbxport`).

### Changed
- The project the suite serves is now Project OPFOR BFBC2; the repositories moved to `C:\Programming\Projects`.

## [0.2.0] - 2026-10-26

### Added
- NavKit closer to DICE's path data: area link rule, symmetric portals, the reference extent, headroom band, route
  nodes at AI order positions, detailed zones, ladder links, narrow lips and door sills, exact bridge tracing,
  objective floors, D lines and F cells, vehicle path data, boat meshes, routes across domains, route-island
  pruning, the offline sampler.
- `navparity`, `ordercheck`, `navcompare`, `navdiff` and `sample_all` for measuring builds against DICE's data.
- Navigation Studio overlays: the reference AI layer and anomalies.
- `apt.py`: an APT menu reader for PC and Xbox 360.
- `xmlinst`: level XML instances whatever the attribute order.

## [0.1.0] - 2026-10-16

### Added
- The suite as one repository (BFBC2 Tool Suite, MIT licence): NavKit (path data decoded byte for byte, the
  generator, the driver and its window), DbxEdit and DbxEditGUI, FbrbTool, LevelDiff, the crash reader, the console
  readers (STFS, XISO).
- The shared core: the DBX reader, the level loader, terrain and water, a 3D view with GPU picking.
- The Map Inspector; the Navigation Studio with the path test, walkability edits, links and route edges,
  traversals, the apply report and installing.
- The suite window (hub) with level pickers; error handling and logs for every tool; the test runner's quick and
  game tiers.

## The back-filled versions' last commits

The last commit on `main` of each back-filled version, should they ever be tagged:

| Version | Commit | Date |
|---|---|---|
| 0.8.0 | `c448e14` | 2026-11-18 |
| 0.7.0 | `ee25186` | 2026-11-17 |
| 0.6.0 | `b94ff47` | 2026-11-15 |
| 0.5.0 | `eefeacb` | 2026-11-10 |
| 0.4.0 | `6e01543` | 2026-11-07 |
| 0.3.0 | `5bd9c47` | 2026-10-31 |
| 0.2.0 | `a376e7e` | 2026-10-26 |
| 0.1.0 | `d971a9b` | 2026-10-16 |
