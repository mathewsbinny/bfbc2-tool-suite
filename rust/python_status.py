r"""Every Python file of the suite with its port status: writes PYTHON_STATUS.md and python_status.json at the root.

    python rust\python_status.py            write both files (the same bytes on every run of the same tree)
    python rust\python_status.py --print    the Markdown on stdout, nothing written

TABLE below is the one place each tracked .py file is classified (its kind, status, Rust counterpart, callers,
retiring round and a one-line note). The non-blank line counts are counted from the files at each run, and the Rust
files that embed a Python file with `include_str!` (bfbc2.exe prints its usage from it, or keys a cache by its hash)
are found at each run; everything else is in the table. tests/test_repo.py fails when a .py file is not in the table
or the table names a file that is gone, so a new or deleted Python file needs its row here (then run this script).
Line counts change with every edit, so no check compares them: regenerate when the table changes.

The statuses (the rounds that retire them are the Python exit's, PX1-PX7, in tasks/registry.toml):
    live             only Python does this job
    partly           part of its job is ported; the note names what is left
    twin             a Python twin of Rust code, kept for parity, as a fallback where the Rust refuses, or for callers
    ported           Rust answers its job and the Python is a thin caller (a shim over bc2_native or bfbc2.exe)
    retired-pending  in a retired tool's folder, to be moved or deleted (RETIRED.md rule 1)
    stays            benchmarks and parity measurements against the old Python, build and dev scripts
    test             tests and their helpers stay Python until "Tests in Rust"
"""
from __future__ import annotations

import ast
import json
import os
import re
import subprocess
import sys
from typing import NamedTuple

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MD = 'PYTHON_STATUS.md'
JSON = 'python_status.json'

STATUSES = ('live', 'partly', 'twin', 'ported', 'retired-pending', 'stays', 'test')
TOOL = 'tool entry point'
LIB = 'library'
TEST = 'test'
HELPER = 'test helper or golden recorder'
BENCH = 'benchmark or parity script'
DEV = 'build or dev script'
SHIM = 'shim'
KINDS = (TOOL, LIB, TEST, HELPER, BENCH, DEV, SHIM)


class Row(NamedTuple):
    path: str
    kind: str
    status: str
    rust: str           # the Rust counterpart: crate::module or a bfbc2.exe command ('' if none)
    callers: str        # who imports or runs it (suite, tests, Project OPFOR's src)
    retiring: str       # the round planned before the Python exit (history; px_round() maps it to PX1-PX7)
    note: str           # one line ('' for a test: its docstring's first sentence is used)


def R(path: str, kind: str, status: str, rust: str, callers: str, retiring: str, note: str) -> Row:
    return Row(path, kind, status, rust, callers, retiring, note)


def T(path: str, note: str = '') -> Row:
    """A test file."""
    return Row(path, TEST, 'test', '', 'run_tests.py', 'Tests in Rust', note)


def H(path: str, callers: str, note: str = '') -> Row:
    """A test helper or a test's case module."""
    return Row(path, HELPER, 'test', '', callers, 'Tests in Rust', note)


NAV = 'bfbc2-auto-navkit'
HARN = 'bfbc2-automated-map-and-bot-test-harness'
IMP = 'bfbc2-bf1943-import'
CONV = 'bfbc2-console-convert'
L1 = 'bfbc2-map-editor/placement/scripts'

# The classifications (state of the suite's main at the QW3 merge, 49f79c3, 2026-11-22; NK4 / NK5 are changing
# bfbc2-auto-navkit's rows as they port walkability and the stage). Callers: git grep of imports, script names and
# runs in the suite and in Project OPFOR BFBC2's tracked files; "tests N" counts the suite's test files.
TABLE: tuple[Row, ...] = (
    # --- bfbc2-anim-tools
    R('bfbc2-anim-tools/aimanim.py', LIB, 'ported', 'bc2_formats::aimanim; bfbc2 aimanim',
      'bf1943import; tests 2; OPFOR 2 (bf1943_weapons, weapon_preview)', 'PR1b-2',
      'references retired in RT1; parse / write / to_pc go through bc2_native; its docstring is the usage bfbc2.exe prints'),
    R('bfbc2-anim-tools/granny.py', TOOL, 'partly', 'bc2_formats::granny, bc2_convert::granny / granny_edit, '
      'bc2_py::granny::Granny', 'bf1943import, dbxport, meshport; tests 6; OPFOR 12', 'PR1b-2',
      "the Granny object is Rust's (PR1b-2-rest3: its bytes, tables, tree, field / array_at / put / to_bytes), "
      'curve_keys / sample too (PR1b-2-rest); left: GrannyReference (the Python reader and writer) and the '
      '*_reference twins, kept for parity'),
    T('bfbc2-anim-tools/tests/test_aimanim.py'),
    T('bfbc2-anim-tools/tests/test_granny.py'),
    T('bfbc2-anim-tools/tests/test_granny_curves.py'),
    T('bfbc2-anim-tools/tests/test_granny_f2a_native.py'),
    T('bfbc2-anim-tools/tests/test_granny_native.py'),
    T('bfbc2-anim-tools/tests/test_granny_port.py'),

    # --- bfbc2-auto-navkit (NK4 / NK5 in progress)
    R(f'{NAV}/navkit.py', TOOL, 'partly', 'bfbc2 navkit (bc2-cli navkit: driver, prepare, apply)',
      'hub, navkitGUI.pyw, navparity, navregress, harness; tests 23', 'NK5',
      'Rust answers every command but the in-game sample (it starts the game; all runs it as a child): build, all and '
      'apply on a NavKit base since NK5 (bc2_navbuild::build), sample --offline since NKS-deep C '
      '(bc2_navbuild::sampler); left: the GUI and the tests call its functions'),
    R(f'{NAV}/tools/detailed.py', TOOL, 'partly', 'bfbc2 detailed (bc2-cli navtools::detailed)',
      'navkit.py, navregress, navsample_offline, sample_all; tests 4', 'NK5',
      'the choice is Rust (O5); left: --score (evaluation only) and the library calls of build_inputs'),
    R(f'{NAV}/tools/hkpack.py', TOOL, 'twin', 'bc2_formats::havok packfile (hkpack_open); bfbc2 hkpack',
      'levelobjects, navsample_offline, collision; tests 4; OPFOR research 2', 'proposed: PR1b-2',
      "F4's read_value_reference / read_object_reference kept by ruling; Packfile keeps Python dicts"),
    R(f'{NAV}/tools/hpdcheck.py', TOOL, 'twin', 'bfbc2 hpdcheck (bc2-cli navtools::hpdcheck)',
      'navregress, navsample_offline, harness; tests 2', 'NK5',
      'the command line is Rust; the library check() stays for the regression builds'),
    R(f'{NAV}/tools/hpdtool.py', TOOL, 'twin', 'bc2_formats::hpd; bfbc2 hpdtool',
      '22 suite modules (navkit, NavKit tools, navdata); tests 16; OPFOR 1', 'NK5',
      'parse has no Python left (RT3); write_reference stays while the build hands it objects the core refuses'),
    R(f'{NAV}/tools/hpdview.py', TOOL, 'live', '',
      'none (bfbc2 hpdview checks arguments, then hands off)', 'proposed: PR1c (delete)',
      "matplotlib drawing of one sector; the editor's navmesh view shows the same; nothing calls it"),
    R(f'{NAV}/tools/kymesh.py', LIB, 'twin', 'bc2_navbuild / bc2_formats kymesh (kymesh_parse, write)',
      'navkit.py, navdetail, hpdtool, detailed, hpdcheck; tests 10', 'NK5',
      'parse_mesh_reference kept "for good" as the format reference (E3o); write_mesh_reference for refused objects'),
    R(f'{NAV}/tools/ladders.py', TOOL, 'twin', 'bfbc2 ladders (bc2-cli navkit::ladders)',
      'navkit.py, navregress, navwalk; tests 3', 'NK5',
      "the command line is Rust; extract() feeds navwalk's ladder re-floods"),
    R(f'{NAV}/tools/levelobjects.py', TOOL, 'twin', 'bfbc2 levelobjects; bc2_scene::collision (navkit_level_scan)',
      'navkit.py, navregress, navsample_offline, navwalk, ladders; tests 5', 'NK5',
      'the command line and the scan are Rust; the Python walk stays as fallback where the core refuses'),
    R(f'{NAV}/tools/navboundary.py', TOOL, 'live', '', 'tests 3', 'proposed: NKG1',
      "a cell-by-cell boundary diagnostic against DICE's path data; no Rust measure does it"),
    R(f'{NAV}/tools/navcompare.py', TOOL, 'twin', 'bfbc2 navcompare (bc2-cli navkit::navcompare)',
      'navkit.py, navparity; tests 3', 'NK5', 'the command line is Rust; compare() is a library of navparity'),
    R(f'{NAV}/tools/navdetail.py', LIB, 'ported', 'bc2_navbuild::detail (navkit_detail_features)',
      'tests 1; rust/parity_nk3_detail.py (a pinned checkout)', 'NK5',
      "NK3 retired the Python rule; the build's detail pass is the Rust stage's (NK5); features() for the tests"),
    R(f'{NAV}/tools/navdf.py', TOOL, 'twin', 'bc2_navbuild::parity (compare_sectors)',
      'navparity, navregress; tests 1', 'proposed: NKG1',
      'D / F parity measure; the comparison runs in Rust when built, the Python as reference'),
    R(f'{NAV}/tools/navdiff.py', TOOL, 'twin', 'bc2_navbuild::parity (navkit_area_cells)',
      'navdf, navparity, navsample_offline, navvehicle, navwater, navdata; tests 7', 'proposed: NKG1',
      "area cells in Rust (E3h); the layers' comparison and the PNGs are Python"),
    R(f'{NAV}/tools/navdomains.py', TOOL, 'twin', 'bc2_navbuild domains (DomainGraph)',
      'tests 4', 'proposed: A3',
      "cross-domain route prototype; dijkstra in Rust (E4e) with the Python as fallback; no tool calls it"),
    R(f'{NAV}/tools/navedits.py', SHIM, 'ported', 'bc2_navbuild::walk (edit_masks, region_mask_at)',
      'navwalk; tests 2', 'NK5',
      "NK4: the masks are Rust (matplotlib's crossing test ported); left: regions_of parses a side file's operations"),
    R(f'{NAV}/tools/navislands.py', LIB, 'ported', 'bc2_navbuild::graphfile::shadow_islands',
      'tests 1; rust/parity_nk2_file.py (a pinned checkout)', 'NK5',
      "NK2 moved the rule; prune / note_nodes for callers outside the build (the build's are the Rust defaults)"),
    R(f'{NAV}/tools/navnative.py', SHIM, 'ported', 'bc2_native (the E4e functions)',
      'levelobjects, navdf, navdiff, navdomains, navsample_offline, navvehicle, ordercheck; tests 2',
      'PR1b-2', "the switch between E4e's Rust and their Python references; goes with them (one backend registry)"),
    R(f'{NAV}/tools/navparity.py', TOOL, 'partly', 'bfbc2 navparity (bc2-cli navtools::navparity)',
      '17 suite modules (navregress, harness, NavKit tools); tests 5', 'NK5',
      'Rust answers argument errors and "no reference run"; left: run (the measures over many levels)'),
    R(f'{NAV}/tools/navregress.py', TOOL, 'live', 'bfbc2 navregress (arguments only)',
      'navkit.py, navboundary, navdf, navsample_offline, navvehicle, harness; tests 9', 'NK5',
      'the regression builds call navwalk.build (Rust since NK5); left: the run, its inputs and measures in Python'),
    R(f'{NAV}/tools/navsample.py', TOOL, 'partly', 'bc2_navbuild navsample (navkit_navsample_parse, _v3)',
      'navkit.py, navwalk, detailed, navparity, navsample_offline, navvehicle; tests 8', 'NK4',
      'load is Rust (versions 1-3; version 3 streamed, NK4); left: flood / compare and the info / compare command line'),
    R(f'{NAV}/tools/navsample_offline.py', TOOL, 'partly',
      'bc2_navbuild sampler / offline / hulls (navkit_offline_sample, navkit_offline_sample_level); bfbc2 navkit '
      'sample --offline', 'tests 6', 'proposed: NKS-deep C-rest',
      'the sampling (casts, capsule, terrain), the level geometry, like / validate / compare and stage in Rust (NKS-deep '
      "C: bfbc2 navkit sample --offline; the export's stage step runs it); left: the research drivers (parity, "
      'stockmp, build, check, all) and the asset_bodies reference the F3 / F4 tests call'),
    R(f'{NAV}/tools/navvehicle.py', TOOL, 'partly', 'bc2_navbuild vehicle (navkit_obstacle_distance)',
      'navdomains; tests 5', 'proposed: A3', 'obstacle distance (E4e) and stage_on_grid / write (NK5: '
      'navkit_stage_grid; NK5-rest: its settings as overrides of the Rust defaults) in Rust; left: the flood, the seeds '
      'and the command line'),
    R(f'{NAV}/tools/navwalk.py', LIB, 'partly', 'bc2_navbuild::walk, ::rules (navkit_walkable, navkit_walk_stage)',
      'navkit.py, navregress, navsample_offline, navislands, navvehicle, navwater_mesh, level; tests 12', 'NK5',
      "walkable, coarse and the rules are Rust (NK4); build / water_tops / outside_bounds / deep_water are thin calls "
      "of the Rust build (NK5-rest: navbuild.py's); left: score's command line"),
    R(f'{NAV}/tools/navwater.py', TOOL, 'live', '', 'navwater_mesh; tests 1', 'proposed: NKG1',
      "water depth against DICE's coverage (a measure); navwalk.water_tops is Rust since NK4 (bc2_navbuild::walk)"),
    R(f'{NAV}/tools/navwater_mesh.py', TOOL, 'live', '', 'navdomains; tests 4', 'proposed: NK5 (or A3)',
      "boats' navigation side file (prototype); its water heights through navwalk.water_tops (Rust, NK4)"),
    R(f'{NAV}/tools/ordercheck.py', TOOL, 'twin', 'bfbc2 ordercheck (bc2-cli navkit::ordercheck)',
      'navkit.py, navcompare, navparity, harness, levelinfo, stockmp config; tests 2', 'NK5',
      'the command line is Rust; its order_points / PathData are libraries of NavKit and the harness'),
    R(f'{NAV}/tools/sample_all.py', TOOL, 'live', 'bfbc2 sample_all (arguments only)', 'navkit.py; tests 0',
      'proposed: after NK5', "starts the game per level (Python by rule while sampling is navkit.py's)"),
    T(f'{NAV}/tests/test_apply.py'),
    T(f'{NAV}/tests/test_characters.py'),
    T(f'{NAV}/tests/test_collision_backends.py'),
    T(f'{NAV}/tests/test_detail_features.py'),
    T(f'{NAV}/tests/test_dynamic_props.py'),
    T(f'{NAV}/tests/test_havok_index.py'),
    T(f'{NAV}/tests/test_headroom.py'),
    T(f'{NAV}/tests/test_links.py'),
    T(f'{NAV}/tests/test_lips.py'),
    T(f'{NAV}/tests/test_navboundary.py'),
    T(f'{NAV}/tests/test_navbuild_native.py'),
    T(f'{NAV}/tests/test_navdomains.py'),
    T(f'{NAV}/tests/test_navedits.py'),
    T(f'{NAV}/tests/test_navislands.py'),
    T(f'{NAV}/tests/test_navkit_offline.py'),
    T(f'{NAV}/tests/test_navsample_offline.py'),
    T(f'{NAV}/tests/test_navtools_native.py'),
    T(f'{NAV}/tests/test_navwater_mesh.py'),
    T(f'{NAV}/tests/test_objective_floor.py'),
    T(f'{NAV}/tests/test_pathdata_native.py'),
    T(f'{NAV}/tests/test_reference_clip.py'),
    T(f'{NAV}/tests/test_regress_progress.py'),
    T(f'{NAV}/tests/test_roof_prune.py'),
    T(f'{NAV}/tests/test_tool_edges.py'),
    T(f'{NAV}/tests/test_traversal.py'),
    T(f'{NAV}/tests/test_vehicle.py'),
    T(f'{NAV}/tests/test_vehicle_level.py'),
    T(f'{NAV}/tests/test_water_rule.py'),

    # --- bfbc2-automated-map-and-bot-test-harness
    R(f'{HARN}/crash-reader/crashreaderlib.py', LIB, 'twin', 'bc2_harness (ExeHandle, crash log); bfbc2 crashreader',
      'exeref; tests 3; rust/bench_harness, bench_sweep', 'PR1b-2',
      'command line retired (RT3); Image keeps capstone for other executables'),
    R(f'{HARN}/crash-reader/exeref.py', TOOL, 'twin', 'bc2_harness::pe (ExeHandle); bfbc2 exeref',
      'tests 4; rust/bench_harness', 'PR1b-2',
      'Rust answers BFBC2Game.exe; the Python handles other executables (capstone) and is the hand-off'),
    R(f'{HARN}/crash-reader/harnesscore.py', SHIM, 'ported', 'bc2_harness through bc2_native',
      'crashreaderlib, ingame, checks; tests 1', 'PR1b-2', "the harness's native loader (one backend registry)"),
    R(f'{HARN}/harness.py', TOOL, 'partly', 'bfbc2 harness (bc2-cli harness: cli, runs)',
      'tests 5', 'PR1b-2, Tests in Rust', 'Rust answers everything but run (NumPy draws, NavKit builds)'),
    R(f'{HARN}/testharness/__init__.py', LIB, 'partly', '', 'harness.py; tests 3', 'PR1b-2',
      "the package of the harness's library (its modules' rows below)"),
    R(f'{HARN}/testharness/checks.py', LIB, 'partly', 'bc2_harness::graph (pair bits, cell index)',
      'harness.py; tests 3', 'PR1b-2', 'reachability pair bits and the ground cell index in Rust; the test kinds Python'),
    R(f'{HARN}/testharness/classify.py', LIB, 'live', '', 'harness.py, checks, levelinfo; tests 2', 'PR1b-2',
      'which level points are infantry points'),
    R(f'{HARN}/testharness/ingame.py', LIB, 'twin', 'bc2_harness (opfor.log sessions)',
      'harness.py, checks, levelchecks, runs; tests 2', 'PR1b-2',
      'sessions in Rust (E4f); sessions_reference kept (a faster Python algorithm, RT1)'),
    R(f'{HARN}/testharness/levelchecks.py', LIB, 'live', '', 'harness.py; tests 2', 'PR1b-2',
      "a level's AI-readiness checks from files"),
    R(f'{HARN}/testharness/levelinfo.py', LIB, 'live', '', 'harness.py; tests 3', 'PR1b-2',
      'what the harness reads about a level (points, AI layer, load set)'),
    R(f'{HARN}/testharness/runs.py', LIB, 'twin', 'bc2-cli harness::runs', 'harness.py; tests 2', 'PR1b-2',
      "saved runs and their comparison; bfbc2 harness show / compare / list answer in Rust"),
    T(f'{HARN}/crash-reader/tests/test_crash_native.py'),
    T(f'{HARN}/crash-reader/tests/test_crashreader.py'),
    T(f'{HARN}/crash-reader/tests/test_exeref.py'),
    T(f'{HARN}/tests/test_harness.py'),

    # --- bfbc2-bf1943-import
    R(f'{IMP}/bf1943import.py', TOOL, 'partly',
      'bc2_formats::bf1943 (Source: the bundle check, payload reads, RESTARTS); bc2-cli bf1943src / import; '
      'bfbc2 bf1943import',
      'meshport, texqa, renderqa, xdiff; tests 28; OPFOR 28', 'PR1b-1c',
      "Source reads through the Rust reader (PR1b-1-rest: restart points on disk); left: import / check (the "
      "Importer), the source index pickle, the Python reader kept as the reference"),
    R(f'{IMP}/dbxcatalogue.py', TOOL, 'partly',
      'bc2_convert::dbxcatalogue (catalogue_build) and its cache (catalogue_load); bfbc2 dbxcatalogue',
      'dbxport; tests 8; OPFOR 8', 'proposed: OPFOR in Rust stage 2',
      "the pass is Rust (F2b), the cache Rust's own file (PR1b-1e: the pickle retired); left: the Catalogue class "
      "OPFOR's WW2 modules and dbxport's Python ask, and build_reference (the reference)"),
    R(f'{IMP}/dbxparitylib.py', LIB, 'live', 'bc2-cli bf1943 dbxparity (instances, the fair pools: the same rules)',
      'tests 1 (test_dbxport_fair); rust/bench_bf1943; OPFOR 1 (qa_checks)', 'proposed: OPFOR in Rust stage 2',
      "the library half of the retired dbxparity.py (PR1b-1d: bfbc2 dbxparity answers alone): instances "
      "flattened field by field, the fair pools and catalogue"),
    R(f'{IMP}/dbxport.py', LIB, 'twin', 'bc2_convert::dbxport (port, rules)',
      'effectport; tests 9; OPFOR 10', 'PR1b-1c',
      'the partition port is Rust (E4d); port_python kept as reference and fallback'),
    R(f'{IMP}/effectport.py', LIB, 'ported', 'bc2_convert::effectport, bc2_convert::soundport (bc2_native.effectport_*, '
      'soundport_port)', 'tests 3; rust/bench_ports; OPFOR 9 (wake_level.port_sounds among them)', 'OR8 (done)',
      "BF1943 effects with their emitters and textures, and the sound porter (OPFOR's port_sounds): a wrapper over "
      'the Rust core since OR8, the Python twin retired after the parity proof (rust/BENCHMARKS.md "OR8")'),
    R(f'{IMP}/emitteratlas.py', LIB, 'twin', 'bc2_convert::emitteratlas',
      'texqa, texwrite; tests 3; OPFOR 3', 'PR1b-1c',
      'Rust since F2b; the Python kept (an unreadable chunk is recorded, float near-ties by design)'),
    R(f'{IMP}/havokport.py', LIB, 'twin', 'bc2_convert::havokport', 'tests 3; rust/bench_ports; OPFOR 5', 'PR1b-1c',
      'Rust since F2b; its Python reference waits for recorded goldens (RT1)'),
    R(f'{IMP}/importcore.py', SHIM, 'ported', "bc2_native (the importer's functions)",
      'dbxcatalogue, emitteratlas, havokport, meshport, mopp; tests 3', 'PR1b-2', "the importer's native loader"),
    R(f'{IMP}/meshport.py', LIB, 'partly',
      "bc2_convert::meshport (the searches; Porter for bfbc2 portparity and OPFOR's install ports)",
      'tests 4; OPFOR 5', "OPFOR in Rust stage 4 (the soldiers' and kits' renamed ports)",
      "searches in Rust; the Porter in Rust too (PR1b-1d; its shader plan and bare meshes OR2: OPFOR's install "
      "ports, bc2_install::levelbuild::ports); left: renames, donors and poses (OPFOR's soldier and kit builds)"),
    R(f'{IMP}/mopp.py', LIB, 'twin', 'bc2_convert::mopp', 'havokport; tests 3', 'PR1b-1c',
      'Rust since F2b; its Python reference waits for recorded goldens (RT1)'),
    T(f'{IMP}/tests/test_bf1943import.py'),
    T(f'{IMP}/tests/test_dbxcatalogue_native.py'),
    T(f'{IMP}/tests/test_dbxport.py'),
    T(f'{IMP}/tests/test_dbxport_fair.py'),
    T(f'{IMP}/tests/test_parity_reports.py'),
    T(f'{IMP}/tests/test_dbxport_native.py'),
    T(f'{IMP}/tests/test_dbxport_opfor.py'),
    T(f'{IMP}/tests/test_dbxport_ranking.py'),
    T(f'{IMP}/tests/test_dbxport_rules.py'),
    T(f'{IMP}/tests/test_dbxport_tables.py'),
    T(f'{IMP}/tests/test_emitteratlas.py'),
    T(f'{IMP}/tests/test_emitteratlas_native.py'),
    T(f'{IMP}/tests/test_havokport.py'),
    T(f'{IMP}/tests/test_physmat_native.py'),
    T(f'{IMP}/tests/test_havokport_native.py'),
    T(f'{IMP}/tests/test_meshport.py'),
    T(f'{IMP}/tests/test_ports_native.py'),

    # --- bfbc2-console-convert
    R(f'{CONV}/audio.py', TOOL, 'partly', 'bc2_formats::wave, bc2_convert XAS encoder; bfbc2 audio',
      'tests 7; OPFOR 2 (wake_level, wake_vo)', 'PR1b-1c',
      'Wave / SPS reading and XAS encoding in Rust; left: one ffmpeg process per sound (XMA decode), the writers\' '
      'references'),
    R(f'{CONV}/convert.py', TOOL, 'twin', 'bfbc2 convert (bc2-cli nav::convert)', 'tests 4; rust/bench_sweep',
      'PR1b-1c', 'the command line is Rust; the Python answers damaged input; manifest() used by tests'),
    R(f'{CONV}/havok.py', LIB, 'twin', 'bc2_convert havok / hkclasses (convert_wrapper)',
      'havokport, convert; tests 7; OPFOR 7', 'PR1b-1c', "F2a's references kept until recorded goldens retire them"),
    R(f'{CONV}/mesh.py', LIB, 'twin', 'bc2_formats::meshdata, bc2_convert mesh',
      '17 suite modules (meshport, meshview, renderqa ...); tests 9; OPFOR 18', 'PR1b-1c',
      'Rust for parse / write / relayout; element, pair_by_kind, strip_to_list still Python for callers'),
    R(f'{CONV}/meshset.py', LIB, 'twin', 'bc2_formats meshset', 'meshport, mesh, renderqa; tests 5; OPFOR 15',
      'PR1b-1c', 'write_reference kept for objects the core refuses'),
    R(f'{CONV}/soot.py', LIB, 'ported', 'bc2_convert terrain / converters (soot)', 'tests 2; OPFOR 2', 'PR1b-1c',
      'a thin call into the Rust core (F2a)'),
    R(f'{CONV}/terrain.py', LIB, 'twin', 'bc2_convert::terrain',
      'navparity, navsample_offline, soot, renderqa, level; tests 10; OPFOR 2', 'PR1b-1c',
      'Rust since F2a; road_positions / quadtree_nodes still Python; references until goldens; _Walk and '
      "ROAD_VERTEX retired (OR2: OPFOR's VisualTerrain walk is bc2_install::levelbuild::terrainfiles)"),
    R(f'{CONV}/texqa.py', TOOL, 'partly', 'bfbc2 texqa (bc2-cli texqa: the measures)',
      'tests 9; OPFOR 1 (qa_checks)', 'PR1b-1c',
      "the measures are Rust; left: DICE's copies, BF1943 sources, the PIL contact sheets, argparse errors"),
    R(f'{CONV}/texture.py', LIB, 'twin', 'bc2_formats texture (tiled, packed, layout)',
      '16 suite modules; tests 13; OPFOR 29', 'PR1b-1c',
      "tiled / packed / layout references kept by ruling: texqa's independent decoder"),
    T(f'{CONV}/tests/test_audio.py'),
    T(f'{CONV}/tests/test_audio_native.py'),
    T(f'{CONV}/tests/test_console_f2a_native.py'),
    T(f'{CONV}/tests/test_havok.py'),
    T(f'{CONV}/tests/test_havok_classes_native.py'),
    T(f'{CONV}/tests/test_mesh_decode.py'),
    T(f'{CONV}/tests/test_meshdata_native.py'),
    T(f'{CONV}/tests/test_soot.py'),
    T(f'{CONV}/tests/test_terrain.py'),
    T(f'{CONV}/tests/test_texqa.py'),
    T(f'{CONV}/tests/test_texture.py'),
    T(f'{CONV}/tests/test_texture_helpers_native.py'),
    T(f'{CONV}/tests/test_texture_native.py'),
    T(f'{CONV}/tests/test_trees.py'),
    T(f'{CONV}/tests/test_wave_native.py'),

    # --- bfbc2-console-readers
    R('bfbc2-console-readers/apt/apt.py', TOOL, 'twin', 'bc2_menu apt; bfbc2 apt',
      'aptcheck, aptgraft, aptrefslib, menucommon; tests 6', 'PR1c (with aptgraft)',
      "the command line is Rust; Movie is the menu tools' library (aptgraft's and aptcheck's; OPFOR reaches none of "
      'it since OR14)'),
    R('bfbc2-console-readers/stfs/stfs.py', TOOL, 'twin', 'bc2_formats stfs; bfbc2 stfs',
      'record_console_readers_answers, test_cli_errors; bfbc2 stfs (refusals)', 'proposed: PR1c (delete)',
      "OPFOR's console_onslaught / onslaught_art moved to bc2_formats::stfs (OR5); since OR14 test_console_readers"
      '(_native) run on its recorded answers (console_readers_answers.json) and import it no more: it can be deleted '
      '(left: bfbc2 stfs embeds its docstring and hands refusals to it, test_cli_errors runs it)'),
    R('bfbc2-console-readers/xiso/xisolib.py', LIB, 'twin', 'bc2_formats xiso; bfbc2 xiso',
      'record_console_readers_answers', 'proposed: PR1c (delete)',
      'command line retired (RT3); since OR14 test_console_readers(_native) run on its recorded answers '
      '(console_readers_answers.json) and import it no more: it can be deleted'),
    H('bfbc2-console-readers/tests/record_console_readers_answers.py', 'by hand (while stfs.py / xisolib.py exist)',
      "records stfs.py's and xisolib.py's answers for test_console_readers(_native); goes with the twins"),
    T('bfbc2-console-readers/tests/test_console_readers.py'),
    T('bfbc2-console-readers/tests/test_console_readers_native.py'),

    # --- bfbc2-culltree
    R('bfbc2-culltree/culltreelib.py', LIB, 'twin', 'bc2_levelwrite cull tree; bfbc2 culltree',
      'tests 4; bc2-cli cull (its hash keys the row cache)', 'PR1b-2',
      "command line retired (RT3); F4's string-table reference kept by ruling"),
    T('bfbc2-culltree/tests/test_culltree.py'),
    T('bfbc2-culltree/tests/test_culltree_e3m.py'),
    T('bfbc2-culltree/tests/test_culltree_native.py'),

    # --- retired tools' folders
    R('bfbc2-dbx-tools/dbxedit.py', SHIM, 'retired-pending', 'bfbc2 dbxedit (bc2_formats::dbxedit)',
      'hub; tests 9; OPFOR 3 (through bfbc2core.dbxedit)', 'PR1c',
      "DbxEdit's command line over bfbc2core.dbxedit; the folder's tools are retired (GR4)"),
    R('bfbc2-fbrb-tool/fbrbtool.py', SHIM, 'retired-pending', 'bfbc2 fbrbtool (bc2_formats::fbrb::tool)',
      'hub, navkit.py; tests 11; OPFOR 7 (bat / ps1 fallbacks)', 'PR1c',
      "FbrbTool's command line over bfbc2core.fbrbtool; OPFOR's scripts name it as their fallback"),

    # --- bfbc2-map-editor (L1's placement study)
    R(f'{L1}/l1_a1.py', DEV, 'stays', '', 'none', 'PY1 / PR1c',
      "writes l1_a1_params.json, which bc2_levelwrite::autolayout embeds"),
    R(f'{L1}/l1_aabb.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'one-off check of AABB capture areas'),
    R(f'{L1}/l1_analyze.py', DEV, 'stays', '', 'none', 'PY1 / PR1c', 'writes l1_distributions.md'),
    R(f'{L1}/l1_anchor.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'one-off check of spawner anchors'),
    R(f'{L1}/l1_cps.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'exploration (output not kept)'),
    R(f'{L1}/l1_dmg.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'exploration (output not kept)'),
    R(f'{L1}/l1_dump.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'exploration (output not kept)'),
    R(f'{L1}/l1_explore.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'exploration (output not kept)'),
    R(f'{L1}/l1_geom.py', LIB, 'twin', 'bc2_levelwrite::placement / site geometry', 'l1_measure, l1_sdmcheck, l1_try_hf',
      'PY1 / PR1c', "l1_measure's plane geometry"),
    R(f'{L1}/l1_level.py', LIB, 'twin', 'bc2_levelwrite::placement', 'l1_measure and 9 other L1 scripts',
      'PY1 / PR1c', "l1_measure's level graph"),
    R(f'{L1}/l1_levels.py', LIB, 'twin', 'bc2-levelwrite examples/v1_rules.rs', 'l1_measure, l1_aabb, l1_anchor, l1_scan',
      'PY1 / PR1c', 'the stock MP levels with era and mode (v1_rules.rs lists them the same way)'),
    R(f'{L1}/l1_measure.py', TOOL, 'twin', 'bc2_levelwrite::placement::measure',
      'l1_sdmcheck; tests 1 (VW2 cites simulate)', 'PY1 / PR1c',
      'V1 ported the measures (3,539 of 3,539 values equal); kept as the parity reference'),
    R(f'{L1}/l1_read.py', LIB, 'twin', 'bc2_formats fbrb / dbx', 'l1_dump, l1_geom, l1_level, l1_levels, l1_measure',
      'PY1 / PR1c', "a stock level's partition straight from the bundles"),
    R(f'{L1}/l1_rules.py', DEV, 'stays', 'bc2_levelwrite::rules (embeds l1_rules.json)', 'none', 'PY1 / PR1c',
      'the proposed rules checked against the stock levels (l1_rules_check.md)'),
    R(f'{L1}/l1_rushcheck.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'one-off Rush check'),
    R(f'{L1}/l1_scan.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'exploration across every level'),
    R(f'{L1}/l1_sdmcheck.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'one-off Squad Deathmatch check'),
    R(f'{L1}/l1_show.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)',
      'prints a measurement file; starts with a UTF-8 BOM'),
    R(f'{L1}/l1_try_ca.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)', 'exploration (output not kept)'),
    R(f'{L1}/l1_try_hf.py', DEV, 'stays', '', 'none', 'proposed: PR1c (delete)',
      'exploration (output not kept); starts with a UTF-8 BOM'),

    # --- bfbc2-menu-tools
    R('bfbc2-menu-tools/aptcheck.py', TOOL, 'twin', 'bc2_menu (aptcheck); bfbc2 aptcheck',
      'apt, aptrefslib; tests 5; OPFOR 4', 'PR1c',
      'the command line is Rust; verify / lint still build Code in Python for the two movies'),
    R('bfbc2-menu-tools/aptgraft.py', TOOL, 'twin', 'bc2_menu graft / pacific (O3, E4c, OR14); bfbc2 aptgraft',
      'tests 4; rust/parity_menu_movies, parity_pacific_menu', 'PR1c (delete)',
      "a test reference only since OR14: OPFOR's WW2 and Onslaught menu tools edit movies through bc2_native "
      '(AptMovie, pacific_main_nav / main_root, apt_dump_text)'),
    R('bfbc2-menu-tools/aptrefslib.py', LIB, 'twin', 'bc2_menu; bfbc2 aptrefs', 'tests 3; rust/bench_apt',
      'proposed: PR1b-2 (delete)', 'command line retired (RT3); refs_many_python reference; only tests import it'),
    R('bfbc2-menu-tools/bc2loc.py', TOOL, 'twin', 'bc2_menu loc (OR14); bfbc2 bc2loc',
      'menucommon; tests 6; rust/parity_pacific_menu', 'PR1c (delete)',
      "a test reference only since OR14: OPFOR's WW2 tools read and write texts through bc2_native.LocFile"),
    R('bfbc2-menu-tools/menucommon.py', LIB, 'twin', 'bc2_menu (bigf, pe, common)',
      'aptcheck, aptrefslib, bc2loc; tests 6', 'PR1c',
      "PEImage over the Rust PE reader, OPFOR's menu patch format (applied by bfbc2 aptpatch since OR5), movie lookup"),
    T('bfbc2-menu-tools/tests/test_apt_e3n.py'),
    T('bfbc2-menu-tools/tests/test_apt_e4c.py'),
    T('bfbc2-menu-tools/tests/test_aptgraft.py'),
    T('bfbc2-menu-tools/tests/test_bc2loc_native.py'),
    T('bfbc2-menu-tools/tests/test_bc2loc_write.py'),
    T('bfbc2-menu-tools/tests/test_menu_tools.py'),
    T('bfbc2-menu-tools/tests/test_pe_native.py'),

    # --- bfbc2-mesh-viewer
    R('bfbc2-mesh-viewer/meshview.py', TOOL, 'ported', 'bc2_convert::meshview; bfbc2 meshview; the editor\'s preview',
      '25 suite modules (shaderdb, bf1943import, texqa ...); tests 31; OPFOR 47', 'PR1b-2',
      "PR1b-2-rest: Index / load_mesh / mesh_surfaces / stack_models / render on the Rust port through bc2_native "
      "(MeshViewIndex, meshview_render), the pickle cache retired (meshview_index.bin); page() retired in OR4 "
      "(bc2_convert::meshview::page, OPFOR's preview pages); left in Python: the NumPy arrays and dicts, _bundles "
      "(OPFOR's step tracker sees its walk), the partition cache of read()"),
    R('bfbc2-mesh-viewer/tests/record_preview_parity.py', HELPER, 'stays', '', 'by hand (gr3.rs reads its output)',
      'PR1b-2', "recorded meshview's answers for the editor's asset preview (GR3); the recording is the retired Python's "
      "(since PR1b-2-rest meshview.py is the Rust port: running it again records the port)"),
    T('bfbc2-mesh-viewer/tests/test_meshview.py'),
    T('bfbc2-mesh-viewer/tests/test_meshview_index.py'),

    # --- bfbc2-model-tools
    R('bfbc2-model-tools/decimate.py', LIB, 'twin', 'bc2_convert decimate', 'tests 1; OPFOR 2 (type38, winchester1897)',
      'PR1b-2-rest2', 'Rust since PR1b-2-rest2 (OpenBLAS products written out: XWW2-MOD-1); the NumPy original '
      'kept as decimate_reference'),
    R('bfbc2-model-tools/shapes.py', LIB, 'partly', 'bc2_convert shapes', 'tests 2; OPFOR 1 (type100)', 'PR1b-2-rest2',
      'products, tangents and windings in Rust since PR1b-2-rest2 (XWW2-MOD-1; *_reference the NumPy originals); '
      'the builders (lathe, extrude) plain Python floats'),
    R('bfbc2-model-tools/skinmesh.py', LIB, 'twin', 'bc2_convert skinmesh', 'tests 2; OPFOR 3', 'PR1b-2',
      'Rust since F2a; its references kept (OPFOR feeds objects it builds)'),
    R('bfbc2-model-tools/texwrite.py', LIB, 'twin', 'bc2_convert texwrite', 'emitteratlas; tests 7; OPFOR 11',
      'PR1b-2', 'DXT encoding in Rust, its principal axes too since OR14 (bc2_convert::eigh3); references kept'),
    T('bfbc2-model-tools/tests/test_decimate.py'),
    T('bfbc2-model-tools/tests/test_model_f2a_native.py'),
    T('bfbc2-model-tools/tests/test_models.py'),
    T('bfbc2-model-tools/tests/test_pictures_native.py'),
    T('bfbc2-model-tools/tests/test_texwrite.py'),

    # --- bfbc2-navigation-studio (retired in GR2)
    R('bfbc2-navigation-studio/overlay.py', TOOL, 'retired-pending', "bc2-editor nav (the Navigation panel's overlays)",
      'tests 2', 'PR1c',
      "headless JSON exports the editor's AI workspace also makes (GR2)"),
    T('bfbc2-navigation-studio/tests/test_overlay_offline.py'),

    # --- bfbc2-shader-tools
    R('bfbc2-shader-tools/pcbuild.py', LIB, 'live', '', 'renderqa, xdiff; tests 6; OPFOR 10',
      'stays by decision (OR15)',
      'HLSL compiled with d3dcompiler_47 (ctypes), its cache and the database bindings; no Rust compile path'),
    R('bfbc2-shader-tools/renderqa.py', TOOL, 'partly', 'bfbc2 renderqa (argparse, errors); bc2-render (the draw)',
      'meshview, xdiff; tests 7; OPFOR 6', 'stays by decision (OR15)',
      'left: every drawing (shaderview.exe D3D11), the NumPy measures and the Pillow sheets'),
    R('bfbc2-shader-tools/shaderdb.py', TOOL, 'partly',
      'bc2_formats::shaderdb (open, merge, levels); bc2_convert::shaderlevels, shadertranslate, shadercmd; bc2_native.ShaderMerger (shaderdb.merger); bfbc2 shaderdb',
      '33 suite modules; tests 37; OPFOR 54', 'OPFOR in Rust stage 3',
      "reader (SD1), queries (E4b), the command line, the in-process merge (OR3a: the Python Merger retired, "
      "OPFOR's 21 sites on shaderdb.merger), retexture, the level cover index and the level scan (OR3b: "
      "bc2_convert::shaderlevels, the pickles retired) and OPFOR's BF1943 shader translation driver (OR3c: "
      "bc2_convert::shadertranslate - translate_plan, missing / foreign entries, discards, the soldier glue) are Rust; "
      "the needs and merge commands are also a library (OR10: bc2_convert::shadercmd, from bfbc2 shaderdb, for OPFOR's "
      "console port stage 7, which no longer runs this script); left: path glue, level_meshes (DBX reading)"),
    R('bfbc2-shader-tools/xdiff.py', TOOL, 'live', 'bfbc2 xdiff (argparse, errors only)',
      'tests 5; OPFOR 4 (xdiff.main in process)', 'stays by decision (OR15)',
      "the translator's differential test (xenos_exec, WARP draws)"),
    R('bfbc2-shader-tools/xenos.py', LIB, 'twin', 'bc2_shader::xenos (decoder, disasm)',
      'xdiff, xenos_exec, xenos_hlsl; tests 5; OPFOR 4', 'OPFOR in Rust stage 3',
      'parse_shader_python for damaged blobs and disasm for edited trees kept by ruling'),
    R('bfbc2-shader-tools/xenos_exec.py', LIB, 'live', '', 'xdiff; tests 2', 'OPFOR in Rust stage 3',
      'the NumPy reference interpreter of Xenos microcode (kept Python since E3f by measurement)'),
    R('bfbc2-shader-tools/xenos_hlsl.py', LIB, 'ported', 'bc2_shader::hlsl (translate)',
      'xdiff, xenos_exec; tests 3; OPFOR 3', 'OPFOR in Rust stage 3',
      'translate_python retired (R3); a thin call into the Rust translator'),
    T('bfbc2-shader-tools/tests/test_pcbuild_batch.py'),
    T('bfbc2-shader-tools/tests/test_renderqa.py'),
    T('bfbc2-shader-tools/tests/test_shaderdb.py'),
    T('bfbc2-shader-tools/tests/test_shaderdb_levels.py'),
    T('bfbc2-shader-tools/tests/test_shaderdb_merge.py'),
    T('bfbc2-shader-tools/tests/test_shaderdb_native.py'),
    T('bfbc2-shader-tools/tests/test_shaderdb_queries.py'),
    T('bfbc2-shader-tools/tests/test_shadertranslate.py'),
    T('bfbc2-shader-tools/tests/test_shaderview_serve.py'),
    T('bfbc2-shader-tools/tests/test_xdiff.py'),
    T('bfbc2-shader-tools/tests/test_xenos_native.py'),

    # --- bfbc2-stockmp-ai (its command line is Rust since RT3: only tests reach this Python)
    R('bfbc2-stockmp-ai/author.py', LIB, 'twin', 'bc2_stockmp (schema, write)', 'pipeline; tests 5', 'PR1b-2',
      'author_native is Rust (E4a); the Python reference remains'),
    R('bfbc2-stockmp-ai/config.py', LIB, 'twin', 'bc2-cli stockmp::generate / pyx', 'author, pipeline, validate; tests 4',
      'PR1b-2', 'generate and the snapper in Rust (O5); hand-written config shapes still compared against it'),
    R('bfbc2-stockmp-ai/dbxbin.py', LIB, 'twin', 'bc2_formats::dbx edit', 'stockmp modules; tests 4; '
      'rust/bc2-levelwrite/tests/dbxbin_reference.py', 'PR1b-2', "levelwrite's real_levels test uses it as reference"),
    R('bfbc2-stockmp-ai/levelmodel.py', LIB, 'twin', 'bc2_stockmp (factions, model)', 'author, pipeline, stockmplib; '
      'tests 3', 'PR1b-2', "the level model in Rust (E4a)"),
    R('bfbc2-stockmp-ai/pipeline.py', LIB, 'twin', 'bc2-cli stockmp::build', 'stockmplib, validate; tests 8',
      'PR1b-2', 'the build and packaging in Rust (O5)'),
    R('bfbc2-stockmp-ai/stockcore.py', SHIM, 'ported', 'bc2_stockmp through bc2_native',
      'author, dbxbin, levelmodel, validate; tests 3', 'PR1b-2', "the stock-MP tool's native loader"),
    R('bfbc2-stockmp-ai/stockmplib.py', LIB, 'twin', 'bc2-cli stockmp; bfbc2 stockmp', 'tests 5', 'PR1b-2',
      'command line retired (RT3); summary / install / pathdata_for kept for the tests only'),
    R('bfbc2-stockmp-ai/validate.py', LIB, 'twin', 'bc2-cli stockmp::validate; bc2_stockmp checks',
      'tests 4', 'PR1b-2', 'the offline checks in Rust (O5); the Python is the reference'),
    T('bfbc2-stockmp-ai/tests/test_native_levels.py'),
    T('bfbc2-stockmp-ai/tests/test_stockmp.py'),
    T('bfbc2-stockmp-ai/tests/test_validate.py'),

    # --- core
    R('core/bfbc2core/__init__.py', LIB, 'live', '', 'every Python tool; OPFOR', 'the last Python',
      "the package; __version__ from VERSION"),
    R('core/bfbc2core/appicon.py', LIB, 'twin', 'rust/app_icon.rs, bc2-editor appicon', 'hub (.pyw); tests 1',
      'proposed: with the hub', 'the icon lookup the Rust builds also make'),
    R('core/bfbc2core/atomicio.py', LIB, 'twin', "bc2_formats' one whole-file writer (PR1a)",
      '17 suite modules; tests 5', 'PR1b-2', 'atomic whole-file writes for the Python tools'),
    R('core/bfbc2core/caches.py', TOOL, 'twin', 'bc2-cli caches / budget; bfbc2 caches',
      '25 suite modules; tests 3', 'PR1b-2', "the cache budget's twin; the command line is Rust (O3); one implementation "
      "needs PR1b-2's library crate"),
    R('core/bfbc2core/collision.py', LIB, 'ported', 'bc2_scene::collision (havok_index, triangles)',
      'detailed, ladders, levelobjects, navsample_offline, navwater_mesh, level; tests 3', 'NK5',
      'the Python walk retired (F1a); a thin module over the core'),
    R('core/bfbc2core/dbx.py', LIB, 'ported', 'bc2_formats::dbx', '47 suite modules; tests 24; OPFOR 33', 'PR1b-1c',
      'read and written in Rust; Element trees for Python callers (PR1b-1 decided the one DBX document: an arena with the DbxEdit rules, merged in PR1b-2)'),
    R('core/bfbc2core/dbxedit.py', LIB, 'ported', 'bc2_formats::dbxedit; bfbc2 dbxedit',
      'navkit.py, validate, dbxedit.py; tests 9; OPFOR 3', 'PR1b-2', "DbxEdit's commands in process"),
    R('core/bfbc2core/envvars.py', LIB, 'live', '', 'run_tests.py; tests 1 (test_envvars)', 'proposed: Tests in Rust',
      'the registry of every environment variable the suite (Python and Rust) reads'),
    R('core/bfbc2core/errors.py', LIB, 'twin', 'bc2-cli pyproc / pylog (errors.setup / guard emulated)',
      '36 suite modules; tests 5; OPFOR 2', 'the last Python', "the Python tools' day logs and error reports"),
    R('core/bfbc2core/fbrb.py', LIB, 'ported', 'bc2_formats::fbrb', '32 suite modules; tests 39; OPFOR 26', 'PR1b-1c',
      'a thin module over the core (the bundle cache in Rust)'),
    R('core/bfbc2core/fbrbtool.py', LIB, 'ported', 'bc2_formats::fbrb::tool; bfbc2 fbrbtool',
      'fbrbtool.py, navkit.py, hub; tests 11', 'PR1b-2', "FbrbTool's commands in process"),
    R('core/bfbc2core/gameledger.py', LIB, 'ported', 'bc2_gamefs::ledger, bc2_install::ledgerapi (bc2_native.ledger_call); '
      'bfbc2 gameledger', 'navkit.py, texqa, stockmp pipeline / lib, OPFOR opforcore.stages / migrate; tests 8',
      'OR15', "a client of the one install ledger (round OR15: the 913-line twin retired, OPFOR's ledger.py too)"),
    R('core/bfbc2core/level.py', LIB, 'ported', 'bc2_scene (scene_load)', '13 suite modules; tests 9', 'PR1b-1c',
      "the level loader in Rust (E1a); the Python builds the tools' Entity / Level objects"),
    R('core/bfbc2core/native.py', LIB, 'live', 'bc2-cli pyproc (RETIRED list mirrored)',
      '63 suite modules, CI workflows; tests 53; OPFOR 11', 'PR1b-2 (one registry), then the last Python',
      'finds and loads bc2_native, its fingerprint, the backend registry, RETIRED_CLI'),
    R('core/bfbc2core/navdata.py', LIB, 'ported', 'bc2_navbuild parity / bc2-editor navdata',
      'navboundary, navparity, navwater_mesh, harness, overlay; tests 11', 'NK5',
      'its Python twins retired (E3o); cells and areas from the core'),
    R('core/bfbc2core/navpath.py', LIB, 'ported', 'bc2_navbuild astar / navpath', 'harness checks; tests 6', 'PR1b-2',
      'the path test over the route graph, in Rust'),
    R('core/bfbc2core/navtune.py', LIB, 'twin', 'bc2-cli navkit apply, bc2-editor navtune', 'navkit.py; tests 9',
      'NK5', 'the navtune side files; the editor and bfbc2 navkit apply have their own'),
    R('core/bfbc2core/paths.py', LIB, 'twin', 'bc2-cli / bc2-editor find_opfor, pyos',
      '23 suite modules; tests 39; OPFOR 2 (opforcore.paths)', 'the last Python',
      "the Python tools' path discovery; the Rust tools each have their own (PR1b-2's bc2-gamefs)"),
    R('core/bfbc2core/terrain.py', LIB, 'ported', 'bc2_convert::terrain (heightfield)',
      'navparity, navsample_offline, renderqa, level; tests 10; OPFOR 1', 'PR1b-1c', 'height fields from the core'),
    R('core/bfbc2core/water.py', LIB, 'ported', 'bc2_formats::water', 'navregress, navwater, level; tests 5',
      'NK4', 'water meshes from the core'),
    R('core/bfbc2core/xmlinst.py', LIB, 'ported', 'bc2_formats xmlinst', 'NavKit tools, overlay; tests 1; OPFOR 2',
      'NK5', "DbxEdit XML's instances in Rust (F2b)"),
    H('core/tests/cli_o2_crash.py', 'test_cli_o2'),
    H('core/tests/cli_o2_menu.py', 'test_cli_o2'),
    H('core/tests/cli_o2_nav.py', 'test_cli_o2'),
    H('core/tests/cli_o3_audio.py', 'test_cli_o3'),
    H('core/tests/cli_o3_bf1943.py', 'test_cli_o3'),
    H('core/tests/cli_o3_caches.py', 'test_cli_o3'),
    H('core/tests/cli_o3_cull.py', 'test_cli_o3'),
    H('core/tests/cli_o3_graft.py', 'test_cli_o3'),
    H('core/tests/cli_o3_meshview.py', 'test_cli_o3'),
    H('core/tests/cli_o3_stockmp.py', 'test_cli_o3'),
    H('core/tests/cli_o4_harness.py', 'test_cli_o4'),
    H('core/tests/cli_o4_navkit.py', 'test_cli_o4; rust/parity_nk1_apply.py'),
    H('core/tests/cli_o4_navtools.py', 'test_cli_o4'),
    H('core/tests/cli_o4_texqa.py', 'test_cli_o4'),
    H('core/tests/cli_o5_bf1943.py', 'test_cli_o5'),
    H('core/tests/cli_o5_nav.py', 'test_cli_o5'),
    H('core/tests/cli_o5_readers.py', 'test_cli_o5'),
    H('core/tests/cli_o5_shader.py', 'test_cli_o5'),
    H('core/tests/cli_o5_stockmp.py', 'test_cli_o5'),
    H('core/tests/csharp_golden.py', 'tests 8'),
    H('core/tests/goldens.py', 'tests 8'),
    T('core/tests/test_abbrev.py'),
    T('core/tests/test_argfmt.py'),
    T('core/tests/test_caches.py'),
    T('core/tests/test_cli_o1.py'),
    T('core/tests/test_cli_o2.py'),
    T('core/tests/test_cli_o3.py'),
    T('core/tests/test_cli_o4.py'),
    T('core/tests/test_cli_o5.py'),
    T('core/tests/test_core_edges.py'),
    T('core/tests/test_dbx.py'),
    T('core/tests/test_dbx_native.py'),
    T('core/tests/test_dbx_write_native.py'),
    T('core/tests/test_dbxedit.py'),
    T('core/tests/test_errors.py'),
    T('core/tests/test_fbrb.py'),
    T('core/tests/test_fbrb_cache.py'),
    T('core/tests/test_fbrb_native.py'),
    T('core/tests/test_fbrb_pack_native.py'),
    T('core/tests/test_fbrb_unpack_native.py'),
    T('core/tests/test_fbrbtool.py'),
    T('core/tests/test_gameledger.py'),
    T('core/tests/test_geometry_native.py'),
    T('core/tests/test_level.py'),
    T('core/tests/test_logs_caches.py'),
    T('core/tests/test_navdata.py'),
    T('core/tests/test_navtune.py'),
    T('core/tests/test_navtune_native.py'),
    T('core/tests/test_paths.py'),
    T('core/tests/test_prepare_o4.py'),
    T('core/tests/test_scene_native.py'),
    T('core/tests/test_spec_probes.py'),
    T('core/tests/test_terrain.py'),
    T('core/tests/test_water.py'),
    T('core/tests/test_xmlinst_native.py'),

    # --- design
    R('design/icons/make_icon.py', DEV, 'stays', '', 'by hand', 'stays', "makes the application icon from a render"),

    # --- the root
    R('run_tests.py', DEV, 'test', 'bc2 test entries run through it (tests/test_rust.py)',
      'CI (checks.yml), hub, tests 51', 'Tests in Rust', 'the test runner: tiers, ids, scheduling'),
    R('suite_paths.py', LIB, 'live', '', 'every Python tool and test; OPFOR 2', 'the last Python',
      "the one sys.path bootstrap"),

    # --- rust (scripts beside the crates)
    R('rust/bc2-cli/gen_argspecs.py', DEV, 'stays', 'bc2-cli argspecs.rs (its output)', 'by hand; tests 2',
      'with the last argparse tool',
      "writes argspecs.rs from the Python tools' argparse parsers (needs them alive)"),
    R('rust/bc2-editor/tests/package_levels_check.py', HELPER, 'test', '', 'G3 (by hand); tests 1', 'Tests in Rust',
      "G3's independent check of Package-only levels"),
    R('rust/bc2-editor/tests/record_inspector.py', HELPER, 'stays', '', 'by hand (inspector_parity.rs reads its '
      'output)', 'PR1c', "recorded the retired Map Inspector's outliner for the editor's test"),
    R('rust/bc2-levelwrite/tests/dbxbin_reference.py', HELPER, 'test', '', 'real_levels.rs', 'PR1b-2',
      "the Python reference of levelwrite's edits (stockmp's dbxbin)"),
    R('rust/bench_apt.py', BENCH, 'stays', '', 'by hand', 'stays', 'E4c timings'),
    R('rust/bench_bf1943.py', BENCH, 'stays', '', 'by hand; tests 2', 'stays', 'E3r timings'),
    R('rust/bench_cli.py', BENCH, 'stays', '', 'by hand', 'stays', 'O1 timings'),
    R('rust/bench_dbx.py', BENCH, 'stays', '', 'by hand', 'stays', 'dbx reader timings'),
    R('rust/bench_dbxport.py', BENCH, 'stays', '', 'by hand', 'stays', 'E4d timings (Rust and port_python)'),
    R('rust/bench_f2a.py', BENCH, 'stays', '', 'by hand', 'stays', 'F2a timings against the references'),
    R('rust/bench_fbrb.py', BENCH, 'stays', '', 'by hand', 'stays', 'bundle reader timings'),
    R('rust/bench_fbrb_cache.py', BENCH, 'stays', '', 'by hand', 'stays', 'E3e bundle cache timings'),
    R('rust/bench_granny.py', BENCH, 'stays', '', 'by hand', 'stays', 'E3i timings'),
    R('rust/bench_memory.py', BENCH, 'stays', '', 'by hand', 'stays',
      "the batch readers' peak memory; any command's process-tree peak (--command)"),
    R('rust/bench_harness.py', BENCH, 'stays', '', 'by hand', 'stays', 'E4f timings against the references'),
    R('rust/bench_level.py', BENCH, 'stays', '', 'by hand', 'stays', 'level loader timings'),
    R('rust/bench_navkit.py', BENCH, 'stays', '', 'by hand', 'NK5', 'one NavKit build (navregress.build_level)'),
    R('rust/bench_ports.py', BENCH, 'stays', '', 'by hand; tests 2', 'stays', 'E3g timings'),
    R('rust/bench_sd1.py', BENCH, 'stays', '', 'by hand', 'stays', 'SD1 timings'),
    R('rust/bench_shaderdb.py', BENCH, 'stays', '', 'by hand', 'stays', 'E4b timings'),
    R('rust/bench_sweep.py', BENCH, 'stays', '', 'by hand', 'stays', 'the E3m sweep'),
    R('rust/bench_xenos.py', BENCH, 'stays', '', 'by hand', 'stays', 'E3f timings'),
    R('rust/build_native.py', DEV, 'stays', 'cargo, maturin (it drives them)',
      'CI, hub, 25 suite modules (the fix text); tests 27; OPFOR 4', 'Release',
      'builds or downloads bc2_native and the binaries'),
    R('rust/hang_stacks.py', DEV, 'stays', '', "bc2-editor's test hang guard (tests/main.rs); tests 1", 'stays',
      "a hung process's thread stacks (HG1)"),
    R('rust/native_census.py', DEV, 'stays', '', 'by hand', 'PR1b-2 (made a check)',
      "who calls each bc2_native export (AU5 ARC-12)"),
    R('rust/parity_e3o_nav.py', BENCH, 'stays', '', 'by hand; tests 2', 'stays',
      'kymesh Rust reader against parse_mesh_reference'),
    R('rust/parity_nk1_apply.py', BENCH, 'stays', '', 'by hand', 'stays', 'NK1 parity against a pre-NK1 checkout'),
    R('rust/parity_nk2_file.py', BENCH, 'stays', '', 'by hand', 'stays',
      'NK2 parity against a pre-NK2 checkout (both checkouts pinned at bda5b8d or earlier: they need navbuild.py)'),
    R('rust/parity_nk3_detail.py', BENCH, 'stays', '', 'by hand (detail_tests.rs cites it)', 'stays',
      'NK3 parity against a pre-NK3 checkout (capture pinned at bda5b8d or earlier: it needs navbuild.py)'),
    R('rust/parity_nk4_walk.py', BENCH, 'stays', '', 'by hand', 'stays',
      "NK4 parity and memory against a pre-NK4 checkout (walkability's arrays per stage, private bytes; both "
      "checkouts pinned at bda5b8d or earlier: they need navbuild.py)"),
    R('rust/parity_nk5_build.py', BENCH, 'stays', '', 'by hand', 'stays',
      "round NK5's parity: whole regression builds, old against new (files, log lines, time and peak memory)"),
    R('rust/parity_menuart.py', BENCH, 'stays', '', 'by hand', 'stays',
      "round OR5's picture operations (bc2_menu::pil / modeart) against Pillow, numpy and OPFOR's retired onslaught_art"),
    R('rust/parity_menu_movies.py', BENCH, 'stays', '', 'by hand', 'stays',
      "round OR14's OPFOR movie tools on bc2_native.AptMovie (bc2_menu::graft) against aptgraft.py: written files, "
      "printed lines, damaged inputs, random cases"),
    R('rust/parity_pacific_menu.py', BENCH, 'stays', '', 'by hand', 'stays',
      "round OR14's Pacific War menu edits (bc2_menu::pacific) and texts (bc2_menu::loc) against menu_install.py's "
      'Python on aptgraft.py and bc2loc.py: movies, damaged movies, the whole menu step, every language file'),
    R('rust/parity_silhouette.py', BENCH, 'stays', '', 'by hand', 'stays',
      "round OR14's side silhouettes (bc2_convert::pictures::side, Pillow's polygon fill) against OPFOR's "
      'type95_icon.silhouette and Pillow: random polygons, synthetic and game meshes'),
    R('rust/parity_texwrite_axes.py', BENCH, 'stays', '', 'by hand', 'stays',
      "round OR14's texwrite principal axes (bc2_convert::eigh3) against numpy.linalg.eigh, bit for bit, and the "
      "encoded blocks"),
    R('rust/parity_navkit_paths.py', BENCH, 'stays', '', 'by hand', 'stays',
      "NavKit's level driver through the command line, the editor in process and the export step (files, lines)"),
    R('rust/python_status.py', DEV, 'stays', '', 'by hand; tests/test_repo.py', 'stays',
      'this inventory'),
    R('tasks/build.py', DEV, 'stays', '', 'by hand; run_tests.py (tasks-registry)', 'PX6',
      "the task registry's generator and check: ROADMAP.md, tasks/board.json, battlefield-opfor's TASKS.md"),

    # --- spec
    R('spec/probes/destruction.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/destruction.md'),
    R('spec/probes/census.py', DEV, 'stays', '', 'run_tests.py (quick: --check); core/tests/test_spec_probes.py (game)', 'stays',
      "the specification's census of DICE's files (spec/census.json, spec/CENSUS.md) and its coverage gate"),
    R('spec/probes/dependencies.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/transforms/dependencies.md (the asset graph; --steps the WW2 step graph); game tier'),
    R('spec/probes/effects_chain.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/effects.md (the processor chain); checked in the game tier'),
    R('spec/probes/gamemodes.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/gamemodes.md (game modes, the game mod: data and exe sites); game tier'),
    R('spec/probes/grime.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/materials.md section 5 (grime, dirt, camouflage); checked in the game tier'),
    R('spec/probes/level.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/model/level.md, the cull trees, vegetation and terrain splines; checked in the game tier'),
    R('spec/probes/lighting.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for the lighting, weather and effects specs'),
    R('spec/probes/materials.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/materials.md'),
    R('spec/probes/menus.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/menus.md (the action script of the movies, the runtime); checked in the game tier'),
    R('spec/probes/modes.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/bot_ai.md (mode entities: event names against the exe); checked in the game tier'),
    R('spec/probes/options.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/settings.md and input.md; checked in the game tier'),
    R('spec/probes/persistence.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/persistence.md (award codes against the value templates); checked in the game tier'),
    R('spec/probes/registry.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/model/registration.md and bundles.md (the stock registries); checked in the game tier'),
    R('spec/probes/resources.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for the SPG4 resource and loose-file formats (occluder, soot, animtree, bink, game_folder); game tier'),
    R('spec/probes/runtime.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for the run-time rules of SPG4-rest (controller states, squad spawn, kill cam, save); game tier'),
    R('spec/probes/scripting.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/scripting.md (the level event graph); checked in the game tier'),
    R('spec/probes/terrain.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/transforms/terrain_edit.md and terrain.md; checked in the game tier'),
    R('spec/probes/terrain_shape.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/transforms/terrain_gen.md (DICE terrain shapes, biomes, playable areas); game tier'),
    R('spec/probes/typeinfo.py', DEV, 'stays', '', 'run_tests.py (quick: --check); core/tests/test_spec_probes.py (game)', 'stays',
      "the executable's reflection record by record (spec/typeinfo.json, spec/engine/typeinfo/*.md tables)"),
    R('spec/probes/vehicles.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/vehicles.md'),
    R('spec/probes/weapons.py', DEV, 'stays', '', 'core/tests/test_spec_probes.py (game)', 'stays',
      'read-only probe for spec/engine/weapons.md'),
    R('spec/trace.py', DEV, 'stays', '', 'run_tests.py (quick); tests 1', 'proposed: Tests in Rust',
      "the specification's traceability check"),
    R('tools/decomp/build_index.py', DEV, 'stays', '', 'tools/decomp/run_dc.py', 'stays',
      "the private decompiled database's SQLite index (round DC0)"),
    R('tools/decomp/decomp.py', TOOL, 'stays', '', 'later rounds (GM1, UI0, RCX0)', 'stays',
      'lookup in the private decompiled database of BFBC2Game.exe by address, name, string, caller or rule'),
    R('tools/decomp/extract_names.py', DEV, 'stays', '', 'tools/decomp/run_dc.py', 'stays',
      "the named exe addresses of the spec and Project OPFOR's notes, for Ghidra"),
    R('tools/decomp/gen_types.py', DEV, 'stays', '', 'tools/decomp/run_dc.py', 'stays',
      "spec/typeinfo.json as a Ghidra type plan"),
    R('tools/decomp/run_dc.py', DEV, 'stays', '', 'by hand', 'stays',
      'builds the private decompiled database with Ghidra headless'),

    # --- tests
    H('tests/f0_parity.py', 'tests 15'),
    H('tests/f2a_parity.py', 'tests 3'),
    H('tests/replay.py', 'tests 14'),
    T('tests/test_appicon.py'),
    T('tests/test_asset_accuracy.py'),
    T('tests/test_cli_errors.py'),
    T('tests/test_decomp_tools.py'),
    T('tests/test_converter_edges.py'),
    T('tests/test_converter_units.py'),
    T('tests/test_deep_scan_fixes.py'),
    T('tests/test_e3o_native.py'),
    T('tests/test_editor.py'),
    T('tests/test_envvars.py'),
    T('tests/test_f3_native.py'),
    T('tests/test_f4_native.py'),
    T('tests/test_hub.py'),
    T('tests/test_lint.py'),
    T('tests/test_locators.py'),
    T('tests/test_native_required.py'),
    T('tests/test_repo.py'),
    T('tests/test_review_fixes.py'),
    T('tests/test_review_fixes2.py'),
    T('tests/test_review_regressions.py'),
    T('tests/test_runner_args.py'),
    T('tests/test_runner_output.py'),
    T('tests/test_runner_parallel.py'),
    T('tests/test_rust.py'),
    T('tests/test_rust_skip.py'),
    T('tests/test_sd1_native.py'),
    T('tests/test_types.py'),
)


def python_files(root: str = SUITE) -> list[str]:
    """The suite's .py files: tracked ones and new ones git does not ignore, those that exist (sorted, '/')."""
    out = subprocess.run(['git', '-C', root, 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--',
                          '*.py'], capture_output=True, check=True).stdout.decode('utf-8')
    return sorted({p for p in out.split('\0') if p and os.path.isfile(os.path.join(root, p))})


def table_problems(files: list[str], table: tuple[Row, ...]) -> list[str]:
    """What is wrong between the files and the table: a file with no row, a row naming no file, a row twice, a kind
    or status that is not one of KINDS / STATUSES."""
    paths = [r.path for r in table]
    seen = set(paths)
    out = [f'{p}: not in rust/python_status.py\'s TABLE (classify it: kind, status, Rust counterpart, callers, '
           'retiring round, note)' for p in files if p not in seen]
    on_disk = set(files)
    out += [f'{p}: in rust/python_status.py\'s TABLE but the file is gone (take its row out)'
            for p in paths if p not in on_disk]
    out += [f'{p}: twice in TABLE' for p in sorted({p for p in paths if paths.count(p) > 1})]
    out += [f'{r.path}: kind {r.kind!r} is not one of {KINDS}' for r in table if r.kind not in KINDS]
    out += [f'{r.path}: status {r.status!r} is not one of {STATUSES}' for r in table if r.status not in STATUSES]
    return out


def non_blank_lines(path: str) -> int:
    with open(path, 'rb') as f:
        text = f.read().decode('utf-8-sig', errors='replace')
    return sum(1 for line in text.splitlines() if line.strip())


def first_sentence(path: str) -> str:
    """A test's note: its module docstring's first sentence (up to 110 characters)."""
    with open(path, 'rb') as f:
        source = f.read().decode('utf-8-sig', errors='replace')
    try:
        doc = ast.get_docstring(ast.parse(source)) or ''
    except SyntaxError:
        return ''
    text = ' '.join(doc.split())
    cut = re.search(r'(?<=\.)\s', text)
    text = text[:cut.start()] if cut else text
    return text if len(text) <= 110 else text[:107].rstrip() + '...'


INCLUDE = re.compile(r'include_str!\(\s*"([^"]+\.py)"\s*\)')


def embedded(root: str = SUITE) -> dict[str, list[str]]:
    """Python files a Rust source embeds with include_str! (bfbc2.exe prints its usage from the docstring, or keys a
    cache by the file's hash): {python path: [rust files]}."""
    out: dict[str, list[str]] = {}
    tracked = subprocess.run(['git', '-C', root, 'ls-files', '-z', '--', 'rust/*.rs'], capture_output=True,
                             check=True).stdout.decode('utf-8').split('\0')
    for rs in sorted(p for p in tracked if p):
        full = os.path.join(root, rs)
        if not os.path.isfile(full):
            continue
        with open(full, encoding='utf-8', errors='replace') as f:
            text = f.read()
        for rel in INCLUDE.findall(text):
            target = os.path.relpath(os.path.normpath(os.path.join(os.path.dirname(full), rel)), root)
            out.setdefault(target.replace(os.sep, '/'), []).append(rs)
    return {k: sorted(set(v)) for k, v in sorted(out.items())}


class Exception_(NamedTuple):
    what: str
    files: tuple[str, ...]  # suite paths, or 'opfor:<path>' for Project OPFOR BFBC2's
    why: str


# What stays Python by decision (the user, 2026-11-27: round OR15's brief), each with its files and why. A suite file
# named here has its TABLE row's retiring 'stays by decision (OR15)' (table_problems checks it).
EXCEPTIONS: tuple[Exception_, ...] = (
    Exception_('the HLSL compiles', ('bfbc2-shader-tools/pcbuild.py',),
               "Microsoft's d3dcompiler_47 compiles the shaders (through ctypes); a Rust path would call the same DLL, "
               'so there is nothing of ours to port'),
    Exception_('the preview tools that launch shaderview',
               ('bfbc2-shader-tools/renderqa.py', 'bfbc2-shader-tools/xdiff.py', 'opfor:src/tools/ww2/wake_preview.py',
                'opfor:src/tools/ww2/weapon_preview.py'),
               'they drive shaderview.exe (D3D11) and measure its pictures for people looking at them; their command '
               'lines and errors are Rust already (bfbc2 renderqa / xdiff)'),
    Exception_("testrun's game launch", ('opfor:src/tools/testrun/testrun.py',),
               'it starts the game (BFBC2Game.exe) with a temporary opfor.ini and watches it; its ini edits are Rust '
               '(bc2_native.testrun_*); in-game tests are paused (the user, 2026-11-20)'),
    Exception_('playtest.py', ('opfor:src/tools/testrun/playtest.py',),
               "the unattended in-game test driver (scenarios, the virtual pad, key input, screenshots of the game's "
               'window): it only runs beside the game'),
)


# The Python exit (the user, 2026-11-27; tasks/registry.toml PX1-PX7): the round that retires each file. PX1 is
# OR16-rest (its files named), PX3 reverses the "stays Python by decision" exceptions, PX6 is the plumbing, the shims
# and every file left; the rest follows the status: partly -> PX2, twin -> PX4 (PY1), test -> PX5 (Tests in Rust).
PX1_FILES = ('bfbc2-auto-navkit/tools/navregress.py', 'bfbc2-auto-navkit/tools/navboundary.py',
             'bfbc2-auto-navkit/tools/navwater.py', 'bfbc2-auto-navkit/tools/navwater_mesh.py',
             'bfbc2-auto-navkit/tools/navsample.py', 'bfbc2-auto-navkit/tools/detailed.py',
             'bfbc2-auto-navkit/tools/hpdview.py', 'bfbc2-bf1943-import/dbxparitylib.py',
             'bfbc2-automated-map-and-bot-test-harness/harness.py',
             'bfbc2-automated-map-and-bot-test-harness/testharness/classify.py',
             'bfbc2-automated-map-and-bot-test-harness/testharness/levelchecks.py',
             'bfbc2-automated-map-and-bot-test-harness/testharness/levelinfo.py')
PX3_FILES = ('bfbc2-shader-tools/xenos_exec.py', 'bfbc2-auto-navkit/tools/sample_all.py')
PX_WHAT = (
    ('PX1', "OR16-rest: navregress, the harness's run and libraries, dbxparitylib, the NavKit diagnostics, hpdview"),
    ('PX2', 'every "partly" file finished'),
    ('PX3', 'the "stays Python by decision" files reversed (the HLSL compiles, xdiff / xenos_exec, the previews, '
            'the game launches)'),
    ('PX4', 'PY1: the twins retired, bc2-pyrt deleted'),
    ('PX5', 'Tests in Rust, on the synthetic corpus'),
    ('PX6', 'the plumbing, the shims, the "stays" scripts as Rust benches; then every .py deleted'),
    ('PX7', 'the baselines (retires no file)'),
)


def px_round(r: Row) -> str:
    """The Python exit's round that retires the file (PX1-PX7); a row whose retiring is already a PX code keeps it."""
    if r.retiring.startswith('PX'):
        return r.retiring.split()[0]
    if r.path in PX1_FILES:
        return 'PX1'
    if r.path in PX3_FILES or r.retiring == 'stays by decision (OR15)':
        return 'PX3'
    return {'partly': 'PX2', 'twin': 'PX4', 'test': 'PX5'}.get(r.status, 'PX6')


def folder_of(path: str) -> str:
    return path.split('/', 1)[0] if '/' in path else '(root)'


def collect(root: str = SUITE) -> list[dict[str, object]]:
    emb = embedded(root)
    rows = []
    for r in sorted(TABLE, key=lambda r: r.path):
        full = os.path.join(root, r.path)
        rows.append({
            'path': r.path, 'folder': folder_of(r.path), 'lines': non_blank_lines(full), 'kind': r.kind,
            'status': r.status, 'rust': r.rust, 'callers': r.callers, 'retiring': px_round(r),
            'retiring_before_px': r.retiring,
            'note': r.note or (first_sentence(full) if r.status == 'test' else ''),
            'embedded_by': emb.get(r.path, []),
        })
    return rows


def totals(rows: list[dict[str, object]]) -> dict[str, object]:
    by_status = {s: {'files': 0, 'lines': 0} for s in STATUSES}
    by_folder: dict[str, dict[str, int]] = {}
    for r in rows:
        n = int(str(r['lines']))
        s = str(r['status'])
        by_status[s]['files'] += 1
        by_status[s]['lines'] += n
        f = by_folder.setdefault(str(r['folder']), {'files': 0, 'lines': 0, **{k: 0 for k in STATUSES}})
        f['files'] += 1
        f['lines'] += n
        f[s] += n
    return {'by_status': by_status, 'by_folder': dict(sorted(by_folder.items())),
            'all': {'files': len(rows), 'lines': sum(int(str(r['lines'])) for r in rows)}}


def cell(text: object) -> str:
    return str(text).replace('|', '\\|').replace('\n', ' ')


def markdown(rows: list[dict[str, object]], tot: dict[str, object]) -> str:
    by_status = tot['by_status']
    by_folder = tot['by_folder']
    alls = tot['all']
    assert isinstance(by_status, dict) and isinstance(by_folder, dict) and isinstance(alls, dict)
    out = [
        '# Python status',
        '',
        'Every Python file of the suite, what it does for the port to Rust and the round that retires it. Generated by',
        '`python rust\\python_status.py` from its table (do not edit this file: edit the table and run it again);',
        'the same data as `python_status.json`. "Retiring" is the Python exit\'s round (PX1-PX7, the user\'s goal of',
        '2026-11-27; each a task in `tasks/registry.toml`); the round planned before it is the JSON\'s',
        '`retiring_before_px`. This page is the one list of the Python that is left (ROADMAP.md keeps none);',
        'retired tools are in RETIRED.md, retired twins in `rust/BENCHMARKS.md` and CHANGELOG.md. A new or deleted',
        '`.py` file fails `tests/test_repo.py` until its row is in the table.',
        '',
        '- **live**: only Python does this job (the real port work left).',
        '- **partly**: part of its job is ported; the note names what is left.',
        '- **twin**: a Python twin of Rust code, kept for parity, as the fallback where the Rust refuses, or for its'
        ' callers.',
        '- **ported**: Rust answers its job and the Python is a thin caller.',
        "- **retired-pending**: in a retired tool's folder, to be moved or deleted.",
        '- **stays**: benchmarks and parity measurements against the old Python, build and dev scripts.',
        '- **test**: tests and their helpers stay Python until "Tests in Rust".',
        '',
        'Lines are non-blank lines, counted when this file was generated. "Embedded" marks a file a Rust source',
        'includes with `include_str!` (bfbc2.exe prints its usage from it or keys a cache by its hash): editing or',
        'deleting it changes the binary. Callers: imports and runs in the suite and in Project OPFOR BFBC2\'s tracked',
        'files ("OPFOR N": its files); "tests N": the suite\'s test files.',
        '',
        '## Totals',
        '',
        '| Status | Files | Lines |',
        '|---|---:|---:|',
    ]
    for s in STATUSES:
        v = by_status[s]
        out.append(f'| {s} | {v["files"]} | {v["lines"]:,} |')
    out.append(f'| **all** | **{alls["files"]}** | **{alls["lines"]:,}** |')
    out += ['', '| Folder | Files | Lines | ' + ' | '.join(STATUSES) + ' |',
            '|---|---:|---:|' + '---:|' * len(STATUSES)]
    for folder, v in by_folder.items():
        out.append(f'| `{folder}` | {v["files"]} | {v["lines"]:,} | '
                   + ' | '.join(f'{v[s]:,}' if v[s] else '' for s in STATUSES) + ' |')
    out += ['', 'Lines by status in the folder table.', '', '## By retiring round', '',
            '| Round | What it retires | Files | Lines |', '|---|---|---:|---:|']
    for px, what in PX_WHAT:
        mine = [r for r in rows if r['retiring'] == px]
        out.append(f'| {px} | {what} | {len(mine)} | {sum(int(str(r["lines"])) for r in mine):,} |')
    out += ['', '## Stays Python by decision (reversed by PX3)', '',
            'The user, 2026-11-27 (round OR15): these jobs stay Python, recorded here as exceptions rather than port '
            'work; the Python exit (the user, 2026-11-27) reverses them in PX3.',
            '(`opfor:` paths are Project OPFOR BFBC2\'s files.)', '', '| What | Files | Why |', '|---|---|---|']
    for e in EXCEPTIONS:
        out.append(f'| {cell(e.what)} | {", ".join(f"`{f}`" for f in e.files)} | {cell(e.why)} |')
    out.append('')
    order = {s: i for i, s in enumerate(STATUSES)}
    for folder in by_folder:
        mine = sorted((r for r in rows if r['folder'] == folder), key=lambda r: (order[str(r['status'])], r['path']))
        v = by_folder[folder]
        out += [f'## `{folder}`', '', f'{v["files"]} files, {v["lines"]:,} lines.', '',
                '| File | Lines | Kind | Status | Rust counterpart | Callers | Retiring (PX) | Note |',
                '|---|---:|---|---|---|---|---|---|']
        for r in mine:
            rust = cell(r['rust'])
            if r['embedded_by']:
                rust = (rust + '; ' if rust else '') + 'embedded'
            out.append(f'| `{r["path"]}` | {r["lines"]} | {r["kind"]} | {r["status"]} | {rust} | {cell(r["callers"])} '
                       f'| {cell(r["retiring"])} | {cell(r["note"])} |')
        out.append('')
    return '\n'.join(out)


def exception_problems(table: tuple[Row, ...] = TABLE) -> list[str]:
    rows = {r.path: r for r in table}
    out = []
    for e in EXCEPTIONS:
        for f in e.files:
            if f.startswith('opfor:'):
                continue
            r = rows.get(f)
            if r is None:
                out.append(f'{f}: an exception ({e.what}) with no row')
            elif r.retiring != 'stays by decision (OR15)':
                out.append(f'{f}: an exception ({e.what}) whose row retires it in {r.retiring!r}')
    return out


def main(argv: list[str]) -> int:
    problems = table_problems(python_files(), TABLE) + exception_problems()
    if problems:
        sys.stderr.write('python_status: the table does not match the files:\n' + ''.join(f'  {p}\n' for p in problems))
        return 1
    rows = collect()
    tot = totals(rows)
    md = markdown(rows, tot)
    if '--print' in argv:
        sys.stdout.write(md)
        return 0
    data = {'generator': 'rust/python_status.py', 'statuses': list(STATUSES), 'kinds': list(KINDS), 'files': rows,
            'totals': tot, 'exceptions': [e._asdict() for e in EXCEPTIONS]}
    for name, text in ((MD, md), (JSON, json.dumps(data, indent=1, sort_keys=True) + '\n')):
        with open(os.path.join(SUITE, name), 'w', encoding='utf-8', newline='\n') as f:
            f.write(text)
    print(f'{MD}, {JSON}: {len(rows)} files')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
