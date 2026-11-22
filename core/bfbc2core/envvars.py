r"""Every environment variable the suite reads (round QW1, audit AU2 F5): one table, its name, its group and what it
does. tests\test_envvars.py greps the suite's code (Python, Rust, workflows, batch files) for quoted BFBC2_* / BC2_* /
BF1943_* / OPFOR_* names and fails on one missing here, and on a name here that nothing reads any more.

    python -m bfbc2core.envvars                 the table, by group
    python -m bfbc2core.envvars --set           only the ones set in this environment, with their values

Groups: cache (a cache's folder or cap; bfbc2core.caches), log (the log folder's caps), native (the Rust core and
its binaries), switch (the Python reference or a behaviour turned on or off), paths (where something is), workers
(how many processes), test (read only by the tests), build (read at build time by cargo), opfor (read by Project
OPFOR's tools, named here because they are the suite's). A new variable gets a row here in the same commit.
"""
from __future__ import annotations

import os
import sys

# name -> (group, what it does)
VARIABLES: dict[str, tuple[str, str]] = {
    # caches (bfbc2core.caches: the folder, or 0 for none; the cap)
    'BFBC2_CACHE_TOTAL_GB': ('cache', 'the one budget over every cache (bfbc2core.caches)'),
    'BFBC2_FBRB_CACHE': ('cache', "the bundle cache's folder (decompressed FBRB payloads); 0: no disk cache"),
    'BFBC2_FBRB_CACHE_GB': ('cache', "the bundle cache's cap in GB"),
    'BFBC2_FBRB_CACHE_MIN_MB': ('cache', 'the least compressed payload (MiB) the bundle cache puts on disk'),
    'BFBC2_FBRB_MEMORY_MB': ('cache', "the bundle cache's in-process budget in MiB (0 keeps nothing)"),
    'BFBC2_COLLISION_CACHE': ('cache', "the collision index cache's folder; 0: none"),
    'BFBC2_COLLISION_CACHE_MB': ('cache', "the collision index cache's cap in MB"),
    'BFBC2_CULLTREE_CACHE': ('cache', "the cull tree reference rows cache's folder; 0: none"),
    'BFBC2_CULLTREE_CACHE_MB': ('cache', "the cull tree cache's cap in MB"),
    'BFBC2_EFFECTPORT_CACHE': ('cache', "the BF1943 effect port cache's folder; 0: none"),
    'BFBC2_EFFECTPORT_CACHE_MB': ('cache', "the effect port cache's cap in MB"),
    'BFBC2_HLSL_CACHE': ('cache', "the HLSL compile cache's folder (pcbuild); 0: none"),
    'BFBC2_HLSL_CACHE_MB': ('cache', "the HLSL compile cache's cap in MB"),
    'BFBC2_SHADERCOVER_CACHE': ('cache', "the shader coverage index cache's folder; 0: none"),
    'BFBC2_SHADERCOVER_CACHE_MB': ('cache', "the shader coverage cache's cap in MB"),
    'BFBC2_EDITOR_CACHE_MB': ('cache', "the map editor's prepared-level cache's cap in MB"),
    'BFBC2_HARNESS_CACHE_MB': ('cache', "the test harness's project cache cap (OPFOR work\\harness\\cache) in MB"),
    'BFBC2_NAVSTUDIO_CACHE_MB': ('cache', "the navigation data project cache cap (OPFOR work\\navstudio\\cache) in MB"),
    'BFBC2_MESHVIEW_CACHE_MB': ('cache', "the mesh viewer's index project cache cap (OPFOR work\\tools_out\\meshview_index.pkl) in MB"),
    'BFBC2_NAVWATER_CACHE_MB': ('cache', "the boat mesh project cache cap (OPFOR work\\navwater\\cache) in MB"),
    'BF1943_SOURCE_INDEX': ('cache', "the BF1943 bundle index's folder; 0: none (every bundle checked on each open)"),
    'BF1943_SOURCE_INDEX_MB': ('cache', "the BF1943 bundle index's cap in MB"),
    'BF1943_SOURCE_MEMORY_MB': ('cache', 'decoded spans of BF1943 compressed payloads kept in the process (MB)'),
    'BF1943_RESTARTS_MB': ('cache', "the BF1943 Source's decompression restart points kept in the process (MB)"),
    # logs
    'BFBC2_LOGS_MB': ('log', "the log folder's cap in MB"),
    'BFBC2_LOG_ARCHIVE_MB': ('log', "the log archive's cap in MB"),
    'BFBC2_LOG_FILE_MB': ('log', 'one log file before it rolls over, in MB'),
    'BFBC2_LOG_TOOL_MB': ('log', "one tool's logs in MB"),
    # the Rust core and its binaries
    'BFBC2_PURE_PYTHON': ('native', '1: the optional Rust pieces off (their Python twins run); retired pieces still need the module'),
    'BFBC2_CLI_PYTHON': ('native', "1: the ported command lines run their Python instead of bfbc2.exe (no warning)"),
    'BFBC2_STRICT_NATIVE': ('native', '1: a stale or missing bfbc2.exe is an error instead of a fall-back to the Python (round QW1)'),
    'BFBC2_RUST_TARGETS': ('native', "the root of every checkout's cargo build folder (default rust-targets beside the checkouts)"),
    'BFBC2_COMPILE_CACHE': ('native', "1: the suite's cargo runs go through sccache when installed (off by default: it serves rebuilds of one build folder, not other worktrees'; round TS1)"),
    'BFBC2_COMPILE_CACHE_GB': ('cache', "the shared compile cache's cap in GB (sccache's SCCACHE_CACHE_SIZE; default 6)"),
    'BFBC2_RUST_NO_SKIP': ('test', "1: tests\\test_rust.py checks every crate, even one unchanged since its last green run on this PC (run_tests.py full sets it; round TS1)"),
    'BFBC2_NATIVE_REPO': ('native', 'owner/name of the repository build_native.py --download fetches from (a fork)'),
    'BFBC2_PYTHON': ('native', 'the Python bfbc2.exe and the editor hand a command line to (default bin\\python.txt, then PATH)'),
    'BFBC2_CLI_SUITE': ('native', "the suite folder bfbc2.exe's hand-offs run the .py of (default: the one above the binary)"),
    'BFBC2_CLI_TRACE': ('native', 'a file bfbc2.exe appends who answered each command line to (Rust or the hand-off)'),
    'BFBC2_APP_ICON': ('native', 'an .ico used instead of design\\icons\\app.ico (the builds and the hub)'),
    # switches: a Python reference or a behaviour
    'BFBC2_APTGRAFT_REFERENCE': ('switch', "1: aptgraft runs its retired Python reference's recorded rules (parity)"),
    'BFBC2_CRASHREADER_REFERENCE': ('switch', "1: the crash reader's Python reference instead of the Rust core (parity)"),
    'BFBC2_CULLTREE_REFERENCE': ('switch', "1: culltree's reference path (no row cache) for parity runs"),
    'BFBC2_DBXPORT_PYTHON': ('switch', '1: dbxport runs its Python instead of the Rust core'),
    'BFBC2_DBXPORT_DUAL': ('switch', '1: dbxport runs both and raises DualMismatch where they differ (parity runs)'),
    'BFBC2_DBXPORT_REFERENCE': ('switch', '1: dbxport ranks with the reference scan instead of the index (parity)'),
    'BFBC2_DBXPORT_STATS': ('switch', "1: dbxport prints its ranking's statistics"),
    'BFBC2_EFFECTPORT_REFERENCE': ('switch', "1: effectport's Python reference (parity)"),
    'BFBC2_EMITTERATLAS_REFERENCE': ('switch', "1: emitteratlas' Python reference (parity)"),
    'BFBC2_MESHPORT_REFERENCE': ('switch', "1: meshport's Python reference (parity)"),
    'BFBC2_HARNESS_PYTHON': ('switch', "1: the test harness's Python instead of the Rust core"),
    'BFBC2_IMPORTER_PYTHON': ('switch', "1: the BF1943 importer's Python pieces instead of the Rust core"),
    'BFBC2_NAVTOOLS_PYTHON': ('switch', "1: NavKit's tools' Python instead of the Rust core"),
    'BFBC2_STOCKMP_PYTHON': ('switch', "1: stock-MP's Python instead of the Rust core"),
    'BFBC2_SHADERVIEW_SERVE': ('switch', '0: one shaderview process per job instead of shaderview -serve'),
    'BF1943_SOURCE_WHOLE': ('switch', '1: the BF1943 Source reads whole bundles (the reference of the lazy Source)'),
    'BC2_GAMEVIEW_COPY': ('switch', "the editor's game-accurate view reads the frame back instead of sharing the texture"),
    'BC2_RENDER_NO_CB_OFFSETS': ('switch', "the renderer does not use Direct3D 11.1's constant buffer offsets"),
    'BC2_RENDER_UNCACHED': ('switch', "the renderer reads a level's bundles one after another without the bundle cache"),
    # paths
    'BFBC2_DBXEDIT_EXE': ('paths', "a DbxEdit.exe to record answers from (the retired exe's tests)"),
    'BFBC2_FBRBTOOL_EXE': ('paths', "an FbrbTool.exe to record answers from (the retired exe's tests)"),
    'BF1943_XENON': ('paths', "Battlefield 1943's extracted Xbox 360 data (default: beside the suite)"),
    'BF1943_SUITE_FBRB': ('paths', "the BF1943 tool suite's bundle reader folder (bf1943-fbrb-tool)"),
    'OPFOR_ROOT': ('paths', 'the Project OPFOR BFBC2 checkout (default: found beside or above the suite)'),
    'OPFOR_TOOLKIT': ('paths', "the folder holding the suite as Project OPFOR sees it (OPFOR's opforcore.paths)"),
    'OPFOR_GAME': ('paths', "a game folder for levelwrite's export tests (default: OPFOR's game copy)"),
    # workers
    'BFBC2_HLSL_WORKERS': ('workers', "pcbuild's compiler processes"),
    'BFBC2_TEXQA_WORKERS': ('workers', "texqa's worker processes"),
    'BFBC2_XDIFF_WORKERS': ('workers', "xdiff's worker processes"),
    # build
    'BC2_SOURCE_HASH': ('build', "the sources' fingerprint build_native.py passes to cargo; the artefacts bake it in"),
    # opfor
    'BFBC2_MAPEXPORT': ('opfor', "a mapexport.exe for Project OPFOR's mapedit_install (default: the suite's bin\\)"),
    # tests only
    'BFBC2_CLI_WORKERS': ('test', 'the command-line parity tests (O2 to O5): processes at a time'),
    'BFBC2_O2_REF': ('test', 'the reference checkout of the O2 / O4 parity tests'),
    'BFBC2_O3_REF': ('test', 'the reference checkout of the O3 parity tests'),
    'BFBC2_O5_REF': ('test', 'the reference checkout of the O5 parity tests'),
    'BFBC2_O5_LEVELS': ('test', "the levels O5's stock-MP builds run on"),
    'BFBC2_O5_STANDIN': ('test', "the stand-in OPFOR folder of O5's stock-MP builds"),
    'BFBC2_PACK_TEST_WORKERS': ('test', "the FBRB packer test's processes"),
    'BFBC2_UNPACK_TEST_MIN_MB': ('test', "the FBRB unpack test's smallest payload"),
    'BFBC2_UNPACK_TEST_MAX_MB': ('test', "the FBRB unpack test's largest payload"),
    'BFBC2_SD1_SIBLINGS_OF': ('test', "the folder whose siblings hold BF1943's copy for the SD1 test"),
    'BFBC2_SUITE_REFERENCE': ('test', 'the reference checkout of the path locator test'),
    'BFBC2_TEST_SCRATCH': ('test', 'where the replay and argfmt tests write their scratch files'),
    'BFBC2_TEST_STAND_IN_EXE': ('test', "1: bfbc2.exe's menu tools read an exe stand-in kept without its MZ / PE "
                                'signatures (SI) as the program (round AV1: O2 writes no synthetic program '
                                'Microsoft Defender could block)'),
    'BC2_INSTALL_CRASH_AT': ('test', "bfbc2 install stops the process (exit 9) before its n-th write step of the run (a "
                             'level bundle, a record, a marker or a ledger step), as a kill would (round OR1)'),
    'BC2_LEDGER_CRASH_AT': ('test', "the install ledger stops the process (exit 9) before its n-th write step, as a "
                            'kill would (round DF2)'),
    'BC2_EDITOR_SHOTS': ('test', "the editor tests save their screenshots there"),
    'BC2_EDITOR_COLLISION_VIEW': ('test', "1: the simplified view stays on collision (round SV1's before pictures)"),
    'BC2_EX1_STAGE': ('test', "EX1's names test keeps its staged canary export in this folder"),
    'BC2_GW1_LEVELS': ('test', "GW1's WW2 game-view test on these levels (default guadal_canal,iwo_jima_s)"),
    'BC2_GW1_OPFOR': ('test', "GW1's WW2 game-view test uses this stand-in OPFOR folder (default: one made in temp)"),
    'BC2_GW1_PACKAGE_LEVELS': ('test', "GW1's weather test on these Package levels (default nam_mp_002cq,ons_mp_002)"),
    'BC2_TEST_HANG_S': ('test', "the editor tests' hang guard: a test running longer (s) ends the binary, named"),
    'BC2_TEST_HANG_STACKS': ('test', "1: the hang guard prints every thread's stack first (rust\\hang_stacks.py)"),
    'BC2_XF1_STAGE': ('test', "XF1's cross-set export test keeps its staged export in this folder"),
    'BC2_MEMBER_EDIT_STAGE': ('test', "the member-edit export test keeps its staged exports (one folder per level) here"),
    'BC2_EDITOR_PREPARE': ('test', "the editor's level prepare test on this level (a stand-in OPFOR folder)"),
    'BC2_EDITOR_G3': ('test', "G3's real Package levels test on these levels"),
    'BC2_EDITOR_G3_ART': ('test', "G3's content browser art test on"),
    'BC2_EDITOR_G3_KEEP': ('test', "G3 keeps its junctions and folders"),
    'BC2_EDITOR_G3_OUT': ('test', "G3's output folder"),
    'BC2_EDITOR_G3_PROJECTS': ('test', "G3's New map projects on these Package levels"),
    'BC2_EDITOR_G3_ROOT': ('test', "G3's stand-in root (default: a temp folder)"),
    'BC2_EDITOR_G3_SHOTS': ('test', "G3's screenshots folder"),
    'BC2_EDITOR_G4_REAL': ('test', "1: G4's start flow on the real game copy"),
    'BC2_EDITOR_G4_SHOTS': ('test', "G4's screenshots folder"),
    'BC2_PROJECTS': ('test', "a folder of canary map projects levelwrite's version test loads"),
    'BC2_JOURNAL_KILL_CHILD': ('test', "the autosave journal test's child process: writes the journal until it is killed"),
    'BC2_R5C_PACKAGE_LEVELS': ('test', "R5c's Package levels drawn in the game-accurate view"),
    'BC2_R5C_PACKAGE_OPFOR': ('test', "R5c's OPFOR folder for the Package levels test"),
}

# Quoted names in the code that are not environment variables (test_envvars.py skips them).
NOT_VARIABLES = {
    'OPFOR_TEST_NEW_KEY': 'a localization key bc2loc writes in its test',
}


def table(only_set: bool = False) -> str:
    rows = []
    for group in ('cache', 'log', 'native', 'switch', 'paths', 'workers', 'build', 'opfor', 'test'):
        names = sorted(n for n, (g, _) in VARIABLES.items() if g == group and (not only_set or n in os.environ))
        if not names:
            continue
        rows.append(f'{group}:')
        for n in names:
            value = f' = {os.environ[n]}' if only_set else ''
            rows.append(f'  {n}{value}  {VARIABLES[n][1]}')
    return '\n'.join(rows)


if __name__ == '__main__':
    print(table('--set' in sys.argv[1:]) or '(none set)')
