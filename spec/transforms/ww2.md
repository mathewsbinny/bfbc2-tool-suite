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
[doc: rust/BENCHMARKS.md:1598-1606]

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
hashes and the checks. It writes under `work\ww2` only and installs nothing.

[code: opfor:src/tools/ww2/rebuild_all.py:175-200]
[test: opfor:src/tools/tests/test_ww2_build.py::test_rebuild_step_order] [test: opfor:src/tools/tests/test_ww2_build.py::test_every_install_root_has_a_producer]
[test: opfor:src/tools/tests/test_ww2_build.py::test_from_selects_exactly] [test: opfor:src/tools/tests/test_ww2_fidelity_gate.py::test_failed_fidelity_gate_stops_rebuild_before_hashes_and_qa]

### XWW2-RB-2: The oracle: the rebuild is deterministic

Status: **observed** · Variants: BF1943

The rebuild's result is the sha1 of every output under `work\ww2` (`hashes.json`); a change to the WW2 builds or to
the suite is accepted as "the same outputs" only when a full rebuild gives every recorded sha1 again. On 2026-11-19
the rebuild on the suite's Rust core gave 10,852 of 10,852 outputs identical to the oracle, also on the suite's round
RT1 branch.

[code: opfor:src/tools/ww2/rebuild_all.py:141-168]
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
