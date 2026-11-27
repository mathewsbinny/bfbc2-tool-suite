# The console conversion: Xbox 360 -> PC

Two layers:

- **Resources** (`bc2-convert`, `bfbc2-console-convert/convert.py`, `bfbc2.exe convert / audio`): one Xbox 360
  resource in, its PC form out - HavokPhysicsData, textures (XenonTexture -> ITexture), MeshData and mesh sets,
  terrain and water resources, soot meshes, Wave sounds (EA-XMA -> EA-XAS). The formats are layer 1's
  (`formats/havok.md`, `texture.md`, `mesh.md`, `terrain.md`, `water.md`, `audio.md`).
- **Levels** (Project OPFOR's `src\tools\onslaught\console_port.py`): a console Onslaught level from the user's own
  Xbox 360 package, built on the PC map's Linux dedicated-server level in fourteen stages, each a separate tool with
  `--plan` / `--apply` / `--revert` under the stage protocol (`transforms/install.md`, XINST-STG-1 to XINST-STG-4).

The ground truth is the **twin**: a resource the console and PC copies of BFBC2 both ship. A conversion is right
when the converted console file equals its PC twin, byte for byte or in every value the game reads.

## 1. Resources

### XCONS-HK-1: Havok: wrapper identical, packfile equivalent to the PC twin

Status: **verified** · Variants: Xbox 360

The converted HavokPhysicsData's wrapper is byte-identical to the PC twin's (HAVOK-WRAP-2), and its packfile equal
in every byte but values the game does not read or rebuilds at load, which differ between the PC's own files too
(index padding, invalidated broadphase volumes, triangle offsets computed at load, an unused motion index). Class
layouts come from the user's `BFBC2Game.exe` (HAVOK-CLS-1); a byte the layouts would convert at two widths, or a
class the executable lacks, is refused.

[code: rust/bc2-convert/src/havok.rs:577-645] [code: rust/bc2-convert/src/havok.rs:745]
[test: bfbc2-console-convert/tests/test_havok.py::main] [test: rust/bc2-convert/src/havok_tests.rs::a_byte_converted_as_two_widths_is_refused] [test: rust/bc2-convert/src/havok_tests.rs::refusals_and_the_reference]
[data: 400 HavokPhysicsData shared by the four Onslaught builds and the PC: all equivalent; all 594 of the four console levels convert]
[doc: bfbc2-console-convert/FORMAT.md:58-73]

### XCONS-TEX-1: Textures: the layout's length, the twin's picture, the console's flags

Status: **verified** · Variants: Xbox 360

A converted texture is exactly as long as the PC layout of its size and format says (TEX-HDR-2); its blocks are the
console's, untiled and byte-swapped (TEX-X-6), so it is the twin's picture, not its bytes (each platform was
compressed separately, TEX-X-8); its mip chain is no worse than the PC file's own; it keeps the console header's
flags (TEX-FLAG-1). Formats the converter does not know are refused with the reference's message (TEX-X-7).

[code: rust/bc2-convert/src/texture.rs:272-310]
[test: bfbc2-console-convert/tests/test_texture.py::main] [test: rust/bc2-convert/src/texture_tests.rs::conversions_as_the_reference] [test: rust/bc2-convert/src/texture_tests.rs::refusals_carry_the_reference_messages]
[data: 420 console textures of the Onslaught builds convert; of their PC twins of the same size and format 4 byte-identical, 188 the same picture re-encoded, 1 different art]
[doc: bfbc2-console-convert/FORMAT.md:118-134]

### XCONS-MESH-1: Meshes and mesh sets: values swapped, strips unrolled

Status: **verified** · Variants: Xbox 360, BF1943

A converted MeshData has the PC header, its vertex values swapped per value width (MESH-CONV-1), console-only
element formats decoded to half4 (MESH-CONV-2), triangle strips unrolled into lists (MESH-PRIM-2) and gaps between
vertex windows zeroed; a mesh set is rewritten by the PC writer (MSET-WRITE-1). A layout the reference did not
convert is refused.

[code: rust/bc2-convert/src/mesh.rs:236] [test: rust/bc2-convert/src/mesh_tests.rs::swaps_by_value_width] [test: rust/bc2-convert/src/mesh_tests.rs::refuses_what_the_reference_does_not_convert]
[test: rust/bc2-convert/src/mesh_tests.rs::vertices_in_order_with_zeroed_gaps] [test: bfbc2-console-convert/tests/test_console_f2a_native.py::meshes]

### XCONS-TERR-1: Terrain, water and soot: the twin's bytes

Status: **verified** · Variants: Xbox 360, BF1943

Height fields, material maps, visual terrain, water meshes and visual water convert by swapping each value at its
width with the quadtree's node count recomputed; a converted soot mesh equals its PC twin byte for byte.

[code: rust/bc2-convert/src/terrain.rs:146-245] [code: rust/bc2-convert/src/terrain.rs:645]
[test: bfbc2-console-convert/tests/test_terrain.py::main] [test: bfbc2-console-convert/tests/test_soot.py::main] [test: rust/bc2-convert/src/terrain.rs::quadtree]
[data: soot: 30 of 30 BFBC2 twins byte for byte; BF1943 404 of 406 (16-byte vertices refused)]

### XCONS-SND-1: Sounds: EA-XMA in, EA-XAS out, the PC wrapper

Status: **verified** · Variants: Xbox 360, BF1943

A console Wave is decoded (EA-XMA, XMA-DEC-1) and re-encoded as EA-XAS by the suite's encoder (XAS-ENC-1) in
DICE's block layout (XAS-BLK-1), held in memory in a PC Wave (WAVE-WR-1); its structure (segments, block lengths,
sample counts) is that of the PC twins.

[code: rust/bc2-convert/src/wavport.rs:147-190] [code: rust/bc2-convert/src/audio.rs:85-97]
[test: bfbc2-console-convert/tests/test_audio.py::main] [test: rust/bc2-convert/src/audio_tests.rs::groups_as_the_reference] [test: rust/bc2-convert/src/audio_tests.rs::layout_of_frames]
[data: 35,811 BFBC2 twins' wrappers compared]

### XCONS-PAR-1: The Rust converters give the retired Python's bytes

Status: **verified**

Every converter in `bc2-convert` gives, for every console and BF1943 resource of the game copy and OPFOR's work
folders and for built and damaged inputs, the bytes (or the refusal) the retired Python converters gave, recorded
before their deletion (`f0_*_replay*.json`, `f2a_console_replay*.json`).

[test: bfbc2-console-convert/tests/test_console_f2a_native.py::quick] [test: bfbc2-console-convert/tests/test_console_f2a_native.py::textures_game]
[test: bfbc2-console-convert/tests/test_texture_native.py::game]

### XCONS-PAR-2: Recorded measures are the same bits on every machine

Status: **verified**

A value the tests record (a golden or a replayed reference answer) is computed with integer arithmetic and IEEE's
correctly rounded operations only (+, -, *, /, sqrt, and in Rust a correctly rounded fma: `libm::fma`), never with a
NumPy operation BLAS computes (`np.dot`, `np.corrcoef`, `np.linalg.norm` of a vector) or the C runtime's `fma`:
their last bits follow the CPU (OpenBLAS picks its kernel per CPU: `SkylakeX` on the development PC, `Haswell` /
`Zen` on GitHub's runner, and the bits differ) or the installed Windows (`f64::mul_add` without the `fma` target
feature calls the C runtime's `fma`). Round CI1 found two: `texture.check`'s correlation, now exact in the core
(`texcheck::correlation`: the five sums in integers, each rounded once; the retired reference's NumPy answers
re-recorded where only the correlation differed), and `navpath.route`'s straight distance, now the estimate's own
formula (`navpath.distance`: np.linalg.norm's bits on the recording PC, so the recorded routes stand). The quick
tier passes with `OPENBLAS_CORETYPE=Haswell` (the runner's kernel) as without.

[code: rust/bc2-convert/src/texcheck.rs:162-196] [code: core/bfbc2core/navpath.py:89-93]
[code: rust/bc2-navbuild/src/astar.rs:26-33]
[test: rust/bc2-convert/src/texcheck.rs::correlation_exact]
[test: bfbc2-console-convert/tests/test_console_f2a_native.py::quick]
[test: tests/test_e3o_native.py::quick_navpath]

## 2. Levels: the Onslaught port's stages

### XCONS-STG-1: Fourteen stages in a fixed order

Status: **implemented** · Variants: Xbox 360

`console_port.py <level> --xbox <package> --linux <levels> [--from N]` runs, in order: 1 the level from the console
level and the PC map, with registration; 2 the console gameplay on the Linux server level; 3 the Onslaught mode;
4 the `mp2_common` files the level takes copied in; 5 vehicle spawns without PC physics left out; 6 console soldiers
and kits; 7 shader databases covering every placed and reached mesh; 8 the Mark V (one level); 9 PC input for
console seats; 10 every referenced but unloaded PC partition copied in; 11 cull-tree places; 12 the loading video;
13 the HUD; 14 the Mark V's HUD icon (one level). The order is load-bearing: stage 7 follows 6 (the kits' weapons),
9 and 10 follow 8. Stage 4 takes a level's folder list, if it has one, and the closure of what only PC
mp2_common provides (XCONS-ONS-4).

[code: opfor:src/tools/onslaught/console_port.py:1-30] [code: opfor:src/tools/onslaught/console_port.py:75-93]
[doc: opfor:DEVELOPMENT_NOTES.md:1256-1279]

### XCONS-STG-2: What the stages guard against

Status: **observed** · Variants: Xbox 360

Three stages exist because the unconverted data crashed the PC game: console-only vehicle builds (stage 5,
0x5A5A61), console action maps (stage 9, a reference without a value dereferenced at 0x4EB538), and console road
and stop signs drawn without a PC vertex layout (0x98BD6E, still open on Isla Inocentes). The engine side of each:
LOAD-PART-3, LOAD-PART-2, RNDR-DRAW-6.

[exe: BFBC2Game.exe 0x5A5A61 (console-only vehicle mesh and physics), 0x4EB538 (null string of a value-less reference), 0x98BD6E (renderer)]
[doc: opfor:DEVELOPMENT_NOTES.md:1266-1270] [doc: opfor:DEVELOPMENT_NOTES.md:1350]

### XCONS-ONS-1: The console Onslaught import gives the retired Python's work files

Status: **verified** · Variants: Xbox 360

The first stage (`console_onslaught.py`'s build and install preparation, `bc2_install::onslaught` since round OR5)
gives the retired Python's files in its work folder byte for byte: the extracted package files, the unpacks and
their stamps, `level.cmd`, the level and its shared files, the description, the bundles and `report.json`
(written as Python's text mode wrote them, `\r\n`), and the game copy's writes in the same order (the level's
files, then startup-00, package.mft, bundleManifest: REG-NEW-4). Where the Python's answer hung on a set's or a
folder listing's order, it does not: the shared files are copied in name order, the campaign bundles searched in
CAMPAIGN's order. The printed count of a stripped bundle's dropped markers is the Python's (markers dropped less
the renamed copies added; it can be negative).

[code: rust/bc2-install/src/onslaught.rs:549-568] [code: rust/bc2-install/src/onslaught.rs:1049-1058] [code: opfor:src/tools/onslaught/console_onslaught.py:24-72]
[test: rust/bc2-install/src/onslaught.rs::report_text]
[data: round OR5, 2026-11-23: the user's Update #3 package on ONS_MP_008 / MP_008 and ONS_MP_002 / MP_002, every work file and write of the Rust import identical to the Python's (rust/BENCHMARKS.md "OR5")]

### XCONS-ONS-2: The Linux base stage gives the retired Python's work files; its cull slots

Status: **verified** · Variants: Xbox 360

Stage 2 (`console_linux_base.py`'s build, `bc2_install::onslaught::linux` since round OR9) gives the retired Python's
work files byte for byte: the Linux level's stamped unpack, the renamed base, the copied Linux files (new partition
GUIDs `uuid5(URL, 'opfor-linux:<new name>')`), `merge.cmd`, `merge.log`, `enum.cmd`, the merged level and the packed
bundle. Each merged `ReferenceObjectData` takes the Linux level's cull slot of the same GUID, else of the same
blueprint at the same position rounded to 0.1 m (the last such Linux object wins), else of the nearest unused
same-blueprint Linux object under 50 m (ties to the lower slot), else 0. The slots are DICE's (the Linux server
level's own); the matching is OPFOR's rule, not DICE data, and keeps the Python's arithmetic where a last bit could
choose another object: the distance is CPython's `math.dist` (its compensated `vector_norm`, `bc2_pyrt::math`),
the rounding `round(x, 1)`, -0.0 the same key as 0.0. On the user's four levels the Rust stage 2 bundles are the
Python's, and ONS_MP_002's is the bundle the user's own install made (its `.opfor-mode-orig`).

[code: rust/bc2-install/src/onslaught/linux.rs:244-330] [code: rust/bc2-install/src/onslaught/linux.rs:354-370] [code: opfor:src/tools/onslaught/console_linux_base.py:57-73]
[test: rust/bc2-install/src/onslaught/linux.rs::cull_slots]
[data: round OR9, 2026-11-27: the user's ONS_MP_002 / 004 / 005 / 008 on their Linux server levels, every work and game file of the Rust stage identical to the Python's (rust/BENCHMARKS.md "OR9")]

### XCONS-ONS-3: The Onslaught mode stage gives the retired Python's work files

Status: **verified** · Variants: Xbox 360

Stage 3 (`console_mode.py`'s plan and apply, `bc2_install::onslaught::mode` since round OR9) gives the retired
Python's work files byte for byte: the level's partition with the console `GameMode` reference, the console
`SpawnPointManager` and its data, Nelson Bay's PC aerial heightmap values, every hoisted flag `OnlyTakeableByTeam 0`,
and the console's pre-destructions within 1.5 m of a placed door's position left out (the distance as Python's
`** 2` and `** 0.5`: the C library's `pow`); `GameModes/Onslaught.dbx` from the PC install's Conquest with the console
mode's GUIDs (the console's partition GUID when its mp2_common has the mode, else `uuid5(URL,
'opfor:gamemodes/onslaught:<instance>')`), its manifest line after Conquest's. Not ported: the explosive objects'
branch, which the Python never ran (`EXPLOSIVES` off: Nelson Bay crashed at load with them). Only the temporary
folder's name differs (Python's `mkdtemp`), and it is deleted before the bundle is packed.

[code: rust/bc2-install/src/onslaught/mode.rs:125-170] [code: rust/bc2-install/src/onslaught/mode.rs:221-240] [code: opfor:src/tools/onslaught/console_mode.py:45-54]
[test: rust/bc2-install/src/onslaught/mode.rs::doors]
[data: round OR9, 2026-11-27: the user's four levels, every work and game file of the Rust stage identical to the Python's; ONS_MP_002's bundle is the user's installed one (its `.opfor-loadset-orig`)]

### XCONS-ONS-4: The load set stage gives the retired Python's work files; the closure's order

Status: **verified** · Variants: Xbox 360

Stage 4 (`console_loadset.py`'s plan and apply, `bc2_install::onslaught::loadset` since round OR9) gives the retired
Python's work files and writes byte for byte: the level's `package.mft` blocks without their `mp2_common` targets
(the other levels' blocks and the file's line ends as they are), and the PC mp2_common entries the level takes copied
into its bundle - its folder list (Atacama: a pattern, less what the bundles it loads anyway carry), then every
partition the level references that only mp2_common provides, what those reference in turn, and each one's
resources (`<name>.res`, `<name>_*.res`), in the bundle listing's order. The Python visited the closure in a set's
order; the closure, and the partitions' XML files written on the way, do not depend on it, so the Rust visits in name
order. Not ported: `mp2_only`, a diagnostic nothing called; a level without a folder list does not stop at this stage
(XCONS-STG-1's "stops at stage 4" was the docstring's, not the code's): it takes the closure alone.

[code: rust/bc2-install/src/onslaught/loadset.rs:119-140] [code: rust/bc2-install/src/onslaught/loadset.rs:199-305] [code: opfor:src/tools/onslaught/console_loadset.py:46-54]
[test: rust/bc2-install/src/onslaught/loadset.rs::manifest_blocks]
[data: round OR9, 2026-11-27: the user's four levels, also with the manifest from before a loadset run (146 mp2_common targets) and damaged inputs, every work and game file of the Rust stage identical to the Python's]

### XCONS-ONS-5: The vehicles stage gives the retired Python's work files

Status: **verified** · Variants: Xbox 360

Stage 5 (`console_vehicles.py`'s build and apply, `bc2_install::onslaught::vehicles` since round OR10) gives the
retired Python's work files byte for byte: each `VehicleSpawnEntityData` whose vehicle partition is in the level's
bundle with every `Asset` reference null (a console-only mesh and physics, which crashed the game at 0x5A5A61:
XCONS-STG-2) is dropped with DbxEdit's `drop`; then the vehicle's own partitions go - those named like it, and what
they reach that bears the name's first word once no partition left names it - with their manifest lines; the level's
manifest size is set again and the XML deleted. A vehicle another bundle holds (a PC vehicle) is kept. On the user's
levels only ONS_MP_002's Mark V is dropped (five partitions with it); the other three levels pack nothing.

[code: rust/bc2-install/src/onslaught/vehicles.rs:52-90] [code: rust/bc2-install/src/onslaught/vehicles.rs:94-150] [code: opfor:src/tools/onslaught/console_vehicles.py:30-40]
[test: rust/bc2-install/src/onslaught/vehicles.rs::gone_follows_the_vehicle_name]
[data: round OR10, 2026-11-27: the user's ONS_MP_002 / 004 / 005 / 008, stages 5 - 7 chained, every work and game file of the Rust stages identical to the Python's (rust/BENCHMARKS.md "OR10")]

### XCONS-ONS-6: The soldiers stage gives the retired Python's work files; its class map and notes' order

Status: **verified** · Variants: Xbox 360

Stage 6 (`console_soldiers.py`'s build and apply, `bc2_install::onslaught::soldiers` since round OR10) gives the
retired Python's work files byte for byte: per console class a spawner or the Onslaught team names, a copy of the PC
soldier stage 1 swapped it for, under `Objects/Soldiers/OPFOR_ONS_RU_AI|OPFOR_ONS_US/<class>`, with the console's kit,
kit class, spotted times and head collision (player kits also DICE's co-op tuning), the dropped weapon's entries the
console's with the PC entry's ammo per weapon slot, new GUIDs `uuid5(URL, 'opfor-soldier:<new partition>:<old>')`;
the console data it reaches copied in (refused when it has resources); the spawners and team re-pointed at the
`SoldierAsset`; the new manifest lines ahead of the level's. Two Python accidents, decided: the class map is the
suite's `bc2_install::onslaught::SOLDIERS` (stage 1's own) - the Python read it from `console_onslaught.py`'s text,
which round OR5 moved to Rust, so since OR5 the stage stopped at once with `ValueError: substring not found`; and a
player kit's console fields are set per type in the console blueprint's instance order, where the Python went through
a set (its notes' order changed from run to run; the partition does not depend on it).

[code: rust/bc2-install/src/onslaught/soldiers.rs:132-245] [code: rust/bc2-install/src/onslaught/soldiers.rs:355-420] [code: rust/bc2-install/src/onslaught.rs:79-144] [code: opfor:src/tools/onslaught/console_soldiers.py:45-60]
[test: rust/bc2-install/src/onslaught/soldiers.rs::blueprint_takes_the_console_kit]
[data: round OR10, 2026-11-27: the user's four levels (the Python twin given the pre-OR5 class map), every work and game file of the Rust stage identical to the Python's]

### XCONS-ONS-7: The shaders stage merges as shaderdb.py does and translates nothing

Status: **verified** · Variants: Xbox 360

Stage 7 (`console_shaders.py`'s plan and apply, `bc2_install::onslaught::shaders` since round OR10) gives the retired
Python's work files byte for byte: `needs.txt` (every mesh the level's partitions name, then the shared-bundle meshes,
the whole entry names of the house and barrack pairs, and every mesh partition the level's vehicle spawns reach with
the entries of the shader set each spawn draws), and the level's Dx9, Dx10 and Dx11 databases merged from the PC
map's other modes, then `mp_012cq`, `mp_001`, `sp_005_b`, `sp_04`, `sp_02`, `mp_012gr`, `sp_005` and `mp_008cq` (a name
two donors have comes from the first), with their `.merge.txt` reports. The merge is `shaderdb.py merge`'s
(`bc2_convert::shadercmd`, moved there from `bfbc2.exe shaderdb`): it copies DICE's compiled programs and translates
no console program, so the Xenos translator (XSH-CF-1) has no part in it. The partitions are read as the Python read
them: XML beside an unpacked partition (a folder with none converted whole; a partition without one reads as missing),
then the game copy's mesh index.

[code: rust/bc2-install/src/onslaught/shaders.rs:94-120] [code: rust/bc2-install/src/onslaught/shaders.rs:240-330] [code: rust/bc2-convert/src/shadercmd.rs:560-700] [code: opfor:src/tools/onslaught/console_shaders.py:38-55]
[test: rust/bc2-install/src/onslaught/shaders.rs::patterns_and_donors] [test: rust/bc2-convert/src/shadercmd.rs::needs_from_a_file]
[data: round OR10, 2026-11-27: the user's four levels, every work and game file of the Rust stage identical to the Python's, the merged databases among them]

### XCONS-ONS-8: The Mark V stage gives the retired Python's work files; its base and its texture check

Status: **verified** · Variants: Xbox 360

Stage 8 (`console_markv.py`'s build, `bc2_install::onslaught::markv` since round OR11) gives the retired Python's
work files byte for byte: the boat's console physics (`bc2_convert::havok::convert` on the executable's hkClass
layouts), each mesh LOD with each part in its donor material's PC vertex layout (the console's fifth two-half element
left out), the mesh set with the PC LOD sizes, the textures the level lacks (the user's PC bundles first, else the
console's converted), the boat's partitions, its nine materials' entries cloned from PC donors into the level's Dx9 /
Dx10 / Dx11 databases, its spawn grafted back from the level before stage 5, and the packed bundle. Decided against
the Python, where it was an accident of the code and not DICE data:

- **The base** is the bundle the install ledger hands the stage (the level before this stage's own install), for the
  shader databases and the "level has it" texture check alike. The Python read the game copy's bundle, which on a
  redo already holds the boat's entries and textures.
- **The texture check** takes each material's texture names with that material's own renames, as its cloned entry
  names them. The Python applied each material's renames to every name collected so far, so a later material's
  rename could hide an earlier material's texture (Main's `Humvee/HumveeWreck` was never checked). On the user's
  level both name sets find the same textures.
- **The PC bundles searched** for missing textures are the stock ones: a folder Project OPFOR installed a level into
  (its install record `opfor-<name>-installed.json`: the WW2 levels, `ww2_common`) is not BFBC2 data, as the suite's
  stock mesh index already rules (XCONS-ONS-10).
- **The ledger's check comes before the build**, as the Python's own comment said ("refused before the build,
  nothing written") but its code did not: it built first, so a bundle changed outside the ledger stopped in the
  build's reader (a damaged bundle's `ValueError`) instead of in the ledger's refusal. Either way nothing is written
  to `game\` and the work folder is dropped.

[code: rust/bc2-install/src/onslaught/markv.rs:489-520] [code: rust/bc2-install/src/onslaught/markv.rs:640-680] [code: rust/bc2-install/src/onslaught/files.rs:9-21] [code: opfor:src/tools/onslaught/console_markv.py:37-48]
[test: rust/bc2-install/src/onslaught/markv.rs::restore_spawn_grafts_and_copies_the_fields_that_named_it] [test: rust/bc2-install/src/onslaught/markv.rs::console_layouts_find_the_record_after_its_guid]
[data: round OR11, 2026-11-27: the user's ONS_MP_002 (Valparaiso), stages 8 - 10 chained, every work and game file of the Rust stages identical to the Python's (rust/BENCHMARKS.md "OR11")]

### XCONS-ONS-9: The input stage gives the retired Python's work files

Status: **verified** · Variants: Xbox 360

Stage 9 (`console_input.py`'s build, `bc2_install::onslaught::input` since round OR11) gives the retired Python's
work files byte for byte: every seat whose `InputActionMaps` names the console's input set takes the PC set of the
same kind (`InputActionMaps` null, the PC `InputConceptDefinition` and `InputMapping`); a console vehicle's seats take
the input of their PC version's seat with the same `EntryOrderNumber`, the Mark V (no PC version) DICE's role rule
(the driver the vehicle set, a seat on a branch with a weapon the vehicle concepts with the T-90 gunner's mapping
from mp_common, any other the soldier set); a seat without input is the AI's and keeps its entry radius; the Mark V's
player seats are entered from 5 m, as the PC patrol boat's; the console input partitions leave the bundle unless
something still names them. DbxEdit's XML pass over the unpack is not checked for its return code, as in the Python
(a partition it cannot read has no XML and is left as it is). Nothing was decided against the Python.

[code: rust/bc2-install/src/onslaught/input.rs:121-175] [code: rust/bc2-install/src/onslaught/input.rs:236-292] [code: opfor:src/tools/onslaught/console_input.py:54-79]
[test: rust/bc2-install/src/onslaught/input.rs::an_ai_seat_and_its_radius_stay] [test: rust/bc2-install/src/onslaught/input.rs::the_role_rule_without_a_counterpart]
[data: round OR11, 2026-11-27: the user's four levels, every work and game file of the Rust stage identical to the Python's (rust/BENCHMARKS.md "OR11")]

### XCONS-ONS-10: The dependency stage gives the retired Python's work files; stock sources only

Status: **verified** · Variants: Xbox 360

Stage 10 (`console_deps.py`'s build, `bc2_install::onslaught::deps` since round OR11) gives the retired Python's work
files byte for byte: every partition the level references that no bundle it loads holds (its Package and Dist level,
ingame and loader bundles, mp_common, sp_common, the startup bundles), found in the PC level bundles (the PC map's
own first, Package before Dist), copied in with its resources by name, round after round until nothing is missing,
and the manifest's lines for them. Decided against the Python:

- **Stock sources only**: a level folder holding Project OPFOR's install record (`opfor-<name>-installed.json`: the
  WW2 levels) is not searched. Its partitions are BF1943 conversions, not the stock PC data the stage means to copy;
  the user's own October install, which the port was made on, had none. With them installed the Python copied
  766 - 837 files from them into each of ONS_MP_004 / 005 / 008 (BF1943's Thompson, WW2 builds of BFBC2's 1P weapon
  animations the stock install has nowhere); the Rust's files are the same with or without them.
- **The level's own bundle** is read as the bundle the ledger hands the stage, not the game copy: on a redo the game
  copy already holds the copies this stage made, so the Python found nothing missing and its rebuilt bundle (from
  the ledger's source) lost them.

[code: rust/bc2-install/src/onslaught/deps.rs:181-200] [code: rust/bc2-install/src/onslaught/deps.rs:218-232] [code: rust/bc2-install/src/onslaught/files.rs:9-21] [code: opfor:src/tools/onslaught/console_deps.py:31-39]
[test: rust/bc2-install/src/onslaught/deps.rs::the_first_name_with_a_prefix_in_insertion_order]
[data: round OR11, 2026-11-27: the user's four levels on the stock install, every work and game file of the Rust stage identical to the Python's (rust/BENCHMARKS.md "OR11")]
### XCONS-ONS-11: The cull stage gives the retired Python's work files; culltree runs as its own command

Status: **verified** · Variants: Xbox 360

Stage 11 (`console_cull.py`'s build and apply, `bc2_install::onslaught::cull` since round OR12) gives the retired
Python's work files and write byte for byte: the installed level bundle unpacked, `bfbc2 culltree add --auto
--misplaced 2.0` run on its exclusive tree and level against the PC map's Linux level and every `work\levels\*` folder
(glob's order: dot names left out, paths sorted as strings), the first and last report lines printed, the report kept
as `add_report.txt` (UTF-8, CRLF), and when the tool built a tree, both partitions copied into the unpack with their
manifest sizes and the bundle packed. Nothing to add is "already applied", before the ledger's check (the Python's
order: a stage refused by a later write still reports that nothing is to add). The cull tool runs as the command line
OPFOR names (`cli_command('culltree')`), as the Python ran it: its library is in bc2-levelwrite, which depends on the
installer, so the installer cannot call it in process. Its output is read as UTF-8 (the Python read it in the ANSI code
page, a difference only for a non-ASCII blueprint name; DICE's names are ASCII).

[code: rust/bc2-install/src/onslaught/cull.rs:139-182] [code: rust/bc2-install/src/onslaught/cull.rs:184-211] [code: opfor:src/tools/onslaught/console_cull.py:42-58]
[test: rust/bc2-install/src/onslaught/cull.rs::levels_and_paths]
[data: round OR12, 2026-11-27: the user's four levels, every work and game file of the Rust stage identical to the Python's]

### XCONS-ONS-12: The loading stage gives the retired Python's work files

Status: **verified** · Variants: Xbox 360

Stage 12 (`console_loading.py`'s build, `bc2_install::onslaught::loading` since round OR12) gives the retired
Python's work files and writes byte for byte: the console level's `LoadingMovie` partition path (its first part
capitalised, the rest upper case, as the console bundles spell it; the level's XML written beside it first when it
has none), the console loader (the level's own extract, else the earlier u3 extract), the movie's partition added to
the level bundle, partition and Bink resource to the loader, the partition to `startup-00` beside the description, the
level's and description's `LoadingMovie` and the description's `GameMode` (OPFOR's `GameModes/Onslaught`, added to
`startup-00`) pointed at them, and only the changed bundles packed in level, loader, startup order. An entry already
present is never added twice; a run with nothing to change is "already applied". The ledger's layers of `startup-00`
(one per level) and the reverts stay OPFOR's.

[code: rust/bc2-install/src/onslaught/loading.rs:88-123] [code: rust/bc2-install/src/onslaught/loading.rs:149-215] [code: opfor:src/tools/onslaught/console_loading.py:28-41]
[test: rust/bc2-install/src/onslaught/loading.rs::movie_in_startup] [test: rust/bc2-install/src/onslaught/loading.rs::entries]
[data: round OR12, 2026-11-27: the user's four levels, every work and game file of the Rust stage identical to the Python's]

### XCONS-ONS-13: The HUD stage gives the retired Python's work files; its movie edits in process

Status: **verified** · Variants: Xbox 360

Stage 13 (`console_hud.py`'s apply and `--loading-screen`, `bc2_install::onslaught::hud` since round OR12) gives the
retired Python's work files and writes byte for byte: `ingame-00`'s `hudlayer_shared` (the console timer),
`mp_objectives` (`getMapDesc`) and `hud_root` (onslaught as conquest), `loader-00`'s `loadingscreen_mp` (headline,
objective, goldrush's team name, tdm's panel), each built on the bundle the ledger names (its own prior only for a redo),
a movie that is this stage's own patched result put back to the original kept at the last apply first (never patched
twice; a lost or changed original refuses), an install from before the entry records given one from the file before
it, the record (`json.dumps(indent=1)`) and the kept originals written only after the ledger's commit. The edits are
`bc2_menu::graft`'s in process, where the Python ran `bfbc2 aptgraft`'s commands (the same library): each step's line
is the one the command printed last. A movie the Rust model cannot read now refuses with the model's words; `bfbc2
aptgraft` handed such a movie to the retired `aptgraft.py`, whose own reading the menu tools' rounds keep only as a
test reference (OR14: OPFOR's movie tools are `bc2_native.AptMovie`). A failing step names its command
(`aptgraft <command> ...`) and the model's message rather than the binary's path and its error report.

[code: rust/bc2-install/src/onslaught/hud.rs:266-282] [code: rust/bc2-install/src/onslaught/hud.rs:317-380] [code: opfor:src/tools/onslaught/console_hud.py:67-92]
[test: rust/bc2-install/src/onslaught/hud.rs::never_patches_twice] [test: rust/bc2-install/src/onslaught/hud.rs::records_and_names]
[data: round OR12, 2026-11-27: the user's four levels, a redo on the entry records and the main menu's loading screen, every work and game file of the Rust stage identical to the Python's]

## Open questions

- 0x98BD6E on ONS_MP_004's console road / stop signs: which vertex layout or program the renderer lacks (layer 3,
  rendering).
