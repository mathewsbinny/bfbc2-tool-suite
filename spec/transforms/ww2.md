# The WW2 port: Battlefield 1943 -> BFBC2, and its rebuild oracle

The Pacific War content of Project OPFOR is made from the user's own Battlefield 1943 (Xbox 360) files in two layers:

- **The suite's importer** (`bfbc2-bf1943-import`: `bf1943import.py`, `meshport`, `havokport`, `effectport`,
  `dbxport`; their cores in `rust/bc2-convert`): one BF1943 resource in, its BFBC2 PC form out (meshes and mesh sets,
  Havok 4.6.1 -> 5.5.0 physics, effects and emitter atlases, DBX partitions ported to BFBC2's classes; formats:
  `formats/havok.md` HAVOK-V461-1, `mesh.md`, `audio.md` WAVE-HDR-2). Outputs go under OPFOR's `work\`, never into a
  game folder.
- **OPFOR's WW2 builds** (`src\tools\ww2\`): about forty build steps that make the levels (`wake_level`,
  `ww2_level_build`), shaders, weapons, soldiers, kits, UI and sounds under `work\ww2`, run in a fixed order by
  `rebuild_all.py`, then installed by `wake_install.py` / `ww2_level_install.py` under the stage protocol
  (`transforms/install.md`).

Two kinds of oracle hold the port to the truth: **DICE's own ports** (BF1943 assets BFBC2 itself shipped, converted
and compared with BFBC2's files) for the importer, and **the recorded rebuild** (every output's sha1) for the builds.

## 1. The importer's oracles

### XWW2-HK-1: Physics: BF1943's Kurogane converts to BFBC2's own Kurogane

Status: **verified** · Variants: BF1943

BF1943's Kurogane physics, converted from Havok 4.6.1 to 5.5.0, equals BFBC2's own Kurogane object for object
(62 objects paired by walking pointers from the root) and field for field (floats within 0.1 %), except values the
game rebuilds at load and MOPP code lengths DICE rebuilt by a byte or two; the Zero and Corsair, which DICE
re-exported, pair all but subparts DICE merged. Converted MOPP code keeps BF1943's tree, and rays cast through it hit
what brute force over every triangle hits. Every byte of the WW2 side project's converted physics is accounted for by
a 5.5.0 field.

[code: rust/bc2-convert/src/havokport.rs:1-40] [test: bfbc2-bf1943-import/tests/test_havokport.py::main] [test: bfbc2-bf1943-import/tests/test_havokport.py::mopp_check]
[test: rust/bc2-convert/src/havokport.rs::refusals] [test: rust/bc2-convert/src/mopp.rs::ray_hits]

### XWW2-MESH-1: Meshes: BF1943 weapons BFBC2 shipped port to BFBC2's files

Status: **verified** · Variants: BF1943

BF1943 weapon models that BFBC2 also shipped (Type 100, Nambu, M1911A1, Mk 2 grenade, Shin Gunto) port to BFBC2's
own files: the same parts, counts, offsets, index buffer and bone palettes, every position exact and every normal
within 0.01 on LOD 0; at least 37 rigid mesh sets BFBC2 shipped write as its files. A port under a new namespace
(`Characters/WW2/`) gives the same LOD bytes and renames every name the mesh set holds.

[code: rust/bc2-convert/src/meshport.rs:27-45] [test: bfbc2-bf1943-import/tests/test_meshport.py::main] [test: bfbc2-bf1943-import/tests/test_meshport.py::renamed_port]
[test: bfbc2-bf1943-import/tests/test_meshport.py::rigid_sets]

### XWW2-PAR-1: The Rust ports give the reference's bytes

Status: **verified** · Variants: BF1943

Every mesh, physics and effect port gives the same bytes (or the same refusal) through the Rust core as through the
reference recorded before the change: 273 of 273 mesh ports, 202 of 202 physics ports, 713 of 713 effect outputs,
and seeded built inputs (console meshes, random packfiles cut short or changed, layout searches).

[test: bfbc2-bf1943-import/tests/test_ports_native.py::game] [test: bfbc2-bf1943-import/tests/test_ports_native.py::quick_havok] [test: bfbc2-bf1943-import/tests/test_ports_native.py::quick_mesh]
[doc: rust/BENCHMARKS_ARCHIVE_1.md:1598-1606]

### XWW2-PAR-2: The parity tools measure the importer against DICE's own ports, in Rust

Status: **verified** · Variants: BF1943

`bfbc2 dbxparity` compares every instance of the partitions both games ship (same GUID and type) field by field,
each value as the DBX text shows it, and classifies each difference (same, enum, kind, nested, renamed, value,
BF1943-only, DICE-only); `--predict` ports each weapon and vehicle partition three times with dbxport (on a fair pool
with the catalogue's class defaults never the partition's own, without the conventions, and with DICE's version in
the pool) and counts the DICE values each run gives. `bfbc2 portparity` converts every model both games ship (the
Porter's layouts, mesh data, mesh set and textures) and compares it with DICE's element by element (positions within
1e-4 m, normal frames within 0.25 degrees, the rest within 1e-3; pictures by PSNR at the smaller size). Their reports
and JSON are the retired Python tools' byte for byte (round PR1b-1d).

[data: the user's BF1943 and game copy, 2026-11-22: 2,353 partitions and 16,249 instances compared, 173 partitions predicted, 148 models (147 ported) - every report, JSON and console line the same SHA-256 as dbxparity.py / portparity.py at 0434c22; the 147 ports' LODs, mesh sets and textures identical to meshport.Porter's]
[test: bfbc2-bf1943-import/tests/test_parity_reports.py::main] [test: rust/bc2-cli/src/bf1943/argp.rs::every_recorded_answer] [test: rust/bc2-convert/src/meshconv.rs::converts_as_mesh_convert]
[code: rust/bc2-cli/src/bf1943/dbxparity.rs:151-260] [code: rust/bc2-cli/src/bf1943/portparity.rs:252-360] [code: rust/bc2-convert/src/meshport.rs:450-560]

### XWW2-PAR-3: A float2 vertex element is compared as two singles

Status: **verified** · Variants: PC

Format 0x02 is `float2`, 8 bytes (MESH-VTX-1). `portparity` reads it as two singles. The retired `portparity.py`
read it as three (its `DTYPE` table: 12 bytes, the next element's first four with it), an accident of the Python
decided against the game's format; no part the tool compares on the user's files has a float2 element at stream 0,
so the reports did not change.

[data: portparity over the 148 shared models with the Python's table changed to two singles: the same report, and its element reader was called 0 times for a float2]
[test: rust/bc2-cli/src/bf1943/portparity.rs::float2_is_two_singles] [code: rust/bc2-cli/src/bf1943/portparity.rs:44-64]

### XWW2-DBX-1: DBX ports take BFBC2's schema of the same class

Status: **verified** · Variants: BF1943

A BF1943 partition is ported onto a BFBC2 partition of the same class as its template: BFBC2's fields in BFBC2's
order, BFBC2-only fields added from the template, BF1943's values kept; DbxEdit reads the result and writes it back
unchanged. The template ranking read from an index makes the reference scan's choice and score on every call.

[test: bfbc2-bf1943-import/tests/test_dbxport.py::main] [test: bfbc2-bf1943-import/tests/test_dbxport_ranking.py::quick]

## 2. The rebuild

### XWW2-RB-1: A fixed order; the first failing step stops the run

Status: **verified** · Variants: BF1943

`rebuild_all.py` runs the WW2 builds in their documented order (BF1943's effects first, the level, the shaders'
full run before their clone pass, the weapon builds, soldiers, kits, weathering, pictures then names, the UI,
translation, the remaining builds the install takes, the emitter atlases, the fidelity gate last); every install
root has a producer; `--from` starts at exactly the named step; the first failing step stops the run before the
hashes and the checks. It writes under `work\ww2` only and installs nothing. Since round OR7 the order is the
declared order of a step graph: steps that touch none of each other's files run side by side, and every pair that
does keeps this order (XDEP-STEP-8, XDEP-STEP-9); the first failing step stops the run from starting more.

[code: opfor:src/tools/ww2/rebuild_all.py:128-156]
[test: opfor:src/tools/tests/test_ww2_build.py::test_rebuild_step_order] [test: opfor:src/tools/tests/test_ww2_build.py::test_every_install_root_has_a_producer]
[test: opfor:src/tools/tests/test_ww2_build.py::test_from_selects_exactly] [test: opfor:src/tools/tests/test_ww2_fidelity_gate.py::test_failed_fidelity_gate_stops_rebuild_before_hashes_and_qa]

### XWW2-RB-2: The oracle: the rebuild is deterministic

Status: **observed** · Variants: BF1943

The rebuild's result is the sha1 of every output under `work\ww2` (`hashes.json`); a change to the WW2 builds or to
the suite is accepted as "the same outputs" only when a full rebuild gives every recorded sha1 again. On 2026-11-19
the rebuild on the suite's Rust core gave 10,852 of 10,852 outputs identical to the oracle, also on the suite's round
RT1 branch.

[code: opfor:src/tools/ww2/rebuild_all.py:256-275]
[data: rebuild_all on 2026-11-19 (main and the suite's RT1 branch): 10,852 of 10,852 outputs' sha1 identical to the recorded oracle]
[doc: opfor:research/WW2_PORT_PLAN.md:442-447] [doc: opfor:DEVELOPMENT_NOTES.md:1477-1481]

### XWW2-RB-3: Incremental rebuilds run what changed, and only that

Status: **verified** · Variants: BF1943

`rebuild_all --changed` runs only the steps whose code, inputs, listed folders or outputs changed since their
recorded run; a step whose rerun gives the same files does not rerun the steps after it; a step that read a file a
later step rewrote sends the run back to that file's writer; a failed step leaves no record and runs again.

[test: opfor:src/tools/tests/test_rebuild_changed.py::test_first_run_runs_all_then_nothing] [test: opfor:src/tools/tests/test_rebuild_changed.py::test_code_change_reruns_the_step_and_cuts_off_when_its_output_is_the_same]
[test: opfor:src/tools/tests/test_rebuild_changed.py::test_a_rewritten_input_sends_the_run_back_to_its_writer] [test: opfor:src/tools/tests/test_rebuild_changed.py::test_a_failed_step_has_no_record_and_reruns]

### XWW2-RB-4: The fidelity gate rejects lost source content

Status: **verified** · Variants: BF1943

The last step reads the producers' outputs and fails the rebuild (before hashes and QA) when source content was
lost: a flag's rig or animations, source anti-aircraft effects or recoil, water resources or bindings, overlay
states, or opaque programs where the source draws water or bunkers blended. It does not read the install staging.

[test: opfor:src/tools/tests/test_ww2_fidelity_gate.py::test_fidelity_gate_runs_last_after_all_its_producers] [test: opfor:src/tools/tests/test_ww2_fidelity_gate.py::test_fidelity_gate_rejects_missing_flag_skeleton_without_reading_staging]
[test: opfor:src/tools/tests/test_ww2_fidelity_gate.py::test_fidelity_gate_rejects_opaque_wave_and_bunker_programs]

### XWW2-MOD-1: The weapon tools' NumPy products are the reference machine's OpenBLAS kernels, written out

Status: **verified** · Variants: PC

`decimate.py` (the third-person LODs of the Type 38 and the Winchester 1897) and `shapes.py` (`Geo.moved`,
`Geo.tangents`, the windings of `_facing` / `outward` / `fix_winding`) compute with NumPy, whose matrix products
go through OpenBLAS. OpenBLAS picks its kernels by the processor (`DYNAMIC_ARCH`), and each kernel has its own order
of operations and fused multiply-adds, so NumPy's results differ in the last bit from one CPU to another (round
CI1's lesson): the recorded WW2 outputs are those of the machine that recorded the oracle (OpenBLAS 0.3.34, core
`SkylakeX` on an AMD Zen 4). The suite's Rust core (`bc2_convert::decimate`, `bc2_convert::shapes`, round
PR1b-2-rest2) does not call a BLAS: it writes those kernels' operations out - `ddot` under 16 elements as the first
product then a fused multiply-add per element; `x @ M` for a 4-vector as
`y_j = fma(x3, M3j, fma(x2, M2j, fma(x0, M0j, x1 M1j)))`; `(n, 3) @ (3, 3)` for n >= 2 as
`fma(a2, b2, fma(a1, b1, a0 b0))` and for n = 1 as `fma(a2, b2, fma(a0, b0, a1 b1))`; NumPy's sums of three columns
as `((0 + x0) + x1) + x2`; every product is added to a zeroed output, so a zero result is +0 - so it is deterministic on any processor and gives the oracle's bits. The NumPy originals
are kept (`*_reference`) and compared bit for bit where NumPy runs those kernels; elsewhere the recorded digests are
the check.

[test: bfbc2-model-tools/tests/test_decimate.py::parity] [test: bfbc2-model-tools/tests/test_decimate.py::shapes_parity]
[test: rust/bc2-convert/src/decimate.rs::blas_forms] [code: rust/bc2-convert/src/decimate.rs:54-68] [code: rust/bc2-convert/src/shapes.rs:33-62]
[data: 200,000 ddot and 100,000 quadric costs, 3,000 dgemv vectors, dgemm rows of n = 2 ... 1,000: all equal to NumPy on the reference machine]

### XWW2-PIC-1: The loadout pictures' renderer is NumPy's float64 order, its dgemv written out

Status: **verified** · Variants: PC

The WW2 weapons' loadout pictures (OPFOR's `pictures.py`, the Rust core since round OR4: `bc2_convert::pictures::weapon`)
are drawn in `float64` in NumPy's order of operations: per triangle the barycentric weights of the box's pixel centres,
the depth test, the normal (interpolated, or the face's cross product) normalised as `sqrt((x0^2 + x1^2) + x2^2)` and
turned to the camera; the hit pixels' `n @ light` / `n @ half` as OpenBLAS's `dgemv` gives them - for one pixel
`fma(a2, b2, fma(a1, b1, a0 b0))`, for more `fma(a2, b2, fma(a0, b0, a1 b1))`; `pow` the C runtime's. The supersampled
means: the colour summed over the block in row order from +0, the coverage as NumPy's pairwise rows (each row from -0,
then added from +0); the colour bled into the transparent texels six times over the four neighbours (wrapping, as
`np.roll`). The DXT5 encoding stays `texwrite`'s.

[code: rust/bc2-convert/src/pictures/weapon.rs:65-73] [code: rust/bc2-convert/src/pictures/weapon.rs:213-278]
[test: bfbc2-model-tools/tests/test_pictures_native.py::renderer]
[data: round OR4: 34 loadout pictures x 3 sizes and 30 seeded part sets identical to the retired NumPy renderer (Project OPFOR test_ww2_pictures_parity, its recorded digests)]

### XWW2-PIC-2: Pictures are scaled as Pillow scales them

Status: **verified** · Variants: PC

The loading and map pictures and the weapon icons are scaled as Pillow 12's `resize` (`Resample.c`) scales them: per
output sample the filter's weights (Lanczos of 3 lobes, box of 0.5) over `[center - support, center + support)`
rounded to whole samples, normalised, in 22-bit fixed point rounded away from zero; each pass sums from `1 << 21` and
clips `>> 22` to 0 - 255; the horizontal pass first, over the rows the vertical pass reads. RGBA is premultiplied first
(`MULDIV255`) and divided back after (`255 c / a`, kept where `a` is 0 or 255).

[code: rust/bc2-convert/src/pictures/resample.rs:64-102] [code: rust/bc2-convert/src/pictures/resample.rs:177-206]
[test: bfbc2-model-tools/tests/test_pictures_native.py::resize_vs_pillow]
[data: round OR4: 700 seeded pictures of 1, 3 and 4 channels and the four levels' loading and map pictures identical to Pillow's]

### XWW2-PIC-3: The written pictures are checked against DICE's form

Status: **verified** · Variants: PC

A loading picture is read back as DICE's: version 3, DXT1, flags 1, 1280 x 720, one level, 460,956 bytes wrapped, and
at least 30 dB against its source; a map picture DXT5, flags 1, 512 x 256, one level, 131,228 bytes wrapped, at least
28 dB over its 295 x 196, its alpha 250 or more there and 0 from four texels past it. The PSNR's mean is NumPy's
pairwise sum of the squared differences.

[code: rust/bc2-convert/src/pictures/loading.rs:114-188] [code: rust/bc2-convert/src/pictures/mod.rs:97-164]
[test: bfbc2-model-tools/tests/test_pictures_native.py::loading]

### XWW2-PIC-4: The preview pages' textures are PNG files of the same pixels

Status: **implemented** · Variants: PC

OPFOR's three.js preview pages (`ww2_attachments.html`, `ww2_scopes.html`; `meshview.page()` until round OR4) hold each
part's colour texture as a PNG file. The page is the Python's character for character but for those files: the `png`
crate encodes them instead of Pillow, so their bytes differ and their pixels do not. A preview is neither game data nor
a rebuild output (the oracle leaves `work\ww2\preview` out), so no DICE rule decides it: the pixels are the rule.

[code: rust/bc2-convert/src/meshview/page.rs:223-270]
[test: bfbc2-model-tools/tests/test_pictures_native.py::page]
[data: round OR4: the attachments page of 13 textures under two titles: the text identical, every texture's pixels identical]

### XWW2-PIC-5: A vehicle's side silhouette is Pillow's polygon fill, equal depths drawn in face order

Status: **verified** · Variants: PC

The HUD silhouettes OPFOR draws from a vehicle mesh (`type95_icon.silhouette`: the Type 95 boat, the Onslaught boats of
`console_icons`; the Rust core since round OR14: `bc2_convert::pictures::side`) fill each projected triangle as Pillow
12's `ImageDraw.polygon` on an 8-bit picture: the vertices truncated toward zero (C's `(int)`), the span ends
`ROUND_UP` / `ROUND_DOWN` of single-precision crossings, an edge's last row counted twice unless it is the scan's
last, horizontal edges merged and drawn, a degenerate triangle still painting its flat edges. The faces' normals are
`np.cross` (each product rounded, then subtracted), normalised as `sqrt((x^2 + y^2) + z^2)`; the light term as
OpenBLAS's `dgemv` gives it (`fma(nz lz, fma(nx lx, ny ly))`; one face: `fma(nz lz, fma(ny ly, nx lx))`); the
supersampled sums per sub-row, row after row. The faces are drawn far to near by the sum of their three depths;
**faces of equal sums are drawn in their order in the mesh** (a stable sort). NumPy's default `argsort` (the unstable
x86-simd-sort on an AVX-512 processor) orders equal keys by processor and NumPy version, so no rule follows from it,
and DICE drew its own silhouettes by hand: the stable order is the rule. The pictures OPFOR ships - the Type 95 at
78 x 36, the Mark V and the jet ski - are the same with either order.

[code: rust/bc2-convert/src/pictures/polygon.rs:77-200] [code: rust/bc2-convert/src/pictures/side.rs:62-222]
[test: rust/bc2-convert/src/pictures/polygon.rs::random_polygons_are_pillows]
[test: rust/bc2-convert/src/pictures/side.rs::synthetic_meshes_match_the_python]
[data: round OR14: rust/parity_silhouette.py - 20,000 random polygons identical to Pillow 12.3; 85 game-mesh and 300 synthetic pictures identical to the Python with a stable argsort (67 / 84 differ from the default argsort only in pixels a tie can change); the Type 95, MRKV and JETS identical to the shipped Python]

### XWW2-PIC-6: The DXT encoders' principal axis is LAPACK's dsyevd as the oracle machine computes it

Status: **verified** · Variants: PC

`texwrite`'s DXT1 / DXT5 colour blocks take each block's end colours along the principal axis of its covariance
(every WW2 texture `texwrite` encodes). The axis is the last eigenvector of `numpy.linalg.eigh` (LAPACK `dsyevd`,
`UPLO='L'`) as NumPy 2.5 with scipy-openblas 0.3.34 computes it on the oracle machine (OpenBLAS's SkylakeX
kernels): one Householder reflector (`dsytd2`: `dlarfg`, `dsymv`, `ddot`, `daxpy`, `dsyr2`), `dsteqr`'s QL / QR
sweeps and sort, the reflector applied back (`dorm2r`, `dlarf1f`); LAPACK's own code rounding each operation in
source order, the BLAS kernels' `y += a * b` each one fused multiply-add (`dger`'s `alpha * y[j]` and `dsymv`'s
`alpha * x[j]` rounded first). Since round OR14 the suite's Rust core computes it so (`bc2_convert::eigh3`), the
same bits on any processor; NumPy's answer on another processor (no FMA, other kernels) may differ in the last bits.
DICE's textures were encoded by DICE's own tools, so no DICE data decides those bits: the oracle machine's are the
rule, and they keep every rebuilt texture's bytes.

[code: rust/bc2-convert/src/eigh3.rs:479-640] [code: rust/bc2-convert/src/texwrite.rs:1-20]
[test: rust/bc2-convert/src/eigh3.rs::numpy_axes_to_the_bit]
[data: round OR14: rust/parity_texwrite_axes.py - 10,000,000 random covariances, 8,017 degenerate ones and the blocks of 4,495 pictures (111,625,822 blocks) identical to numpy.linalg.eigh's axes; 0 encoded blocks differ]

### XWW2-FX-1: An effect's emitters are its level's copies, its templates the first BFBC2 documents naming a type

Status: **verified** · Variants: BF1943

BF1943 keeps an effect's emitter documents per level (`levels/<level>/emitters/...` and the fx folders'
`emitter_<size>/` folders alike); the port takes the level's own copy, else the first other level's in name order
(BF1943's Wake lacks the tree destruction's three emitters, which Guadalcanal, Iwo Jima and Coral Sea have). The
templates are DICE's small generic detonation, then each BFBC2 emitter document in name order that names a type still
needed in its bytes and holds it. Since round OR8 both are the Rust core's (`bc2_convert::effectport`); every effect
set the WW2 builds port gave the retired Python's files, report lines and records.

[code: rust/bc2-convert/src/effectport.rs:229-243] [code: rust/bc2-convert/src/effectport.rs:880-965]
[test: rust/bc2-convert/src/effectport.rs::level_copies_the_levels_own_else_the_first_other]
[test: bfbc2-bf1943-import/tests/test_ports_native.py::quick_templates]
[data: round OR8: bf1943_fx's 29 effects, the 4 material grids' 44 / 47 / 47 / 38, the 3 level builds' effects, the vehicles' 2: 1,361 files, every report line and record identical to the Python twin; 50 walks at 2 levels identical]

### XWW2-FX-2: A template partition the game copy lacks is named

Status: **implemented** · Variants: PC

The effect port's first template (`fx/explostions/detonations/generic/fx_detonation_s`) missing from the game copy
stops the port with `dbxport.MissingTemplate` naming it. The retired Python raised the reader's `DeletedEntry` for the
empty read (an accident of `dbx.parse(None)`): no DICE data decides an error's wording, the name of what is missing is
the rule. Every other damaged input (cut, emptied and flipped partitions and textures, a dropped partition, a cut
template, 300 cut emitter documents) gives the Python's outcome.

[code: rust/bc2-convert/src/effectport.rs:880-965]
[data: round OR8: 20 damaged effect inputs, 19 the Python's outcome (the exception's type and message, or the same files), the missing template decided here]

### XWW2-SND-1: The fields BFBC2's sound classes add are set as DICE set them

Status: **verified** · Variants: BF1943

A ported sound instance whose BF1943 version lacks a field BFBC2's class has gets DICE's value from its conversions of
BF1943's sounds (the 1,081 sound partitions both games ship, compared by guid): `SoundTemplate`'s
`PreventDuplicateSpawnFrameCount` 15 where BF1943's `SkipDuplicateSpawns` is true, else 3, and
`PreventDuplicateSpawnDistance` 10; every sound emitter's `HFDampingOcclusion` 1. Fields BF1943 has, and instances
without a BF1943 version, keep the template's.

[code: rust/bc2-convert/src/soundport.rs:93-108] [code: rust/bc2-convert/src/soundport.rs:139-158]
[test: rust/bc2-convert/src/soundport.rs::skip_duplicates_decides_the_frame_count]
[test: opfor:src/tools/tests/test_ww2_sound_fills.py::test_fills_rederived_from_dices_conversions]

### XWW2-SND-2: A wave whose audio the port puts in memory is not streaming

Status: **verified** · Variants: BF1943

The sound port converts every wave's audio into the PC Wave in memory (a streamed BF1943 wave's from its
`_waveDataSns_` entries), so the wave's `WaveAsset.IsStreaming` is written false: in DICE's data the flag is true
exactly when the audio is streamed (all 75 stock waves in Wake's bundle, all 793 shared pairs). A wave that does not
convert is reported (`FAILED`) with the Rust converter's reason, where the retired Python gave its `AudioError`'s; its
partition stays as ported.

[code: rust/bc2-convert/src/soundport.rs:212-238] [code: rust/bc2-convert/src/wavport.rs:172-233]
[test: rust/bc2-convert/src/soundport.rs::streamed_waves_in_memory]
[data: round OR8: the five sound ports of the WW2 builds (bf1943_fx, Wake's grid, Wake's level, the menu music, the stings): 566 files, logs and returns identical to the Python twin]

## 3. The install

### XWW2-INS-1: The WW2 install refuses before writing

Status: **verified** · Variants: BF1943

`wake_install --apply` registers last (REG-NEW-4) and refuses before the first write when a backup would be refused
(XINST-STG-2) or the model groups do not fit the members the level places (LEVEL-MG-4, the 0xAB4E58 crash); the
verification after the apply fails on the same; a revert refuses before changing anything; a shared patch that
raises changes nothing. Since round OR1 the game-copy side is the suite's installer (`bfbc2 install`, XINST-WW2-1).

[test: opfor:src/tools/tests/test_ww2_installer.py::test_apply_order_records_and_verify_failure] [test: opfor:src/tools/tests/test_ww2_installer.py::test_guard_refuses_a_polluted_backup]
[test: opfor:src/tools/tests/test_ww2_installer.py::test_gates_stop_before_the_first_write] [test: opfor:src/tools/tests/test_ww2_installer.py::test_revert_refuses_before_changing_anything]
[test: opfor:src/tools/tests/test_ww2_levels.py::test_a_failing_shared_patch_runs_no_installer]

## Open questions

- Shader translation (Xenos -> HLSL) as a transformation: its oracle is the Wake carrier draw; the translator's rules
  are in `formats/shaderdb.md` (XENOS) but its pre- and postconditions are not yet written here.
- Soldiers and hand-held weapons still draw with DICE donor shaders on BF1943 textures (OPFOR's queue); the oracle for
  their translated shaders is not set.
