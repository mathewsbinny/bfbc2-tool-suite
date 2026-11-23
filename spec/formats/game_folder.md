# The game folder: the files outside the bundles

Most of BFBC2 is in its bundles (`formats/fbrb.md`). This file covers everything else DICE ships in the game folder:
which kinds of file there are (the census, `spec/CENSUS.md`), the ones with their own format here, and the ones the
specification leaves out on purpose. "DICE's" means dated before 2013, as every probe of `spec/probes/` takes it;
OPFOR's changed files are recognised by their `.opfor-orig` backups, which hold DICE's original.

## 1. The kinds

### GFILE-KIND-1: Sixteen kinds of file

Status: **verified** · Variants: PC

DICE's game folder holds 1,317 files of 16 kinds (`spec/probes/census.py`'s FILE_KINDS):

| Kind | Files | Specified in |
|---|---|---|
| bundles (`Dist/win32/**.fbrb`, `Package/**.fbrb`) | 1,022 | `formats/fbrb.md` |
| the bundle registries `Dist/win32/package.mft`, `bundleManifest` | 1 + 1 | `model/registration.md` |
| NavKit path data `Output/win32/ai/levels/<level>/{infantry,vehicle}/pathdata.hpd` | 23 | `formats/hpd.md` |
| movies `Output/win32/movies/**.res` | 33 | BINK-OUT-1 |
| the aerial height map `Output/win32/levels/sp_006/AerialHeightmap.res` | 1 | GFILE-AHM-1 |
| `BFBC2Game.exe` | 1 | `engine/` (ENG-TI-1 for its type information) |
| `database.dbmanifest` | 1 | GFILE-DBM-1 |
| `dfe` | 1 | GFILE-DFE-1 |
| `InstallationData.xml`, `UpdateHistory.log` | 1 + 1 | GFILE-INST-1 |
| `dialoglogo128x128.jpg` | 1 | GFILE-LOGO-1 |
| `binkw32.dll` | 1 | BINK-DLL-1 |
| the DirectX redistributables (`D3DCompiler_42.dll`, `D3DX9_42.dll`, `d3dx10_42.dll`, `d3dx11_42.dll`) | 4 | out of scope |
| `winui.dll` | 1 | out of scope |
| the `Support` folder | 224 | out of scope |

Out of scope, with the census's reasons: the DirectX files are Microsoft's runtime of the 2010 SDK, loaded as any
Windows program loads them; `winui.dll` is an MFC 8 dialog library that no file of the folder names (not imported by
the executable or `binkw32.dll`); `Support` is EA's help pages, the EULA, the uninstaller, the CD-key tool and the
Windows Game Explorer resource (`GDFBinary.dll`, `GL.ini`). The game tier re-measures the kinds and fails on a new
one (`census` checks); the quick tier fails when a kind has neither a rule nor an out-of-scope entry.

[test: core/tests/test_spec_probes.py::census]
[test: spec/probes/census.py::check]
[data: spec/probes/census.py --measure on the game copy, 2026-11-23 (spec/census.json): the kinds and counts above]

## 2. Files with a format here

### GFILE-AHM-1: The aerial height map: a 28-byte header and 16-bit heights

Status: **verified** · Variants: PC

A level whose `LevelData.HasFlyingAI` is set gets `Output/win32/levels/<level>/AerialHeightmap.res` beside its
NavKit data: the flying AI's height and line-of-sight grid (the executable's `AerialHeightmap::getGroundHeight`,
`AerialHeightmap::isFreeLineOfSight`, the `Aidll` module's `AerialNavigation`). DICE ships one, SP_006's, the only
level with `HasFlyingAI` true (its `AerialHeightmapCellSize` 8, `AerialHeightmapQuality` 1; the other 73 level
partitions keep 4 / 1 or 2 / 4 and no file). No RES header; little-endian:

| Offset | Type | Content |
|---|---|---|
| 0 | f32 | the cell size in metres (8.0: the level's `AerialHeightmapCellSize`) |
| 4, 8 | 2 x u32 | width and height in cells (512 x 512) |
| 12, 16 | 2 x f32 | the grid's minimum x and z (-2,048, -2,048: the grid covers exactly 4,096 m) |
| 20 | f32 | the height scale (metres per step) |
| 24 | f32 | the height offset |
| 28 | w x h x u16 | heights, height = value x scale + offset; the file ends after them |

The loader 0x59A140 builds `<level folder>/AerialHeightmap.res` and opens it through the file system's open call; it
runs from 0x5AF6D3 only when a flag of its owner (+0x3AD) is set (inferred: `HasFlyingAI`).

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: 1 file, cell 8.0, 512 x 512, origin (-2,048, -2,048), 28 + 2 x 512 x 512 = 524,316 bytes; DICE's 74 LevelData: HasFlyingAI true only in SP_006 (cell size 8, quality 1)]
[exe: BFBC2Game.exe 0x59A140: push "/AerialHeightmap.res" (0x1419CB8), concatenate (0x45AEC0), open through [0x154C444] vtable +0x80; caller 0x5AF6D3 behind `cmp byte [edi+0x3AD], 0`; strings AerialHeightmapCellSize 0x1460EE0, AerialHeightmapQuality 0x1460EF8]

### GFILE-DBM-1: `database.dbmanifest` is DICE's editor database manifest

Status: **verified** · Variants: PC

The game root's `database.dbmanifest` (1,868 bytes, XML, UTF-8) is byte for byte the `<non-resource>` entry of the
same name in both `build_overlay` bundles (RES-NAME-2). It describes DICE's content database, not the game's data:
a root `Database` element (`id`, `displayName`), `Settings` (one `Setting`), `Schema` (`Constraints` from the tools'
source tree), seven `Domain` elements, an `Import`, a `TypeMap` of six `Type`s, four `Remap`s, `Pipelines` and
`EmergencyBindings`. The executable names the file once, constructing an object with it at start-up (0x4BA015 ->
0x4A8810, kept at +0xA0); whether that object reads the content is not traced. The suite copies the file and never
writes it.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: root element Database; the root file equal to build_overlay's entry in 2 of 2 bundles]
[exe: BFBC2Game.exe "database.dbmanifest" 0x14100BC, pushed at 0x4BA015 for 0x4A8810]

### GFILE-DFE-1: `dfe` is 1,000 opaque bytes no game file names

Status: **observed** · Variants: PC

`dfe` in the game root is exactly 1,000 bytes of near-uniform data (8 bits of entropy per byte to the precision 1,000
bytes allow: encrypted or random), dated with DICE's files. No file of the game folder names it (`BFBC2Game.exe`,
`binkw32.dll`, `winui.dll` and the `Support` executables hold no `dfe` string), so the game does not read it; it is
inferred to belong to EA's distribution (the download / activation client that installed the copy). The suite leaves
it alone.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: 1,000 bytes, entropy above 7.5 bits per byte; no `dfe` name in BFBC2Game.exe, binkw32.dll, winui.dll or the Support executables (round SPG4 search)]
[inferred: a 1,000-byte random-looking file in the root that the game never opens is a distribution client's token or key]

### GFILE-INST-1: `InstallationData.xml` and `UpdateHistory.log` are the patcher's record

Status: **verified** · Variants: PC

`InstallationData.xml` holds one `InstallationData` element with `CurrentVersion` (795745: the build of
`BFBC2Game.exe`, NET-VER-1's `ROMEPC795745`), `PatcherVersion`, `EulaVersion`, `PbEulaVersion` (PunkBuster's EULA),
`Sku` and `InstalledHashes`; `UpdateHistory.log` has one line per patch applied: a date and time and
`/patch_<from>_<to>` (DICE's copy: one line, 515195 -> 795745, the final PC patch). Neither name occurs in the
executable: they are written and read by EA's patcher (not in the game folder), so the game's own version check is
the network one (NET-VER-1 - 3). A tool that reports the installed version reads `CurrentVersion`.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: InstallationData's six elements, CurrentVersion 795745; UpdateHistory one patch line 515195 -> 795745; neither name in BFBC2Game.exe]

### GFILE-LOGO-1: `dialoglogo128x128.jpg` is a 128 x 128 JPEG nothing in the folder names

Status: **verified** · Variants: PC

A 128 x 128 baseline JPEG (21,369 bytes) in the root. The executable does not name it (inferred: the EA patcher's or
launcher's dialog logo, like GFILE-INST-1's files).

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: JPEG 128 x 128; "dialoglogo" not in BFBC2Game.exe]

## Open questions

- `dfe`'s owner (an EA distribution client's file is the reading; no client is in the folder to confirm it).
- Whether the start-up object built from `database.dbmanifest`'s name parses the file.
