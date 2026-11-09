# Architecture audit, 2026-11-09

Read-only audit of the suite at `main` a482659 (just after quality round 3) and of its consumer, Project OPFOR BFBC2's
`src\tools\ww2`. Nothing in either repository was changed apart from this file. The game copy was not touched, and the
game was not started.

**Method.**
- **Import graph:** static analysis with `ast` of all 184 suite `.py` / `.pyw` files and the 45 OPFOR ww2 scripts.
  Each import is resolved the way the code resolves it: the file's own folder, then the folders it names in
  `suite_paths.use(...)`. Lazy imports inside functions are included and marked as lazy. An import that resolves only
  because some *other* module put its folder on `sys.path` is reported as an implicit edge (section 1.3).
- **Complexity:** `ruff check --select C90,PLR0912,PLR0915,PLR0913` with default thresholds (C901 > 10, branches > 12,
  statements > 50, arguments > 5), run from the command line. The configuration was not changed.
- **The rest:** AST scans for private access across modules, mutated module-level containers, `global` statements,
  attribute assignment onto imported modules, and functions that mix I/O with logic.
- **Scripts:** in the session scratchpad (`...\scratchpad\arch`): `graph.py`, `analyze.py`, `ww2edges.py`,
  `privates.py`, `ruffagg.py`, `sizes.py`, `iomix2.py`, `layers.py`.
- **Paths:** paths are relative to the suite root unless they start with `ww2/` (OPFOR's
  `src\tools\ww2`). "Non-test" leaves out `tests/` folders and `test_*.py` files.

---

## 1. Import graph

### 1.1 Cross-folder edges (non-test)

`bfbc2-auto-navkit/tools` is counted separately from `bfbc2-auto-navkit` (navkit.py, navkitGUI.pyw), because the
edges between them matter. "Sites" counts import statements; nearly all edges to `core` are `errors` imports in `main`.

| From | To | Sites | What is imported (file:line) |
|---|---|---|---|
| `<root>` (hub, run_tests) | core | 2 | errors (`BFBC2 Tool Suite.pyw:25`, `run_tests.py:207`) |
| `<root>` | navkit | 2 | navkit (`BFBC2 Tool Suite.pyw:77`, `run_tests.py:37`), lazy |
| **core** | **navkit** | **3** | **`core/bfbc2core/level.py:166,403`, `pick.py:19` → navkit.py (`find_opfor`, `Project`, `prepare`, `find_tool`)** |
| **core** | **navkit/tools** | **6** | **`level.py:165` levelobjects, `level.py:435-436` navsample / navwalk; `navdata.py:36-38` hpdtool / kymesh / navdiff (top level)** |
| navkit | navkit/tools | 19 | navbuild, levelobjects, ladders, navsample, detailed, hpdtool, kymesh, navregress, navcompare, navlinks, navtraversal (all lazy, navkit.py:52-843) |
| navkit | core | 3 | navtune (`navkit.py:747`), errors |
| **navkit/tools** | **navkit** | **10** | **`navparity.py:52`, `navsample_offline.py:1086,1108,1120,1192,1340,1351,1387,1424`, `sample_all.py:22` → navkit.py** |
| navkit/tools | core | 41 | errors ×18; navdata, level, terrain, water, xmlinst, dbx |
| harness | navkit | 1 | `harness.py:56` (`navkit.find_opfor`) |
| harness | navkit/tools | 11 | navparity, navregress, hpdcheck, hpdtool, ordercheck |
| harness | core | 17 | navdata, navpath, dbx, fbrb, level, errors |
| **anim-tools** | **mesh-viewer, shader-tools** | **2** | **`granny.py:613-614` meshview.Index, shaderdb.find_opfor (in `granny.load`)** |
| bf1943-import | anim-tools | 3 | aimanim, granny (`bf1943import.py:49-50`, `meshport.py:35`) |
| bf1943-import | console-convert | 9 | texture, havok, mesh, meshset |
| **bf1943-import** | **mesh-viewer** | **4** | **meshview.Index (`bf1943import.py:150,284`, `dbxparity.py:145`, `portparity.py:217`)** |
| bf1943-import | shader-tools | 6 | `shaderdb.find_opfor` ×6 (`bf1943import.py:151,216,285,330`, `dbxparity.py:146`, `portparity.py:218`) |
| bf1943-import | core | 5 | dbx ×4, errors |
| console-convert, console-readers, culltree | core | 1, 3, 1 | errors |
| map-inspector | core | 5 | dbx, viewer, level, errors, pick |
| menu-tools | console-readers | 2 | apt (`aptgraft.py:141`, `menucommon.py:27`) |
| menu-tools | core | 11 | fbrb ×7, errors ×4 |
| mesh-viewer | console-convert, shader-tools, core | 2, 1, 2 | mesh, texture; shaderdb; dbx, fbrb |
| model-tools | console-convert | 1 | mesh (`skinmesh.py:22`) |
| navigation-studio | core | 10 | navtune, navdata, xmlinst, level, navpath, viewer, errors, pick |
| navigation-studio | navkit/tools | 4 | navparity (`overlay.py:59,294`, `studio.py:68,724`) |
| shader-tools | core | 5 | dbx, fbrb, errors |
| **stockmp-ai** | **navkit** | **1** | **`pipeline.py:77` (`navkit.find_opfor`)** |
| stockmp-ai | navkit/tools | 2 | ordercheck (`config.py:139`), hpdtool (`pipeline.py:188`) |
| stockmp-ai | core | 2 | dbx (`validate.py:43`), errors |

Totals: 34 folder pairs, 184 suite files, 174 `suite_paths.use` calls. With the tests included there are 54 pairs.

### 1.2 Cycles

- **Folder level (non-test): one cycle, `core` ↔ `navkit` ↔ `navkit/tools`.**
  - core reaches NavKit's entry script and its tools (rows in bold above).
  - NavKit's tools reach `navkit.py`.
  - Both reach core.
- **Folder level, with tests:** a second cycle appears: `shader-tools → console-convert → bf1943-import → mesh-viewer →
  shader-tools`, plus `anim-tools`. It exists only through test files: console-convert's tests read BF1943 data
  through bf1943-import. It is harmless at run time, but it shows that the format folders' tests depend on the porter
  folder.
- **Module level (non-test): one strongly connected component of five modules:**
  - `core/bfbc2core/level.py:166,403 → navkit.py`
  - `navkit.py:255,578 → tools/navregress.py`
  - `navregress.py:43 → navparity.py` (top level)
  - `navparity.py:38 → navdf.py` (top level), and `navdf.py:220 → navparity.py` (lazy)
  - `navparity.py:52 → navkit.py` (lazy)
  - `navregress.py:86 → core/bfbc2core/level.py` (lazy)

  Every back edge is a lazy import inside a function. That is why it works today, and why import order never shows the
  cycle.

### 1.3 Implicit sys.path edges (hidden import-order dependencies)

These lazy imports resolve only because an *earlier* import put the folder on `sys.path`. QUALITY_AUDIT round 3 checked
that "every module imports with its own folder", but only at import time. These edges fail when the function is called
in a process where the other module was never imported. Checked with `python -I` and only the module's own folder on
the path:

| Module:line | Import | Works only because | Isolated check |
|---|---|---|---|
| `granny.py:614` | `shaderdb` | `meshview` (imported one line earlier) calls `suite_paths.use('shader-tools')` | `import granny` ok, `shaderdb` not resolvable |
| `testharness/checks.py:99,126,141,279`, `classify.py:48` | `bfbc2core` | harness.py's `suite_paths.use(..., 'core', ...)` | `bfbc2core` not resolvable |
| `testharness/runs.py:40` | `navregress` | harness.py's `use('navkit-tools')` | `navregress` not resolvable |
| `studio.py:68,724` | `navparity` | `bfbc2core.navdata` (imported by studio) calls `use('navkit-tools')` | resolvable, transitively |
| `stockmp-ai/tests/test_stockmp.py:377,393`, `bf1943-import/tests/test_havokport.py:49`, 4 more tests | hpdtool, navkit, hkpack, shaderdb | earlier imports | tests only |

### 1.4 Fan-in and fan-out (non-test, distinct modules)

| Fan-in | Module | | Fan-out | Module |
|---|---|---|---|---|
| 39 | core/bfbc2core/errors.py | | 14 | harness.py |
| 20 | navkit/tools/hpdtool.py | | 13 | navkit.py |
| 13 | navkit/tools/kymesh.py | | 11 | navkit/tools/navsample_offline.py |
| 12 | navkit/tools/navparity.py | | 10 | navkit/tools/navparity.py |
| 10 | navkit.py | | 10 | navkit/tools/navvehicle.py |
| 10 | core/bfbc2core/dbx.py | | 10 | navigation-studio/studio.py |
| 9 | navkit/tools/navsample.py | | 9 | navkit/tools/navregress.py |
| 8 | core/bfbc2core/level.py | | 9 | navkit/tools/navwater_mesh.py |
| 7 | navkit/tools/levelobjects.py | | 8 | navkit/tools/navbuild.py |
| 7 | core/bfbc2core/fbrb.py | | 7 | bf1943-import/portparity.py |
| 6 | core/bfbc2core/xmlinst.py | | 6 | navkit/tools/navwater.py |
| 6 | core/bfbc2core/navdata.py | | 6 | testharness/levelinfo.py |
| 6 | navkit/tools/navdiff.py | | 6 | bf1943-import/bf1943import.py |
| 6 | console-convert/texture.py | | 6 | stockmp-ai/pipeline.py |
| 5 | shader-tools/shaderdb.py | | 6 | stockmp-ai/stockmp.py |

- **navparity:** an analysis CLI with fan-in 12. It is imported mostly for `opfor_root` (6 uses), `reference_of` (3),
  `level_xml` (3), `need_reference` (3) and `terrain_bounds`. These are level-input helpers, not parity code.
- **navkit.py:** fan-in 10, mostly for `Project` (10 uses), `ensure_settings` (8) and `find_opfor` (3).
- **Counted from OPFOR,** the suite's most depended-on modules are `meshview` (30 of 45 ww2 files) and `shaderdb` (29),
  in section 1.5.

### 1.5 OPFOR ww2 → suite edges

**How OPFOR reaches the suite.**
- `ww2/attachments.py:47-50` computes `SUITE = $OPFOR_TOOLKIT or <OPFOR's parent>\bfbc2-tool-suite`.
- It prepends `core`, `bfbc2-mesh-viewer`, `bfbc2-model-tools` and `bfbc2-console-convert` to `sys.path` and imports
  `meshset`, `meshview`, `shaderdb`, `skinmesh`, `dbx` and `shapes`.
- **`shaderdb` is never put on the path by OPFOR.** It resolves because `meshview.py:37` calls
  `suite_paths.use('core', 'shader-tools', 'console-convert')`. That is an implicit edge on the consumer side.
- **attachments.py works as OPFOR's bootstrap and re-export hub.** 30 ww2 files do
  `from attachments import meshview, shaderdb, dbx, ...`. Others reach `at.skinmesh.*` (10 + 5 + 4×4 uses),
  `at.meshset.*` (16) and `at.dbx.to_xml` (15). There is also `wl.dbxport.*` / `wl.bf1943import.*` through wake_level
  (ww2/wake_ambience.py), and `dbxport.dbx` (core's dbx through dbxport's namespace) in ww2/bf1943_vehicles.py.
- **sys.path in OPFOR:** 91 `sys.path` lines in 42 of the 45 files. They name the folders `bfbc2-console-convert`
  (16 files), `bfbc2-model-tools` (10), `bfbc2-bf1943-import` (8), `bfbc2-anim-tools` (6), `bfbc2-menu-tools` (6),
  `bfbc2-shader-tools` (2), `core` (2) and `bfbc2-mesh-viewer` (1).
- **Executables and scripts run as processes:**
  - `bfbc2-dbx-tools\DbxEdit.exe` (`attachments.py:58`);
  - `bfbc2-menu-tools\aptcheck.py`, run as a subprocess in ww2/duplicates.py:114, ww2/menu_install.py:514,
    ww2/type95_icon.py:197, ww2/wake_ui.py:71 and ww2/weapon_icons.py:328;
  - `bfbc2-shader-tools\native` (ww2/wake_preview.py).

**Suite modules, the names OPFOR uses, and how many ww2 files use each** (re-exports through attachments are
resolved; privates in bold):

| Suite module | Folder | Files | Names used (files) |
|---|---|---|---|
| meshview | mesh-viewer | 30 | `Index` 29, `render` 11, `load_mesh` 10, **`_layout` 10**, `page` 2, **`_surfaces` 1** |
| shaderdb | shader-tools | 29 | `find_opfor` 25, `Merger` 8, `ShaderDb` 8, `load` 8, `block_constants` 4, `entry_textures` 4, `level_bundles` 4, `ShaderDbError` 3, `split_name` 3, `texture_kind` 3, `coverage` 2, `merge` 2, `block_textures`, `level_meshes`, `needed`, `read_need`, `Shader` |
| dbx (core) | core | 20 | `to_xml` 19, `parse` 4, `write` 4, `value` 3, `TEXT` 2, `floats` 2, `Element` 1, **`_put_varint` 1** |
| texture | console-convert | 15 | `decode` 12, `convert` 5, `levels` 5, `HEADER`, `ConvertError` |
| bf1943import | bf1943-import | 13 | `Source` 13, `default_source` 13, `Importer` 3 |
| granny | anim-tools | 9 | `Granny` 8, `animations` 7, `curve_keys` 6, `sample` 3, `shift` 2, `set_orientations`, `to_pc` |
| texwrite | model-tools | 9 | `texture` 8, `HEADER` 4, `alpha5` 3, `wrapped` |
| mesh | console-convert | 8 | `parse` 6, `elements` 4, `console_layouts` 3, `decode` 2, `element`, `PC_FORMAT`, `SIZE`, **`_pair_by_kind` 1** |
| dbxport | bf1943-import | 7 | `Templates` 7, `port` 7, `REQUIRED`, `NOT_IN_BFBC2`, `dbx` (re-export) |
| meshset | console-convert | 6 | `parse` 6, `KINDS` 3, `write` 2 |
| aptgraft | menu-tools | 5 | `Movie` 5, `GRAFT_LAYOUT` 5, `add_label_alias` 4, `Ref` 3, `Str` 3, `AptError`, `Words`, `text`; ww2/menu_install.py alone uses the assembler internals `Code`, `Ins`, `BRANCH`, `FN_MAGIC`, `SIG`, `code_nodes`, `walk_ins`, `dump_text`, `load_members`, `f32`, `signed` |
| fbrb (core) | core | 5 | `read` 5, `entries` 3 |
| meshport | bf1943-import | 4 | `Porter` 4 |
| havokport / havok | bf1943-import / console-convert | 3 / 3 | `convert`, `ufloat8_table`, `PortError` / `classes` |
| shapes | model-tools | 3 | `extrude`, `lathe`, `Geo`, `fix_winding`, `rot_x/y/z` |
| pcbuild | shader-tools | 3 | `block_texture_list` 3, and 12 single uses in ww2/wake_translate.py (`compile_hlsl`, `consts_dxbc`, `dx11_layout`, `program_block`, ...) |
| skinmesh | model-tools | 2 | `SkinnedLod`, `meshdata`, `lod_size`, `INDICES`, `WEIGHTS`, `Part`, `EditError` |
| effectport | bf1943-import | 2 | `port` (ww2/bf1943_vehicles.py), `port_many`, `sounds_of` (ww2/material_grid.py) |
| terrain | console-convert (and core) | 2 | console-convert: `heightfield`, `material_map`, `visual_terrain(_parts)`, `visual_water(_parts)`, `water_mesh`, `ROAD_VERTEX`, **`_Walk`** (ww2/wake_install.py:1734); core: `read` (ww2/wake_ambience.py:117) |
| audio | console-convert | 2 | `convert`, `AudioError`, `read_wave` |
| bc2loc | menu-tools | 2 | `LocFile`, `khash`, `languages`, `load` |
| decimate, xenos_hlsl, xenos, soot, aimanim | various | 1-2 | `decimate`; `translate`, `Interface`, `PROBE_*`; `parse_shader`; `soot_mesh`; `parse` |

**Privates OPFOR depends on (14 sites):**
- `meshview._layout` 10× (attachments.py:535, bf1943_jp_bazooka.py:65, rg_projectiles.py:241, rifle_grenades.py:185,202,
  scopes.py:224, shingunto.py:255,258, type100.py:304-305, type38.py:623,630, winchester.py:219, winchester1897.py:395);
- `meshview._surfaces` (menu_renders.py:101);
- `dbx._put_varint` (wake_level.py:76-77);
- `mesh._pair_by_kind` (wake_translate.py:153);
- `terrain._Walk` (wake_install.py:1734).

**The weakest contracts:**
- `meshview._layout` is used 10 times: it should become public.
- aptgraft's assembler internals are used by ww2/menu_install.py.
- `shaderdb.find_opfor` is used 25 times as OPFOR's own game-copy locator.

---

## 2. Size and complexity

### 2.1 Largest modules (non-test)

| Lines | Functions | Classes | Module | Concerns in one file |
|---|---|---|---|---|
| **2,416** | 74 | 0 | **ww2/wake_install.py (OPFOR)** | the largest module in either repository: closure, shader DBs, layout fixes, REMAP, terrain patches, install / revert |
| 2,069 | 133 | 11 | menu-tools/aptgraft.py | node model (165), assembler (271), BIGF (372), Movie (411-1101), Grafter/Builder (1102-1337), **Onslaught timer recipe (1338-1816)**, dependencies, checks, CLI |
| 1,856 | 61 | 2 | navkit/tools/navbuild.py | settings, Level, stage, graph build, detours repair, file write |
| 1,497 | 54 | 3 | navkit/tools/navsample_offline.py | 11 sections: sampler settings, Havok assets, grid, rasteriser, terrain, cast loop, capsule test, run, comparison, level driver, stock MP |
| 1,330 | 71 | 6 | shader-tools/shaderdb.py | file model, reader, names, mesh use, Merger, parameter blocks, CLI, **plus `find_opfor` / `level_bundles` (game-copy discovery)** |
| 1,211 | 80 | 10 | culltree/culltree.py | **its own binary DBX codec (70-270)**, the tree, levels, templates, add, checks, CLI |
| 995 | 46 | 4 | navkit/tools/navwater_mesh.py | |
| 915 | 35 | 3 | shader-tools/xenos.py | |
| 901 | 51 | 2 | navkit/navkit.py | CLI **and** the library everyone imports (`Project`, `find_opfor`, `find_tool`, `prepare`, `ensure_settings`) |
| 894 | 25 | 0 | navkit/tools/navwalk.py | one function is a third of it (`walkable`, 285 lines) |
| 892 | 35 | 0 | navkit/tools/navvehicle.py | |
| 792 | 34 | 1 | navigation-studio/studio.py | |
| 767 | 36 | 2 | anim-tools/granny.py | format + `load()` (game-copy lookup through meshview) |

**God modules beyond the known two:**
- **navsample_offline (1,497):** a sampler, a geometry loader, a comparison tool and a stock-MP driver in one file.
- **shaderdb (1,330):** a format library that also owns game-copy discovery.
- **culltree (1,211):** owns a third DBX codec.
- **navkit.py (901):** a CLI used as a library.
- **ww2/wake_install.py (2,416):** on the consumer side, bigger than aptgraft.

### 2.2 Largest functions (non-test)

| Lines | Function |
|---|---|
| 285 | `navkit/tools/navwalk.py:450 walkable` |
| 271 | `navkit/tools/navbuild.py:891 stage` |
| 166 | `navbuild.py:1375 build_graph` |
| 161 | `bf1943-import/havokport.py:127 restructure` |
| 137 | `model-tools/decimate.py:33 decimate` |
| 128 | `navigation-studio/editing.py:255 panel` |
| 121 / 117 | `aptgraft.py:1942 main` / `aptgraft.py:1698 onslaught_timer` |
| 117 | `console-convert/mesh.py:232 convert` |
| 109 | `navbuild.py:1672 repair_detours` |
| 108 | `core/bfbc2core/navdata.py:187 _load` |
| 107 | `harness.py:142 run_level` |
| 102 | `core/bfbc2core/level.py:161 load` |

OPFOR's largest: `ww2/wake_translate.py:317 translate_db` (256 lines, with a nested `entry` of 203),
`ww2/wake_shaders.py:505 clone` (176), `ww2/material_grid.py:140 main` (174), `ww2/wake_install.py:871 closure` (166).

### 2.3 ruff counts (default thresholds; the project does not select these rules)

| Rule | Non-test | Tests | Worst (value) |
|---|---|---|---|
| C901 complex-structure | 154 | 23 | `navwalk.walkable` 81, `navbuild.build_graph` 49, `navbuild.stage` 46, `havokport.restructure` 42, `editing.panel` 38, `decimate` 36, `repair_detours` 35, `aptcheck.verify` 33, `xenos_hlsl.scalar` 31 |
| PLR0912 too-many-branches | 91 | 10 | `walkable` 70, `stage` 48, `build_graph` 44, `restructure` 38, `panel` 38 |
| PLR0915 too-many-statements | 65 | 37 | `walkable` 206, `stage` 175, `restructure` 125, `build_graph` 119, `panel` 117, `decimate` 102, `aptgraft.main` 100 |
| PLR0913 too-many-arguments | 82 | 10 | **`navbuild.stage` 28**, `navwalk.walkable` 24, `navbuild.py:852` 23, `build_graph` 13, `navbuild.py:1638` 12 |
| PLR0911 too-many-returns | 16 | - | |

- **By folder (non-test findings):** navkit/tools 148, menu-tools 48, shader-tools 37, harness 22, stockmp 20,
  bf1943-import 19, core 19, console-convert 17, navigation-studio 13, others ≤ 10.
- **By module:** navbuild 38, aptgraft 25, navsample_offline 20, xenos_hlsl 15, navwalk 14, navvehicle 13.
- **`stage` and `walkable` take 28 and 24 arguments,** even though `BuildSettings` exists. Most of those arguments are
  per-call inputs and flags. A `StageInputs` value would take PLR0913 down where it is worst.

---

## 3. Boundary violations

### 3.1 Private access across modules (non-test)

| Site | Private | Note |
|---|---|---|
| `BFBC2 Tool Suite.pyw:110,234`, `navkit/navkitGUI.pyw:236` | `errors._state['path']` | The log path. Needs a public `errors.log_path()`. |
| `testharness/checks.py:100` | `navpath._adjacency` | The route graph's adjacency. Make it public. |
| `stockmp-ai/author.py:242` (×2) | `dbxbin._varint_bytes` | Varint writer (see 4.1). |
| `stockmp-ai/stockmp.py:65` | `pipeline._level_name` | |
| OPFOR, 14 sites | `meshview._layout` ×10, `_surfaces`, `dbx._put_varint`, `mesh._pair_by_kind`, `terrain._Walk` | Section 1.5. |
| OPFOR-internal | `menu_art._blur` (menu_layers.py:26), `at._quat` (menu_renders.py:36), `wake_level._numbers` (wake_install.py:721), `wake_level._files` (wake_shaders.py:42), `gltf._read_glb` (type38.py:312,321, winchester1897.py:156) | Within OPFOR. |

### 3.2 Module-level mutable state mutated at run time

| Where | State | Mutated at |
|---|---|---|
| `navkit/tools/navbuild.py:138,139,153,188` | `ORDER_STATS`, `ORDER_IDS`, `LADDER_STATS`, `DETAIL_STATS` | 1538/1633, 1662, 1152-1153, 768-776 (cleared by `reset_stats`) |
| `navkit/tools/navwalk.py:87,102,104` | `ROOF_STATS`, `OBJECTIVE_STATS`, `STEEP_STATS` | 790-825, 445-446, 498-518 |
| `navkit/tools/navislands.py:40,41` | `STATS`, `TERRAIN_IDS` | 141/146, 52 |
| `core/bfbc2core/errors.py:26` | `_state` (tool, log path, gui) | 79, 82 in `setup`; read by 3 GUIs (3.1) |
| `core/bfbc2core/errors.py:95-96` `setup` | replaces `sys.excepthook` and `threading.excepthook`, sets `warnings.simplefilter('default')` | process-global hooks, by design (one tool per process) |
| `shader-tools/pcbuild.py:46` | `global _d3d` (the d3dcompiler handle, loaded lazily) | the only `global` statement in the suite |
| `core/bfbc2core/level.py:397-427` `heightfield_of` | an unpacked terrain folder reused forever: no `.source.sha256` stamp, unlike `navkit.Project.unpack` (`navkit.py:184-194`) and `stockmp pipeline.unpacked` (`pipeline.py:110-120`) | stale after a game patch |
| `bf1943-import/dbxport.py:37-54` `Templates` | first instance of a type wins; folders are read with `glob.glob(..., recursive=True)` (line 42) in **file-system order, unsorted** | template choice depends on file order |
| `meshview.Index` | pickled cache `work\tools_out\meshview_index.pkl`, keyed by bundle size and mtime | fine; noted because 30 OPFOR files share it |

OPFOR has 11 mutated module-level containers in ww2/wake_install.py (`_masked_names` 108, `TERRAIN_NAMES` 550, `_dbs`
1050, `_ix` 1421, `_layout_fix` 1535, `COMMON_BUILT` 2184), ww2/wake_shaders.py `_BAKED` 341, ww2/wake_translate.py
`_fmt_cache` 68, ww2/weapon_icons.py `MULTI` 206 and ww2/grime_models.py `BEFORE` 21, plus `global VIETNAM_VO`
(wake_install.py:2401) and `global ORIGINAL` (winchester.py:177).

### 3.3 Monkeypatching

- **Suite, non-test:** no attribute assignment onto imported modules. The only process-global patching is
  `errors.setup`'s exception hooks (3.2).
- **Suite tests:** 38 assignments onto imported modules in 13 test files. The largest are
  `tests/test_review_fixes2.py` (7), `navkit/tests/test_navislands.py` (6), `test_characters.py` (4),
  `test_detail_features.py` (4) and `test_tool_edges.py` (3). This is acceptable for tests, but it shows where an
  injectable parameter is missing.
- **OPFOR:**
  - **`weapons.OUT = ...` in 7 modules** redirects where `weapons.edit` writes: ww2/bf1943_vehicles.py:220,
    bf1943_weapons.py:264, melee.py:158, rg_projectiles.py:153, scopes.py:375, type38.py:588, wake_level.py:465.
  - `pictures.shingunto_repainted = ...` (ww2/grime_models.py:49).
  - An `out` parameter on `weapons.edit` would remove all eight.

### 3.4 Other boundary problems

- **Generic top-level module names in one flat namespace:** `config`, `convert`, `audio`, `mesh`, `texture`,
  `terrain` (twice: console-convert and `bfbc2core.terrain`), `havok`, `pipeline`, `editing`, `overlay`, `shapes`.
  `suite_paths.use` moves suite folders to the *front* of `sys.path`, ahead of OPFOR's own folder that ww2 scripts
  inserted first. There is no collision with ww2 today (checked: no shared names), but a future
  `ww2/texture.py` or `config.py` would be silently shadowed.
- **`stockmp-ai/dbxbin.py` docstring (lines 3-4)** says core's reader "is read-only". `core/bfbc2core/dbx.py:176` has
  had `write()` since then, so the doc is stale.

---

## 4. Duplicate implementations

### 4.1 Binary DBX codecs: three, plus three varint pairs

| Copy | Lines | Model | Writes | Used by |
|---|---|---|---|---|
| `core/bfbc2core/dbx.py` | 368 | strings resolved; `Element(tag, attributes, children, kind, raw)`; `parse/write/to_xml/value/floats/numbers` | yes, re-interned string table (`write`:176) | 10 suite modules and 20 OPFOR files; verified against DbxEdit on 31,404 files |
| `stockmp-ai/dbxbin.py` | 343 | string *indexes* kept; byte-exact round trip; typed setters `set_text/set_bool/set_numbers/set_float/set_transform/set_ref/clone`; `new_guid` | yes, byte for byte | stockmp only (author, levelmodel, pipeline, stockmp, validate) |
| `culltree/culltree.py:70-270` (`El`, `Doc`) | ~200 | keeps every byte; arrays editable | yes | culltree only |

- **Varint helpers:**
  - `_varint`: `dbx.py:76`, `dbxbin.py:211`, `culltree.py:74` (identical bodies).
  - `_put_varint`: `dbx.py:165 (out, v)`, `culltree.py:85 (v, out)` (reversed arguments), `dbxbin.py:222 _varint_bytes(v)`.
- **Which should win:**
  - **core dbx** is the one reader: it has the corpus test and the most users.
  - **dbxbin's byte-exact, index-keeping mode and typed setters** should become core's editing layer, e.g.
    `bfbc2core.dbxedit`, built on core's `_varint`. Its `set_numbers` is what OPFOR re-implements in
    `ww2/wake_level.py:73 _numbers` with the private `dbx._put_varint`.
  - **culltree's `El`/`Doc`** should be replaced by that editing layer once byte-exact round trip is proven on the
    cull-tree files.
- **Order:** do this late. The converter baseline (dbxparity / portparity JSON byte-identical) and a stock-MP Atacama
  build comparison guard it.

### 4.2 GUID schemes (uuid5 over `NAMESPACE_URL`, upper case)

- **Suite:**
  - `dbxbin.new_guid(namespace, *parts)` (`dbxbin.py:341`, joins with `:`). stockmp uses
    `opfor:stockmp:<lv>:<role>:<id>` (`pipeline.py:353`).
  - shaderdb uses md5 GUIDs (`shaderdb.py:167` `name_guid`, `:812`). That is a different scheme, for shader
    entries, and should stay.
- **OPFOR, 10 hand-written copies of the same pattern:**
  - `attachments.py:366 guid(name, old)`: `opfor-ww2:{name}:{OLD}`;
  - `material_grid.py:90`: `opfor-ww2:material-grid:{tag}`;
  - `wake_ambience.py:80`: `opfor-ww2:wake-ambience:{tag}`;
  - `type95_icon.py:156`, `menu_install.py:484` (`opfor/main_nav/...`: slash separator, a different prefix style);
  - `wake_install.py:857` (`'opfor:' + new.lower()`), `wake_install.py:1801`;
  - `wake_level.py:172,188`.
- **Which should win:** `dbxbin.new_guid` moved to core (`bfbc2core.dbx.new_guid(namespace, *parts)`).
- **OPFOR must keep each existing namespace string verbatim.** The GUIDs are baked into installed data, and changing a
  separator changes every GUID. A shared helper is safe only as `new_guid_raw(text)` taking the exact current string.

### 4.3 REF regexes over `to_xml` text

| Site | Pattern | Differences |
|---|---|---|
| `bf1943-import/effectport.py:40` | `ref="([^"]+)/[0-9a-fA-F-]{36}"` | any character |
| `mesh-viewer/meshview.py:150` | `<field name="Name">…<field ref="([^"]+)/…" name="Shader"` | attribute order assumed (xmlinst.py's docstring warns about this) |
| `ww2/bf1943_objects.py:72` | `ref="([a-z0-9_/.\-]+)/[0-9a-f-]{36}"` | **lower case only, no `re.I`, no space:** misses mixed-case references |
| `ww2/wake_install.py:134` | `ref="([A-Za-z0-9_/.!\- ]+)/[0-9A-Fa-f-]{36}"` | allows `!` and space |
| `ww2/wake_level.py:480`, `ww2/wake_shaders.py:30-31` | `ref="([a-z0-9_/.\- ]+)/[0-9a-f-]{36}"`, `re.I` | no `!` |
| `ww2/material_grid.py:150,168,169`, `ww2/wake_ambience.py:218`, `ww2/weapons.py:269` | prefix-specific variants | |
| `primaryInstance="..."` | 16 regex sites in 11 OPFOR files (animswap.py:28, material_grid.py:124, wake_level.py:87,110, kits, pictures, weapons, winchester1897, type100, rg_projectiles, wake_install, wake_shaders) | `dbx.parse(b).partition.attr('primaryInstance')` exists |

- **Which should win:** a structured walk in core, not a regex:
  - `bfbc2core.dbx.refs(f) -> list[(partition, guid, element)]`;
  - `dbx.primary_instance(data)`.

  `core/bfbc2core/xmlinst.py` already exists for exactly this reason (attribute order) and states that "any
  XML-matching code should go through" it.
- **The five REF character classes accept different reference sets.** They should be compared on the WW2 inputs
  before any is replaced. Section 9 lists this as unverified.

### 4.4 Game / OPFOR path discovery: 9 implementations

| Site | Order | Failure |
|---|---|---|
| `navkit/navkit.py:86 find_opfor` | given, `OPFOR_ROOT`, `project-opfor-bfbc2` beside every ancestor of navkit's folder; requires `game\` | `NavKitError` |
| `menu-tools/menucommon.py:44 find_opfor` | same as navkit (copied) | `MenuToolError` |
| `shader-tools/shaderdb.py:446 find_opfor` | given, `OPFOR_ROOT`, **only** the suite's sibling | `ShaderDbError` |
| `navkit/tools/navparity.py:51`, `navvehicle.py:593` `opfor_root` | wrappers: navvehicle → navparity → navkit | |
| `harness.py:55 opfor` | wrapper → navkit | |
| `console-convert/convert.py:38 default_exe` | `OPFOR_ROOT`, `HERE/../../project-opfor-bfbc2` | `FileNotFoundError` |
| `crash-reader/crashreader.py:44 find_exe` | `OPFOR_ROOT`, walk-up | `None` |
| `navkit/tools/detailed.py:105` | `OPFOR_ROOT` **or the hard-coded `C:\Programming\Projects\project-opfor-bfbc2`** | |
| `navkit.py:102 find_tool`; `stockmp-ai/pipeline.py:35 FBRB` (fixed path); `BFBC2 Tool Suite.pyw:37`; `ww2/attachments.py:58 DBXEDIT` | toolkit executables, four ways | |

- **Sibling BF1943 suite, 6 lines:**
  - `bf1943import.py:79-82`: `dirname(SUITE)\bf1943-tool-suite\bf1943-fbrb-tool`, or `BF1943_SUITE_FBRB`. It calls
    `sys.path.insert(0, p)` **on every call**, with no guard.
  - `bf1943import.py:85`: the error message.
  - Tests: `anim-tools/tests/test_granny_port.py:135`, `bf1943-import/tests/test_havokport.py:48`,
    `console-convert/tests/test_mesh_decode.py:84`, `shader-tools/tests/test_shaderdb.py:396`. Each spells the path
    differently (`SUITE/..`, `ROOT/../..`, `dirname(SUITE)`), and only `bf1943import` honours the environment variable.
- **The reverse direction:** OPFOR finds the suite with `OPFOR_TOOLKIT` (`ww2/attachments.py:48`). The suite finds
  OPFOR with `OPFOR_ROOT`.
- **Coupling to error text:** `core/bfbc2core/errors.py:40-42` explains "game copy not found" by matching the
  *message text* (`'no game copy'`, or `'project opfor'` and `'not found'`). Each copy's error wording therefore has to
  keep matching (section 6).
- **Which should win:** a new `core/bfbc2core/paths.py` with:
  - `find_opfor(given)`, using navkit's walk-up order (a superset of shaderdb's);
  - `find_tool(exe, toolkit_dir)`;
  - `game_exe()`;
  - `sibling(name, env)`, for `bf1943-tool-suite` and `operation-rising-sun`;
  - one `GameCopyNotFound` error that errors.py matches by type.

  The old names stay as one-line wrappers that raise their own error subclasses.

### 4.5 Write-a-resource-into-a-tree helpers

- **OPFOR:**
  - `ww2/animswap.py:20 write(out, name, data)` and `ww2/wake_level.py:538 animswap_write(out, name, data)` are the
    same three lines: makedirs, then `open(...,'wb').write`. 27 call sites, including `wl.animswap_write` from
    ww2/wake_ambience.py:100.
- **Suite:**
  - The better version is `bf1943-import/bf1943import.py:207 target(out_dir, name)`, which refuses names that leave
    `out_dir`. With `run_import` (line 215) it writes `.part` and then `os.replace`.
- **Which should win:** a suite helper with bf1943import's guard and atomic write, e.g. `bfbc2core.paths.write_entry`.
  OPFOR's two copies become calls to it.

### 4.6 Others

| What | Copies | Which should win |
|---|---|---|
| **Cached bundle unpack** with a `.source.sha256` stamp | `navkit.py:184 Project.unpack`, `stockmp-ai/pipeline.py:110 unpacked` (same stamp file and logic); `core/bfbc2core/level.py:420-425` (no stamp: the stale-terrain issue) | one `bfbc2core.fbrb.unpacked(bundle, dest, tool)`. It also fixes the terrain staleness. |
| **`fbrb.manifest` parsing** | `console-convert/convert.py:45 manifest`, `aptgraft.py:1083`, `stockmp-ai/pipeline.py:125,161,234,279,360`, `validate.py:226`, `ww2/wake_preview.py:129` | `bfbc2core.fbrb.manifest(folder) -> list[Entry]` |
| **Bundle enumeration of the game copy** | `meshview.py:55 _bundles` (Package before Dist, skips `levels/ons_`, `levels/opfor_` and folders with `opfor-*-installed.json`), `shaderdb.py:455 level_bundles`, `harness levelinfo`, `navkit.Project` | core, with OPFOR's skip rules as an explicit argument |
| **File SHA helpers** | `navkit.py:74 _sha256`, `pipeline.py:62 sha`, `navparity.py:65 _sha1`, `navdomains.py:130 sha1`, `navwater_mesh.py:853 sha1`, `levelinfo.py:388 sha1` | one helper in core |
| **Hand-rolled argv option parsers** | `navsample_offline.py:1410 _opt`, `navvehicle.py:821 _arg`, `harness.py:253 opt`, `culltree.py:1010-1024 _arg/_flag`, `stockmp.py:47 opt`, plus nested `opt` in about ten mains | low priority; argparse or one helper |
| **`Index`-like resource lookups** | `meshview.Index` (BFBC2 game copy: `where`, `read`, `parts`, `entries`), `bf1943import.Source` (BF1943: `entries`, `data`, `find`), `overlay.NavIndex` | `meshview.Index` should move down (step M3); `Source` stays in bf1943-import |
| **Report writers** | dbxport / effectport return report lines; `bf1943import.Importer.convert` returns **records (dicts)**; `aptcheck.Report` (`aptcheck.py:165`); `culltree.Out` (`culltree.py:1031`); harness `RESULT` dicts (DESIGN 5.5) | the record style of bf1943import and harness (section 6) |
| **`_Reader` cursor classes** | `mesh.py:28`, `meshset.py:26`, `shaderdb.py:387`, `xenos.py:161`; `terrain._Walk` (console-convert:140) | small. One `bfbc2core.binreader.Reader` would also give OPFOR a public replacement for `terrain._Walk`. |

---

## 5. Logic in OPFOR that belongs in the suite

| OPFOR code | What is general | Proposed suite home |
|---|---|---|
| `ww2/wake_level.py:494-535 port_sounds` (also `sounds` 484) | Ports BF1943 sound partitions: a reference closure under `sound/`, a template pool grown from donor partitions, dbxport on that pool, and EA-XMA → EA-XAS wave conversion including `_wavedatasns_N` streams. Nothing in it is Wake-specific except `RES`, the default donors and `animswap_write`. effectport's docstring (lines 20-21) already defers to it. | `bf1943-import/soundport.py` with `port(src, ix, roots, donors) -> (files, records)`, the same shape as effectport. OPFOR keeps the Wake donors. |
| `ww2/material_grid.py:63-137` `Grid`, `key`, `clone` (recursive instance clone with memo and GUID remap), `redirect` | A material-grid model (materials by name, relations by unordered pair), deep-cloning an instance with every local instance it references, and redirecting references to another partition's primary instance. | `Grid` and the merge into `bf1943-import/gridport.py`; `clone_instance(f, inst, new_guid, memo)` and `redirect_refs` into core's dbx editing layer. |
| `ww2/wake_level.py:73 _numbers` | Writes NUMBERS raw bytes with the private `dbx._put_varint`. | `dbx.set_numbers` (dbxbin already has it, `dbxbin.py:160`). |
| `ww2/wake_level.py:83-125 twins` | Two general operations: (a) point placements of partition A at partition B's primary instance and move them by an object-space offset; (b) rewrite every reference to partition A as B (the REMAP walk, 107-124). | (b) `bfbc2core.dbx.remap_refs(f, {old_partition: new_ref})`: the same walk is written three more times, in `material_grid.redirect`, `wake_install.py:414-522` (REMAP and REMAP_REFS) and `wake_level.twins`. (a) a level-editing helper: the transform maths is generic, the TWINS table is OPFOR's. |
| `ww2/animswap.py` `write` / `primary` / `ref`; `wake_level.animswap_write` | The write helper (4.5) and the primary-instance regex (4.3). | core |
| `ww2/attachments.py` | Mostly OPFOR content (bayonet geometry, models), but also: (1) the suite bootstrap (`SUITE`, sys.path, re-exports) for 30 files; (2) `guid(name, old)` (4.2); (3) `_quat`, `posed` (pose a mesh by an animation frame through granny `sample` / `curve_keys`, also used by ww2/menu_renders.py, weapon_preview.py and scopes.py). | (1) a small OPFOR `suite.py` that calls `suite_paths.use(...)` from the suite instead of hand-built paths. (3) a posing helper next to granny / skinmesh (anim-tools or model-tools). |
| Effect / port glue in `ww2/bf1943_vehicles.py:162-215`, `bf1943_objects.py:99`, `bf1943_weapons.py:195` | Three copies of one loop: port the meshes (meshport) → write LODs, mesh set and textures; Havok (havokport with the ufloat8 table from the exe); `Templates` from a pool → `dbxport.port` per partition with a `resolve` over "ported ∪ ix.where"; then effects (effectport) added to the ported set. | `bf1943-import`: an `AssetPorter(src, ix, out).port_folder(folder, pool, mesh_donors, effects) -> records`. OPFOR keeps its tables (pools, donors, seat-input maps). |
| `ww2/wake_install.py` closure, REMAP, terrain patches | Mostly an OPFOR install recipe. The reference closure over bundles ("the later build of a name wins, then stock bundles"; `closure`, 871, 166 lines) and the bundle registration are general enough for DESIGN's "bfbc2-asset-builder" (4.9), which DESIGN puts in OPFOR's repository. | Leave it in OPFOR, but split it (2,416 lines). |
| The aptcheck subprocess in 5 ww2 files | Each runs `aptcheck.py check` and reads the exit code and the stdout tail. | `aptcheck.verify` (`aptcheck.py:189`) is importable; a `check(path) -> Report` would end the subprocess and the stdout slicing. |

---

## 6. String-protocol coupling

| Producer | Consumer | Protocol |
|---|---|---|
| `effectport.port` writes `"SOUND <name>: BF1943's, for the sound porter"` (`effectport.py:176`) | `effectport.sounds_of` (`:182-184`) parses `r.split(' ', 2)[1][:-1]`; used by ww2/material_grid.py:174 | **known** |
| `dbxport.port` report lines (`dbxport.py:344-378`: `left out instance T (G): its F X will not ...`, `substituted ... (refers to X)`, `type T (G) written as ...`, `UNMAPPED`, `added`, `nested`, `renamed`) | `effectport.py:171` filters by 7 prefixes; ww2/material_grid.py:183-188 (regex `\(refers to ([^)]+)\)`, prefixes `UNMAPPED`, `type `); ww2/wake_level.py:345-350 (regex `its \w+ (\S+) will not`, `instance (\S+)`); ww2/wake_ambience.py:101; ww2/bf1943_vehicles.py:211-212 (counts `added`, `left out `, `substituted`, `type `) | **5 consumers parse dbxport's prose.** A rewording breaks counts silently. |
| effectport's summary line `"{effect}: N partitions to port, ..."` | ww2/bf1943_vehicles.py:202 `rep[0].split(": ", 1)[1].split(";")[0]` | position and punctuation |
| `meshport.Porter` plan value `'layout of {k}'` / `'layout of {k} ({n} element(s) left out)'` (`meshport.py:120,135`) | ww2/wake_install.py:1363-1364 and ww2/wake_shaders.py:498 match `r'layout of (.+)/[^/]+$'` to recover the donor mesh | the donor is known as data at the producer, but only text crosses the boundary |
| `ordercheck.py` stdout | `navkit.py:624-631` runs it as a subprocess and greps `positions:|on an area|route check|self-check`; `navsample_offline.py:1400-1404` keeps non-indented lines | navcompare is imported in-process three lines later, so ordercheck could be too (`ordercheck.PathData` is already imported by stockmp `config.py:139`) |
| `hpdcheck.py` / `hpdtool.py roundtrip/meshtrip` stdout | `navsample_offline.py:1391-1397` takes the last line | these could be in-process calls |
| `navkit.py` stdout | `navigation-studio/editing.py:131-133` shows the last two lines; `:116` streams `apply` output | acceptable for a GUI, but the result is text only |
| `aptcheck.py check` stdout and exit code | 5 OPFOR files (section 5) | |
| DbxEdit stdout `(\d+) error\(s\), (\d+) warning\(s\)` and `ERROR` lines | `stockmp-ai/validate.py:55-68` | an external C# tool, so text is unavoidable; keep a pinned DbxEdit-format test |
| Exception message text | `core/bfbc2core/errors.py:28-56 CAUSES` classifies by substring (`'no game copy'`, `'project opfor'`, `'not found'`, `'not prepared'`, `'unknown level'`, `'opengl'`) | every `find_opfor` copy's wording is load-bearing; match by exception type |
| The `to_xml` text as an API | 19 OPFOR files call `dbx.to_xml` and run regexes over it (4.3); also effectport `refs`, meshview:150 | XML text used as a query language; DESIGN and xmlinst warn against it |

---

## 7. I/O mixed with logic

**The format converters are mostly clean:** bytes in, bytes out. `texture.convert(b)`, `mesh.convert(b, layouts, ...)`,
`meshset.convert`, `havok.convert(b, klasses)`, `audio.convert(b, source, streams)`, `granny.to_pc`,
`aimanim.to_pc`, `havokport.convert(b, template, classes, table)` and `dbxport.port(data, templates, resolve)` all
return data, and none of console-convert, anim-tools or model-tools has a ≥ 25-statement function that writes or
prints. **The mixing is in the layer above:**

| Function | Mix | Effect on testing |
|---|---|---|
| `core/bfbc2core/level.py:161 load` (102 lines) | reads the game copy, **runs `navkit.prepare` on first use** (an unpack), logs | Core's main loader can start NavKit's prepare step. It is also the reason for the core → navkit edge. |
| `core/bfbc2core/level.py:397 heightfield_of` | runs FbrbTool in a subprocess, caches with no stamp | |
| `mesh-viewer/meshview.py:84 Index.__init__` | walks every bundle, parses shader databases, pickles to `work\tools_out`, prints to stderr | The index used by 30 OPFOR files and 3 suite folders cannot be built in memory without touching disk. |
| `meshview.py:191 load_mesh` | prints | |
| `bf1943-import/bf1943import.py:215 run_import`, `:264 check` | convert + write + log (`Importer.convert` itself is pure: good) | fine as a CLI layer |
| `bf1943-import/dbxport.py:34 Templates.__init__` | globs folders and opens files | Pass `(name, bytes)` pairs (already supported); the folder branch is where the order problem is. |
| `bf1943-import/dbxparity.py:74 compare` | prints | |
| `stockmp-ai/pipeline.py:287 build`, `stockmp.py:95 install`, `validate.py:344 validate` | read, write and log in one function | the stock-MP build |
| `shader-tools/shaderdb.py:569 level_meshes` | walks unpacked folders and computes | |
| `culltree/culltree.py:736 ref_pairs`, `:850 add` | | |
| navkit/tools: `navbuild.py:891 stage` (198 statements, logs), `navvehicle.py:667 build_level`, `navwater_mesh.py:861 level_run`, `navsample_offline.py:1272 parity` / `:1213 stock_mp`, `navkit.py:742 _apply`, `:220 prepare`, `:502 sample_run` | each orchestrates and computes | |
| **OPFOR** | `ww2/wake_translate.py:317 translate_db` (213 statements, writes and prints), `wake_shaders.py:505 clone`, `wake_install.py:1955 build`, `:1538 fix_layouts`, `bf1943_vehicles.py:162 build`, `bf1943_objects.py:99 build`, `wake_level.py:417 resources`, `wake_level.py:494 port_sounds` | each port step writes as it computes, so a WW2 rebuild comparison has to hash the output tree. |

58 suite functions take `log=print` as their default sink. That is a good seam to keep: callers can capture output
without patching.

---

## 8. Proposed layering and migration

### 8.1 Layers (by module, not only by folder)

| Layer | Contents |
|---|---|
| **L0 core** | `suite_paths`; `bfbc2core.{dbx, fbrb, terrain, water, xmlinst, errors}`; new `paths`, `dbx` editing helpers, and `fbrb.unpacked` / `manifest` |
| **L1 formats** | console-convert (`texture, mesh, meshset, havok, audio, terrain, soot`), anim-tools (`granny, aimanim`), model-tools (`skinmesh, shapes, texwrite, decimate`), shader-tools (`shaderdb, xenos, xenos_hlsl, pcbuild`), console-readers (`apt, stfs, xiso`); NavKit's file formats `hpdtool, kymesh, hkpack, navsample, levelobjects`; `stockmp/dbxbin`; `menucommon` |
| **L2 builders / porters / models** | bf1943-import (`dbxport, meshport, havokport, effectport, bf1943import`); NavKit's generator (`navbuild, navwalk, navislands, navdetail, navlinks, navvehicle, navwater*, navdomains, navsample_offline, navdiff, navdf, navboundary, ...`); stockmp (`author, pipeline, levelmodel, config`); culltree; aptgraft; and core's **level / navigation model** `bfbc2core.{level, navdata, navpath, navtune}` |
| **L3 tools / GUIs / CLIs** | `navkit.py`, navkitGUI, NavKit's analysis CLIs (`navparity, navregress, navcompare, ordercheck, hpdcheck, ...`), mesh-viewer, map-inspector, navigation-studio, harness, diff-workbench, the hub, `stockmp.py`, `validate.py`, aptcheck, aptrefs, bc2loc, `convert.py`, and `bfbc2core.{viewer, pick}` |
| **L4 consumer** | OPFOR `src\tools\ww2` (and `src\tools\onslaught`, `menus`, `testrun`, not audited) |

`bfbc2core` holds both L0 and L2/L3 modules. That is the root of the folder-level cycle: `level`, `navdata` and
`pick` are not data-layer modules.

### 8.2 Every non-test edge that breaks the layering

From `layers.py` in the scratchpad: 17 module edges, 31 import sites.

| # | From (layer) | To (layer) | Sites | What crosses |
|---|---|---|---|---|
| 1 | `core/bfbc2core/level.py` (L2) | `navkit.py` (L3) | 166, 403 | `find_opfor`, `Project`, `prepare`, `find_tool` |
| 2 | `bfbc2core/navdata.py` (L2) | `navdiff` (L2 by the map; a core-folder module importing NavKit at top level) | 36-38 | `area_cells`, `GRID`, `Y_OFFSET`, `STACK_TOL` + hpdtool / kymesh |
| 3 | `bfbc2core/pick.py` (L3, core folder) | `navkit.py` | 19 | `Project`, `level_dirs`, `dice_pathdata` |
| 4 | `anim-tools/granny.py` (L1) | `meshview` (L3) | 613 (+614 shaderdb) | game-copy lookup in `granny.load` |
| 5-7 | `bf1943-import/{bf1943import, dbxparity, portparity}.py` (L2) | `meshview` (L3) | 150, 284; 145; 217 | `meshview.Index` |
| 8 | `stockmp-ai/pipeline.py` (L2) | `navkit.py` (L3) | 77 | `find_opfor` |
| 9 | `stockmp-ai/config.py` (L2) | `ordercheck` (L3) | 139 | `ordercheck.PathData`, a path-data reader that should be L1 |
| 10 | `navsample_offline.py` (L2) | `navkit.py` (L3) | 1086, 1108, 1120, 1192, 1340, 1351, 1387, 1424 | `Project`, `ensure_settings`, `prepare`, `bin`, `detailed_*` |
| 11 | `navsample_offline.py` | `navparity` / `navregress` (L3) | 1277; 1278, 1352 | `reference_of`, `level_xml`, `level_water`, `order_points_of` |
| 12 | `navvehicle.py` | `navparity` / `navregress` | 594, 706, 782; 671 | `opfor_root`, `reference_of`, `build_level` |
| 13-16 | `navboundary.py:303`, `navdf.py:220`, `navdomains.py:411,504`, `navwater.py:31`, `navwater_mesh.py:863,977` | `navparity` | | `opfor_root`, `level_xml`, `need_reference`, `terrain_bounds`, `islands`, `run_level`, `LEVELS` |

By folder, the cycle `core ↔ navkit` (1.2) and the implicit edges (1.3) also count as breaks.

**OPFOR (L4)** depends on L1, L2 and L3: `meshview` (L3) from 30 files, and `aptcheck` / aptgraft internals. That is
allowed downward, but `meshview` is used as a library. Moving its `Index` down (step M3) keeps the L4 → L3 edge only
for `render` / `page`.

### 8.3 Ordered migration plan

**Guards for every step:**
- **G1:** `python run_tests.py quick` (54 tests).
- **G2:** the converter baseline from QUALITY_AUDIT round 3: the same 30-run sample on a checkout before and after,
  with timings and paths normalised and outputs byte-compared. It includes portparity / dbxparity JSON and the 77
  SHA-256s of `bf1943import.py import garand`.
- **G3:** a WW2 rebuild comparison: hash every file under `OPFOR\work\ww2\**` after the WW2 builds and
  `wake_install.py --plan` on the old suite, rebuild on the new suite, and compare. Use `--plan` only, never
  `--apply`. Confirm first that no build in the order writes to `game\` (section 9).
- **G4:** for NavKit-touching steps, the sp_004_b / sp_009 regression.

"**OPFOR**" marks a step that changes something OPFOR's ww2 scripts rely on. Every step keeps each module importable
with only its own folder on `sys.path`, and every old public name keeps working through a wrapper or re-export. No
OPFOR change is *required* unless stated.

| Step | What moves or merges | Fixes | Risk | OPFOR |
|---|---|---|---|---|
| **M1** | Add `core/bfbc2core/paths.py`: `find_opfor`, `find_tool`, `game_exe`, `sibling(name, env)`, `GameCopyNotFound`. The 9 implementations in 4.4 become one-line wrappers that keep their names and error subclasses. Remove the hard-coded path in `detailed.py:105`. `bf1943import.fbrb1943` guards its `sys.path.insert`. `errors.CAUSES` matches `GameCopyNotFound` by type, keeping the text tests as a fallback. | Edges 1 (partly), 3 (partly) and 8; navparity / navvehicle `opfor_root`; per-call `sys.path` growth; message-text coupling | Low. shaderdb's narrower search widens to walk-up; the found folder could differ only where two `project-opfor-bfbc2` folders exist on different ancestors. | `shaderdb.find_opfor` (25 files) keeps its name and behaviour. None. |
| **M2** | Additive public API in core: `dbx.primary_instance`, `dbx.refs` (structured), `dbx.put_varint` (keep `_put_varint` as an alias), `dbx.set_numbers`, `dbx.new_guid(namespace, *parts)` (from dbxbin, which re-exports it); `errors.log_path()`; `navpath.adjacency` (keep the alias); `meshview.layout` / `surfaces` public (keep `_layout` / `_surfaces`); `mesh.pair_by_kind`; console-convert `terrain.Walk`. | All 5 GUI / harness private accesses and all 14 OPFOR private accesses get a public name | None: additive. G2 is unchanged by construction. | Nothing required. OPFOR can switch from privates later. |
| **M3** | Move `meshview.Index` and `_bundles` to `bfbc2-shader-tools/gameindex.py` (L1: it needs `shaderdb.load` for `parts` / `entries`; the pickle path and `CACHE_VERSION` stay the same). `meshview` re-exports `Index`. granny, bf1943import, dbxparity and portparity import `gameindex` instead of `meshview`. OPFOR's skip rules (`levels/ons_`, `opfor_`, `opfor-*-installed.json`) become an argument with today's defaults. | Edges 4-7; the bf1943-import → mesh-viewer and anim-tools → mesh-viewer folder edges; the implicit `granny.py:614` edge | Low to medium: a pickle written by the old module path must still unpickle (it stores plain dicts, so it does). | **OPFOR:** `meshview.Index` still works through the re-export, and meshview still puts shader-tools on the path, so `attachments.py`'s implicit shaderdb edge still resolves. Recommended OPFOR follow-up: add `bfbc2-shader-tools` to attachments' path list explicitly. |
| **M4** | Split NavKit's library out of `navkit.py`: `tools/navproject.py` (`Project`, `ensure_settings`, `prepare`, `detailed_sectors/rects`, `bin`; `find_opfor` / `find_tool` from M1). `tools/navinputs.py` takes navparity's and navregress's level-input helpers (`reference_of`, `level_xml`, `need_reference`, `terrain_bounds`, `area_orders_of`, `order_points_of`, `level_water`, `reference_bounds`). navkit.py, navparity and navregress re-export them. | Edges 1, 3, 8 and 10-16; the module cycle `level ↔ navkit ↔ navregress ↔ navparity ↔ navdf`; the core ↔ navkit folder cycle (with M5) | Medium: navkit.py has 98 % quick coverage, and the tests monkeypatch module attributes (3.3). Patches on `navkit.X` must move to `navproject.X`, or the re-export breaks the patch. | None: OPFOR ww2 does not import navkit (OPFOR onslaught / testrun: not audited). |
| **M5** | Reclassify core's NavKit model: move `navdata`, `navpath`, `navtune`, `level`, `pick` and `viewer` out of the L0 package into `bfbc2core.nav` / `bfbc2core.view` subpackages, or document them as L2 / L3 in the layer map and leave the files where they are. The minimal version: `level.load` no longer calls `navkit.prepare` itself. It raises "not prepared" (as `errors.CAUSES` already explains), and the inspector / studio call `navproject.prepare` first. | The last upward edge from core | Medium: changes `level.load`'s first-use behaviour, so the map-inspector and studio offline tests need updating. | None for ww2 (it does not use `level`). |
| **M6** | Structured reports: `dbxport.port` and `effectport.port` also return records, e.g. `[{'kind': 'substituted', 'field': ..., 'refers_to': ...}]`, with the prose report built from them unchanged. effectport returns `sounds` as a list; `sounds_of` stays, reading the list. `meshport` plan entries carry `donor_mesh`. navkit / navsample_offline call ordercheck, hpdcheck and hpdtool in-process. | Section 6, all rows on the suite side | Low if the prose lines are produced from the records byte-identically (checked by G2's report diff). | **OPFOR:** optional switch of 5 parse sites (material_grid, wake_level, wake_ambience, bf1943_vehicles, wake_install / wake_shaders) to the records. The old text stays valid. |
| **M7** | Shared helpers: `fbrb.unpacked(bundle, dest, tool)` with the stamp, used by navproject, stockmp `pipeline.unpacked` and `level.heightfield_of`; `fbrb.manifest(folder)`; one sha helper; `paths.write_entry(out, name, data)` with bf1943import's guard. | 4.6; the stale terrain unpack (3.2) | Low. The terrain unpack becomes stamp-checked, so one extra unpack happens on first run. | None. Optional: OPFOR's `animswap.write` / `wake_level.animswap_write` delegate. |
| **M8** | Deterministic templates: `dbxport.Templates` sorts its glob (`dbxport.py:42`), and effectport's pool order is documented. | File-order-dependent template choice | **Can change output** where two files hold a first instance of the same type. Run G2 and G3. If outputs differ, either keep today's order as the sorted order for the known inputs, or accept the change and record it, as round 3 did for the NavKit re-link. | Possible output difference in WW2 builds: G3 decides. |
| **M9** | Port helpers from OPFOR into the suite: `bf1943-import/soundport.py` (from `wake_level.port_sounds`), `gridport.py` plus `dbx.clone_instance` / `remap_refs` (from material_grid and the twins / REMAP walks), and `AssetPorter.port_folder` (the shared loop of bf1943_vehicles / objects / weapons). | Section 5 | Medium: each port must reproduce OPFOR's outputs byte for byte. Do one helper per step, each with G3. | **OPFOR:** `wake_level.port_sounds` and `material_grid` change to call the suite. These are OPFOR edits, made in the same pair of commits. |
| **M10** | DBX editing layer: dbxbin's index-keeping editor becomes `bfbc2core.dbxedit` (stockmp's `dbxbin` re-exports it); culltree's `El` / `Doc` moves onto it once its byte-exact round trip on every cull tree is proven. | 4.1 | Medium to high: three codecs with different byte guarantees. Guard with the full tier (DbxEdit corpus), stock-MP Atacama, and culltree's verify on every level. | None. |
| **M11** | Split the god modules along their existing section headers: aptgraft (model / assembler / movie / graft / the Onslaught timer recipe); shaderdb (format / merge / CLI); navsample_offline (sampler / comparison / drivers); culltree (after M10). Each keeps a re-exporting module of the old name. | Section 2 | Low per split if re-exported. | **OPFOR:** aptgraft's names used by ww2/menu_install.py (`Code`, `Ins`, `walk_ins`, ...) must stay importable from `aptgraft`. |

**OPFOR follow-ups, outside the suite, not required by any step:**
- a `src\tools\ww2\suite.py` that bootstraps through `suite_paths` instead of hand-built `sys.path`;
- `weapons.edit(out=...)` instead of the 8 `weapons.OUT` patches;
- split `wake_install.py`.

### 8.4 Proposed layer-check test (not added)

`tests/test_layers.py`, in the quick tier, AST only, about 1 second:

1. **Collect.** Walk every non-test `.py` / `.pyw` under the suite. For each file, collect its `suite_paths.use(...)`
   names and every `import` / `from ... import` (lazy ones too).
2. **Resolve.** Resolve each import against the file's own folder plus its `use` folders only. **Fail** when an import
   resolves to a suite module only through another module's bootstrap. That is the implicit-edge rule, section 1.3:
   4 non-test sites today.
3. **Assign layers.** Map each module to a layer with a table: folder defaults plus per-module overrides, as in
   section 8.1. **Fail** when `layer(target) > layer(source)`.
4. **Allow known breaks.** Keep a checked-in list of `(source:line → target)` for today's 17 edges / 31 sites. The
   test fails when a new edge appears, *and* when an allowed edge disappears without being removed from the list, so
   the list only shrinks.
5. **Check the contract with OPFOR (optional).** A list of the public names OPFOR uses (section 1.5's table). The test
   imports each module with only its folder on `sys.path` and asserts each name exists. This catches a re-export lost
   in a split (steps M3, M4 and M11) without needing the OPFOR repository.

The prototype of steps 1-4 is `layers.py` in the session scratchpad. It produced the list in 8.2.

---

## 9. What this audit could not determine

- **Run-time behaviour:**
  - No converter baseline and no WW2 rebuild was run: the audit was read-only.
  - Whether every WW2 build step stays out of `game\` was not verified. `wake_install.py --plan` says it changes
    nothing; ww2/names.py names `game\Package\async\startup-00.fbrb` at line 38, and menu_install.py was not traced.
- **Equivalence of the REF regex variants** (4.3) on the real WW2 inputs. In particular, whether
  `ww2/bf1943_objects.py:72` (lower case, no `re.I`) misses any reference today.
- **Template-order sensitivity:** whether sorting `Templates` (M8) changes any current output.
- **OPFOR folders other than ww2:** `src\tools\onslaught` (8 files), `menus\onslaught_art.py` and `testrun\testrun.py`
  also reference the suite / `OPFOR_TOOLKIT`. They were not audited, so the "no OPFOR change" claims for steps M4 and M5
  cover ww2 only.
- **The heuristics:**
  - The I/O-mix scan counts `open`, `makedirs`, `print`, `log`, `subprocess` and similar by name. A few calls may be
    missed or miscounted.
  - Dynamic imports (`__import__('author')` in `validate.py:125`, `__import__('texture')` in ww2/wake_shaders.py:383)
    are not in the graph.
- **Test-only edges** were summarised (1.2), not listed one by one.
