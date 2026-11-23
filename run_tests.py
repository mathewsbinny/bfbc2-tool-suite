r"""Runs the suite's tests: one line per test, the failures' output at the end, exit code 1 if any failed.

  python run_tests.py [quick | game | full] [--only <name>] [--list] [-j N] [--cpus N] [--mem GB] [--plan] [--times F]
                      [--timeout S]
  python run_tests.py quick --no-game [--ci-steps] [-j N]
  python run_tests.py --help   this text (an argument the runner does not know is an error, exit code 2)

  --only NAME  the tests whose name contains NAME (any case); matching none is an error (exit code 2)
  --timeout S  a test still running after S seconds is ended with its processes and fails; by default 4 times its
               kept duration on this PC, at least 30 min (quick), 3 h (game) or 8 h (full); 0: no limit

  --no-game    only the quick entries that need nothing but the checkout (CI, .github/workflows/checks.yml): no game
               copy, OPFOR folder, BF1943 / Xbox copies or graphics card; the others (NOT_GAME_FREE) are named with
               the reason they are skipped
  --ci-steps   leaves out the entries CI runs as steps of its own (CI_STEPS): cargo fmt, clippy and the workspace's
               tests, ruff, mypy

  quick   no game data, about a minute: the Rust core (built when stale, its cargo checks, its DBX reader against
          the retired Python reader's recorded answers), repository rules, and on files built by the tests:
          DbxEdit's and FbrbTool's commands against the retired exes' recorded answers, the crash reader and
          exeref, the console readers, the menu tools, the test harness; leveldiff's parity with the retired
          LevelDiff.exe is bc2-cli's cargo tests, the DBX inspector's with the retired DbxEditGUI bc2-editor's
  game    + Project OPFOR BFBC2's game copy, about 13 minutes: the DBX reader against the retired Python reader's
          recorded answers and DbxEdit's fingerprints on every unpacked .dbx, FbrbTool's commands on 12 game
          bundles, water on every level,
          the level loader against its snapshot, terrain against the in-game samples, path-data and mesh
          round trips, the crash reader's code view and exeref on BFBC2Game.exe, the menu
          tools on the game's movies and OPFOR's menu patches, the console
          readers on your own Xbox 360 files (skipped when they are not on the PC), the 3D tools opening a
          level (needs a display), the structure of DICE's path data (hpdcheck), the test harness on two small
          levels, NavKit's regression test on two small levels
  full    + the corpus runs, about 90 minutes: the DBX reader on every unpacked .dbx, FbrbTool's commands on every game bundle, NavKit's regression test on the 13 campaign levels and the imported
          Onslaught maps (about 5 hours)

Run `quick` before every commit and `game` before every push; `full` after changes to a format reader.

Side by side (2026-11-17). Tests run N at a time (-j N; default half the CPU threads, at most 8); -j 1 runs them one
after another in the listed order, exactly as before. Each test declares what it needs in NEEDS (below tests()):
  cpu     the CPU threads it keeps busy (1; its process pool's size; 'all' for Rust batch work and cargo)
  mem     its peak memory in GB (an estimate; the runner measures every test's peak and keeps it, see --plan)
  res     shared things it reads ('r') or writes ('w'): a writer never runs beside a reader or another writer of the
          same thing (OPFOR's work\navkit\regress, the navigation data cache, renderqa's build\ folder, a display ...),
          and they keep their listed order, so each sees the thing as it did one at a time
  setup   a build step: the build steps run first, one at a time, before any test (tests load what they build)
  alone   nothing else runs beside it; every test missing from NEEDS runs alone, so a new entry is safe before it is
          declared (and the runner names it)
The runner starts a test when a slot is free, the CPU threads (--cpus, default all) and memory (--mem, default 3/4 of
the PC's) it declares are free, and nothing it conflicts with is running; the longest tests go first, and a test
held back for room is not overtaken by shorter ones that would delay it. Output is the same: one line per test as it
finishes, the failures' output at the end (in list order), the total, exit code 1 if any failed.

  --plan       prints the schedule and the expected wall-clock without running anything, from the durations kept
               after each run (%LOCALAPPDATA%\bfbc2-tool-suite\test_times.json), else tests\test_durations.json
  --times F    durations from F instead: a run_tests.py log or a JSON of name -> seconds

Round QW1 (audit AU2). Skips: a test whose summary line (its last) starts with "skipped" ran nothing on this PC (no
game copy, no Rust, no graphics adapter): its line says skip, the total counts it as skipped, not passed, and its time
is not kept; a passing test's other "skipped" lines (parts it could not run) are counted and named after the
results. Memory: a test is scheduled with the larger of its NEEDS declaration and its last recorded peak on this PC.
Kept durations of names no tier has any more go after STALE_DAYS. matplotlib's config and font cache are the suite's
own (MPLCONFIGDIR: %LOCALAPPDATA%\bfbc2-tool-suite\matplotlib), built before the first test, so no test's stderr
carries its "building the font cache" message.
"""
import contextlib
import json
import os
import subprocess
import sys
import time

SUITE = os.path.dirname(os.path.abspath(__file__))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

PY = sys.executable
TIERS = ['quick', 'game', 'full']


def opfor():
    suite_paths.use('navkit')
    import navkit
    return navkit.find_opfor(None)


HPD_BACKUPS = ('pathdata.hpd.opfor-orig', 'pathdata.hpd.opfor-stockmp-orig')
HPD_FILES = ('pathdata.hpd',) + HPD_BACKUPS


def dice_pathdata_files(root):
    """Original DICE data: navkit.original_file's answer for each folder's pathdata.hpd - the oldest kept backup
    of any OPFOR stage, else the file unless a stage installed it onto nothing (suite deep scan 2026-11-15, C5: this
    took .opfor-orig first, which is stock-MP's file when stock-MP installed before NavKit)."""
    suite_paths.use('navkit')
    import navkit
    out = []
    for dp, _, fs in os.walk(os.path.join(root, 'game', 'Output', 'win32', 'ai')):
        names = {f.lower(): f for f in fs}
        if not any(n in names for n in HPD_FILES):
            continue
        original = navkit.original_file(os.path.join(dp, names.get('pathdata.hpd', 'pathdata.hpd')))
        if original is not None:
            out.append(original)
    return sorted(out)


def pathdata_files(root):
    """Every HPD and kept original in the game and work folders, excluding markers and reports."""
    out = []
    for top in (os.path.join(root, 'game', 'Output', 'win32', 'ai'), os.path.join(root, 'work')):
        for dp, _, fs in os.walk(top):
            out += [os.path.join(dp, f) for f in fs if f.lower() in HPD_FILES]
    return sorted(out)


TEST_RUST = os.path.join(SUITE, 'tests', 'test_rust.py')
RUST_BUILD = 'Rust core: the tests built (cargo test --no-run) and the doc tests'
RUST_QUICK = 'Rust core: the quick tests of every crate (no whole levels)'
RUST_CLIPPY = 'Rust core: clippy, warnings as errors'
RUST_FMT = 'Rust core: cargo fmt --check'
RUST_GAME = 'Rust core: the game tests (whole levels: the editor, levelwrite and the renderer)'


def tests():
    """(id, name, tier, argv). Game paths are only looked up for the tiers that need them.

    The id is the entry's stable key (round PR1a): NEEDS, CI_STEPS, NOT_GAME_FREE, the kept durations
    (test_times.json, tests\\test_durations.json) and tests\\test_repo.py's allowlists are keyed by it, so the
    display name can be reworded (or a round code taken out of it) without losing any of them. An id is lower case,
    words joined by '-', an optional ':variant' (game, live, xbox ...), named by what the entry tests, never by a
    round; it is never changed or reused (test_repo.py checks the form and that ids are unique)."""
    t = [
        ('rust-module-build', 'Rust core build (when stale; skipped without Rust)', 'quick', [PY, os.path.join(SUITE, 'rust', 'build_native.py'), '--if-stale']),
        # the Rust core's checks as four entries (round TS1, audit AU3 TS-1): the build first, then the test run beside
        # clippy and the Python entries; crates unchanged since their last green run on this PC are left out (said)
        ('rust-build', RUST_BUILD, 'quick', [PY, TEST_RUST, 'build']),
        ('rust-quick', RUST_QUICK, 'quick', [PY, TEST_RUST, 'run']),
        ('rust-clippy', RUST_CLIPPY, 'quick', [PY, TEST_RUST, 'clippy']),
        ('rust-fmt', RUST_FMT, 'quick', [PY, TEST_RUST, 'fmt']),
        ('rust-green-crates', "the Rust checks' green crates: fingerprints per crate, the green record under concurrent runners, skip lines",
         'quick', [PY, os.path.join(SUITE, 'tests', 'test_rust_skip.py')]),
        ('dbx-native', "DBX reader (Rust) against the retired Python reader's recorded answers (built, damaged, 200 corpus files)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx_native.py')]),
        ('fbrb-native', "FBRB reader (Rust) against the retired Python reader's recorded answers and FbrbTool.exe's recorded unpacks (built, damaged, 8 game bundles)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_native.py')]),
        ('fbrb-cache', "Rust FBRB bundle cache against the reader without it and the Python reader's recorded answers (built, damaged, changed, 4 processes, 3 game bundles)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_cache.py')]),
        ('geometry-native', "terrain, water and collision readers (Rust) against the retired Python readers' recorded answers (built, damaged, aliases from binary partitions)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_geometry_native.py')]),
        ('shaderdb-native', 'Rust shader database reader / writer against shaderdb.py (built, damaged, writer fallback, merge, mp_008cq)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_native.py')]),
        ('shaderdb-merge', "Rust shader merge (bc2_native.ShaderMerger) against the retired Python Merger's recorded answers (built, damaged, 240 random cases)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_merge.py')]),
        ('shaderdb-levels', "Rust retexture, level cover index and level scan against the retired Python's recorded answers and the Python queries (24 stand-in game copies, 418 retextures)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_levels.py')]),
        ('shaderdb-queries', 'Rust shader database handle and queries against shaderdb.py (never made, made, changed lists; blocks, checks, writer, mp_008cq)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_queries.py')]),
        ('xenos-native', 'Rust shader translator: decoder against xenos.py, translations against the recorded Python (built programs, damaged blobs, random microcode, dispatch, hand-built trees)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xenos_native.py')]),
        ('culltree-native', "cull tree element reader (Rust) against the retired Python reader's recorded answers, level search against glob (built, damaged, listing, 2 work trees)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree_native.py')]),
        ('granny-native', 'Rust Granny reader against granny.py, Xbox 360 -> PC conversion against its recorded results (built, damaged, batch, writes)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_native.py')]),
        ('texture-native', 'Rust texture converter against its recorded results (built, damaged, batch)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture_native.py')]),
        ('audio-native', "Rust EA-XAS audio encoder / decoder against its recorded results (shift rule, seeded PCM of every shape, random frames, DICE's blocks)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_audio_native.py')]),
        ('navbuild-native', 'Rust NavKit grid walks against their recorded results, E2a / E3c / E3h pieces against their Python (random levels, whole stages)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navbuild_native.py')]),
        ('navtools-native', "E4e: NavKit's tools in the Rust core against their Python (vehicle obstacle distance, domain searches, level scan, parity measures; built inputs)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navtools_native.py')]),
        ('appicon', "application icon: the lookup, the hub's window icon, the icon in bin\\'s .exe files", 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_appicon.py')]),
        ('native-required', 'Rust core required: no module, a stale one, the source fingerprint', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_native_required.py')]),
        ('review-regressions', 'review regressions: HPD selection, launcher completion, mypy failures', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_review_regressions.py')]),
        ('asset-accuracy', 'BF1943 asset accuracy: texture palettes, masks, emitters, shader fetches', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_asset_accuracy.py')]),
        ('repo', 'repository rules', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_repo.py')]),
        ('envvars', 'environment variables: every BFBC2_ / BC2_ / BF1943_ / OPFOR_ name the code reads is registered (bfbc2core.envvars)', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_envvars.py')]),
        ('spec-trace', 'specification: rule index and citations (spec/trace.py --check)', 'quick', [PY, os.path.join(SUITE, 'spec', 'trace.py'), '--check']),
        ('native-census', "the Rust core's Python module: every export has a caller (a suite tool or test, or Project OPFOR's files)", 'quick',
         [PY, os.path.join(SUITE, 'rust', 'native_census.py'), '--check']),
        ('runner-output', 'test runner: a failure printed on any console encoding', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_runner_output.py')]),
        ('runner-parallel', 'test runner: tests side by side (resources, failures, exit code, output per test)', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_runner_parallel.py')]),
        ('runner-args', 'test runner: its command line (--help, unknown arguments run nothing)', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_runner_args.py')]),
        ('lint', 'lint (ruff check, every folder)', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_lint.py')]),
        ('types', 'types (mypy, every folder)', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_types.py')]),
        ('core-edges', 'core edges (terrain / water names, errors in threads)', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_core_edges.py')]),
        ('paths', 'paths: OPFOR, game copy, tools and sibling projects on a stand-in tree', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_paths.py')]),
        ('converter-edges', 'converter edges (dbx numbers, dbxport references, texture check)', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_converter_edges.py')]),
        ('converter-units', 'converter units (havok w lanes, MOPP byte code, L8 textures and flags, mesh rename, texcoords, mesh relayout)', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_converter_units.py')]),
        ('fbrbtool-roundtrip', 'FbrbTool round trip, built bundle', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb.py')]),
        ('dbx-write-native', "DBX writer (Rust) against the retired Python writer's recorded bytes (edge, random and edited trees, 200 corpus files, DbxEdit roundtrip)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx_write_native.py')]),
        ('dbxedit', "DbxEdit's commands in process against DbxEdit.exe's recorded answers (the unit tests' 82 cases, the drop-in runner, the batch, the log)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbxedit.py')]),
        ('fbrbtool', "FbrbTool's commands in process against FbrbTool.exe's recorded answers (228 command lines, 100 damaged bundles, .NET's gzip reader, the drop-in runner)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrbtool.py')]),
        ('cli-fbrbtool-dbxedit', "The command lines in Rust (bfbc2.exe fbrbtool / dbxedit) against the Python command lines' recorded answers (every recorded line, alone and on one pipe, code pages, stdio switches)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o1.py')]),
        ('cli-menu-crash-navkit-convert', "O2: the menu, crash-reader and NavKit / converter command lines in Rust (bfbc2.exe aptcheck ... aimanim) against the Python command lines' recorded answers (968 lines, alone and on one pipe, stdio switches; who answered counted)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o2.py')]),
        ('cli-caches-stockmp-graft-meshview-cull', "O3: the caches, stock-MP, aptgraft, audio, BF1943 parity / catalogue, meshview and culltree command lines in Rust (bfbc2.exe caches ... culltree) and the log folder's start-up pruning against the Python command lines' recorded answers (alone and on one pipe, stdio switches, local app data; who answered counted)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o3.py')]),
        ('cli-navkit-prepare-harness-texqa', "O4: NavKit's driver and tools, the editor's prepare, the test harness and texqa in Rust (bfbc2.exe navkit ... texqa) against the Python command lines' recorded answers (alone and on one pipe, stdio switches; who answered counted)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o4.py')]),
        ('cli-readers-shaders-import-stockmp', "O5: the console readers, the shader tools (shaderdb with its merge, renderqa, xdiff), the BF1943 importer, NavKit's detailed choice and status, and stockmp's generate / build / validate / all in Rust (bfbc2.exe apt ... stockmp) against the Python command lines' recorded answers (alone and on one pipe, stdio switches, local app data; who answered counted)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o5.py')]),
        ('fbrb-pack-native', "Rust FBRB packer against FbrbTool.exe's recorded bundles, zlib against Python's (built folders in every layout, 8 game bundles)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_pack_native.py')]),
        ('fbrb-unpack-native', "Rust FBRB unpack against FbrbTool.exe's recorded unpacks (built, odd and damaged bundles, 8 game bundles)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_unpack_native.py')]),
        ('errors', 'error handling and logs', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_errors.py')]),
        ('caches', 'cache caps and pruning (bfbc2core.caches): least recently used first, writers and readers beside it, the command', 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_caches.py')]),
        ('logs-caches', 'log caps and day rollover, project caches in work\\ (bfbc2core.caches), the command', 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_logs_caches.py')]),
        ('cli-errors', 'every command-line tool reports errors', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_cli_errors.py')]),
        ('review-fixes', 'review fixes: apply inputs, stock-MP reinstall, cache key', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_review_fixes.py')]),
        ('review-fixes2', 'review fixes 2: harness missing files, navkit no-original reinstall, old fbrb manifests', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_review_fixes2.py')]),
        ('deep-scan-fixes', 'deep scan fixes: atomic installs and writes, honest caches, damaged files said', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_deep_scan_fixes.py')]),
        ('gameledger', 'install ledger (DF2): DS-1, every order of two tools, an interruption at every write step, the real installs',
         'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_gameledger.py')]),
        ('navtune', 'navigation side files (navtune)', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_navtune.py')]),
        ('water-rule', 'NavKit water rule (ground too deep under water)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_water_rule.py')]),
        ('navedits', 'NavKit walkability edits (navedits)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navedits.py')]),
        ('regress-progress', 'NavKit regression progress estimates', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_regress_progress.py')]),
        ('navboundary', 'NavKit boundary diagnostic (navboundary)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navboundary.py')]),
        ('reference-clip', 'NavKit reference extent clip', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_reference_clip.py')]),
        ('characters', 'NavKit soldiers absent from path data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_characters.py')]),
        ('headroom', 'NavKit headroom band', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_headroom.py')]),
        ('lips', 'NavKit narrow lips and door sills', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_lips.py')]),
        ('dynamic-props', 'NavKit dynamic props (floor under a prop, door leaves)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_dynamic_props.py')]),
        ('roof-prune', 'NavKit roof pruning', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_roof_prune.py')]),
        ('objective-floor', 'NavKit objective floors (M-COM)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_objective_floor.py')]),
        ('detail-features', 'NavKit D lines and F cells', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_detail_features.py')]),
        ('havok-index', 'NavKit collision index (Havok packfile names)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_havok_index.py')]),
        ('navwater-mesh', 'NavKit boat mesh (navwater_mesh)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navwater_mesh.py')]),
        ('navdomains', 'NavKit routes across domains (navdomains)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navdomains.py')]),
        ('vehicle', 'NavKit vehicle path data (navvehicle)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_vehicle.py')]),
        ('navislands', 'NavKit shadow islands (navislands)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navislands.py')]),
        ('navsample-offline', 'NavKit offline sampler (navsample_offline)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navsample_offline.py')]),
        ('tool-edges', 'NavKit tool edges (ordercheck, detailed cache, navdiff pictures)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_tool_edges.py')]),
        ('overlay-offline', 'overlay exports, no game (AI layer, anomalies, usage)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-navigation-studio', 'tests', 'test_overlay_offline.py')]),
        ('navkit-offline', 'NavKit driver, stand-in OPFOR folder (navkit.py steps, install, main)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navkit_offline.py')]),
        ('meshview-index', 'mesh viewer index and cache, stand-in bundles', 'quick', [PY, os.path.join(SUITE, 'bfbc2-mesh-viewer', 'tests', 'test_meshview_index.py')]),
        ('crashreader', 'crash reader, built reports', 'quick', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_crashreader.py')]),
        ('console-readers', 'console readers, built images', 'quick', [PY, os.path.join(SUITE, 'bfbc2-console-readers', 'tests', 'test_console_readers.py')]),
        ('menu-tools', 'menu tools (aptcheck, aptrefs, bc2loc), built data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_menu_tools.py')]),
        ('aptgraft', 'APT writer and grafter (aptgraft), built movies', 'quick', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_aptgraft.py')]),
        ('apt-decode-graft-native', 'APT decoder (Rust) and aptgraft writer against their references (E3n; built movies, 600 damaged, 3,000 floats, grafts)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_apt_e3n.py')]),
        ('apt-model-check-refs-native', 'APT model, aptcheck and aptrefs in Rust against the Python (E4c; built movies, object rules, 300 damaged, grafts, edits, crosshair-add)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_apt_e4c.py')]),
        ('culltree', 'static cull trees (culltree), built data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree.py')]),
        ('culltree-rows-cache', 'cull tree reference rows cache and shape memo against the reference (E3m; built levels: no cache, cold, warm, changed)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree_e3m.py')]),
        ('navmesh-cells-routes-native', 'Rust E3o: mesh reader, navdata cells and union-find, route search, texqa workers against their references, havok verify record against its recorded results (built inputs)', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_e3o_native.py')]),
        ('pathdata-native', 'Rust F0 path data and runtime-mesh writer against hpdtool / kymesh (built files and meshes, 1,500 damaged, 600 changed, 4 game files)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_pathdata_native.py')]),
        ('bc2loc-native', 'Rust F0 localization files and BIGF archives against bc2loc / menucommon (built, damaged, 3,000 keys, the English file, the menu movies)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_bc2loc_native.py')]),
        ('console-readers-native', 'Rust F0 console containers against stfs.py / xiso.py (built packages and discs, 60 damaged packages, 30 damaged discs)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-readers', 'tests', 'test_console_readers_native.py')]),
        ('wave-native', 'Rust F0 Wave resources and EA streams against audio.py (built, 3,000 damaged, 600 changed, 40 BF1943 Waves)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_wave_native.py')]),
        ('meshdata-native', 'Rust F0 mesh headers and mesh sets against the recorded answers of the retired mesh.py reference and the meshset.write Python (built, 3,000 damaged, 800 changed, 60 BF1943 resources)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_meshdata_native.py')]),
        ('texture-helpers-native', "Rust F0 texture.py helpers and PC texture reader against their Python (tiled, packed, layout) and the recorded answers of the retired levels / decode (built, damaged)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture_helpers_native.py')]),
        ('havok-classes-native', 'Rust F0 havok.classes against the recorded answers of its retired Python (built and damaged executables, BFBC2Game.exe)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_havok_classes_native.py')]),
        ('console-convert-native', 'Rust F2a console converter against the recorded answers of the retired terrain.py / soot.py / mesh.py / havok.py / texture.py references (built, damaged, edited relayouts, 40 BF1943 resources)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_console_f2a_native.py')]),
        ('model-tools-native', 'Rust F2a model tools against texwrite.py / skinmesh.py (built pictures and skinned LODs, damaged, the Garand and Type 5 LODs)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_model_f2a_native.py')]),
        ('granny-write-native', "Rust F2a Granny writing against granny.py (CRC, shift, orientations, add_tracks on the WW2 rifles' animations, damaged)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_f2a_native.py')]),
        ('argfmt', "bfbc2.exe argparse help and usage errors at the console width (COLUMNS) against the recorded answers of the Python (41 command lines x 23 values; the generated parser pieces)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_argfmt.py')]),
        ('abbrev', "bfbc2.exe argparse option abbreviations and their ambiguity errors against the recorded answers of the Python (every parser's prefixes)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_abbrev.py')]),
        ('aim-pathmesh-audio-native', 'Rust F3 formats against aimanim / hpdtool mesh layout / audio framing / navsample / hkpack / xenos.disasm (built, damaged, changed, a few real files)', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_f3_native.py')]),
        ('culltree-strings-disasm-native', "Rust F4 against culltree's string table / xenos.disasm's header / hkpack's handle / asset_bodies / texqa / audio.convert (built, damaged, edited, a few real files)", 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_f4_native.py')]),
        ('shaderdb-open-native', "Rust SD1 against shaderdb.py's ShaderDb.parse dispatch / pick_database / dx_from_name / load (built databases and bundles, damaged, every kind of argument)", 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_sd1_native.py')]),
        ('exeref', 'exe string / address references (exeref), built exe', 'quick', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_exeref.py')]),
        ('harness', 'test harness, built data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'tests', 'test_harness.py')]),
        ('crash-native', 'crash reader, exeref and test harness: the Rust core against the reference (seeded x86, built exes, logs, graphs)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_crash_native.py')]),
        ('stockmp', 'stock-MP AI layer, synthetic levels', 'quick', [PY, os.path.join(SUITE, 'bfbc2-stockmp-ai', 'tests', 'test_stockmp.py')]),
        ('validate', 'stock-MP AI validation checks, synthetic builds (validate.py)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-stockmp-ai', 'tests', 'test_validate.py')]),
        ('shaderdb', 'shader databases (shaderdb), built files', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb.py')]),
        ('shaderdb:xbox', 'shader databases: Xbox 360 (BFBC2 twins, BF1943 Wake)', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb.py'), '--xbox']),
        ('renderqa', 'render harness (renderqa): built mesh, shaders and texture drawn and checked', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_renderqa.py')]),
        ('renderqa:game', 'render harness (renderqa): stock meshes with their own level\'s shaders', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_renderqa.py'), '--game']),
        ('xdiff', 'shader translator differential test: Xenos interpreter semantics, cube fetch direction', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xdiff.py')]),
        ('pcbuild-batch', 'HLSL compile cache and batches (pcbuild): deterministic compiler, compile_many / prefetch, disk cache', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_pcbuild_batch.py')]),
        ('shaderview-serve', 'shaderview -serve against one process per job (built jobs, failures, state between jobs, threads, memory)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderview_serve.py')]),
        ('xdiff:game', 'shader translator differential test: BF1943 shaders behind each translator fix exact', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xdiff.py'), '--game']),
        ('pcbuild-batch:game', 'HLSL batches (pcbuild): every Wake pixel shader translated, compile_many against one at a time', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_pcbuild_batch.py'), '--game']),
        ('xdiff:game-all', 'shader translator differential test: every Wake pixel shader served on threads against one process each, one at a time', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xdiff.py'), '--game-all']),
        ('granny-port', 'Granny Xbox 360 -> PC: DICE bone tables', 'quick', [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_port.py')]),
        ('granny-curves', 'Rust granny.curve_keys / sample against the retired Python (built curves of every format)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_curves.py')]),
        # round TS1 (audit AU3 TS-8): three test files no tier ran, two of them cited by the specification
        ('aimanim', 'aim animations (aimanim): a made-up animation written, read in both byte orders and renumbered', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_aimanim.py')]),
        ('trees', 'triangle strips and tree mesh sets (strip_to_list, index_count)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_trees.py')]),
        ('bf1943import', 'BF1943 import pipeline, no data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_bf1943import.py')]),
        ('dbxport-rules', 'dbxport rules: the 19 edge cases, templates, references, schema, value passes', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_rules.py')]),
        ('dbxport-ranking', 'dbxport ranking index against the reference scan (20,000 random rankings, Templates.choose)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_ranking.py')]),
        ('dbxport-tables', 'dbxport tables in Rust against the Python reference: every entry, changed in place as OPFOR changes them', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_tables.py')]),
        ('dbxport-native', 'dbxport in Rust against its Python reference (600 random partitions, pools, catalogues, options)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_native.py')]),
        ('ports-native', 'BF1943 ports E3g: Rust mesh vertices and Havok walk against their recorded results, layout and emitter indexes against their references (built inputs)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_ports_native.py')]),
        ('havokport-native', 'F2b: mopp and havokport in Rust against the recorded answers of their retired Python (seeded MOPP codes, 4.6.1 packfiles, PC resources)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_havokport_native.py')]),
        ('dbxcatalogue-native', 'F2b: dbxcatalogue.build in Rust against its Python (seeded games of built bundles)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxcatalogue_native.py')]),
        ('emitteratlas-native', 'F2b: emitteratlas build, prove and same_but_uv in Rust against their Python (40 synthetic levels)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_emitteratlas_native.py')]),
        ('pe-native', 'F2b: menucommon.PEImage in Rust against the recorded answers of its retired Python (400 built executables)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_pe_native.py')]),
        ('xmlinst-native', 'F2b: xmlinst in Rust against the recorded answers of its retired patterns (3,000 seeded texts)', 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_xmlinst_native.py')]),
        ('navtune-native', 'F2b: navtune validate, side-file text and summary in Rust against the recorded answers of its retired Python (seeded operations and files)', 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_navtune_native.py')]),
        ('dbxport-fair', 'dbxport fair ports, 7 partitions against DICE (skipped without game data)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_fair.py'), '--quick']),
        ('emitteratlas', 'emitter atlases merged, emitters re-pointed (synthetic)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_emitteratlas.py')]),
        ('texqa', 'texture QA: decoding, Xbox untiling, flips / swizzles / degenerate, baseline (synthetic)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texqa.py')]),
        ('dbx-native:game', "DBX reader (Rust) against the retired Python reader's recorded answers, every corpus file", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx_native.py'), '--game']),
        ('rust-dbx-corpus', 'Rust DBX reader against DbxEdit, every fingerprinted file; every bundle extracted (cargo)', 'game',
         [PY, TEST_RUST, '--corpus']),
        ('rust-game', RUST_GAME, 'game', [PY, TEST_RUST, 'run', '--game']),
        ('fbrb-native:game', "FBRB reader (Rust) against the retired Python reader's recorded answers, every bundle", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_native.py'), '--game']),
        ('fbrb-cache:game', 'Rust FBRB bundle cache: every 3rd game bundle read in a random order, the stored bundles', 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_cache.py'), '--game']),
        ('dbx-write-native:game', "DBX writer (Rust) against the retired Python writer's recorded bytes, every corpus file written back and edited", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx_write_native.py'), '--game']),
        ('dbxedit:game', "DbxEdit's commands in process against DbxEdit.exe's recorded answers, 1,592 corpus files through every command and every edit fingerprint", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbxedit.py'), '--game']),
        ('fbrbtool:game', "FbrbTool's commands in process against FbrbTool.exe's recorded answers, 40 game bundles unpacked, listed and packed, 600 damaged bundles, the logged invocations' bundles", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrbtool.py'), '--game']),
        ('cli-fbrbtool-dbxedit:live', "The command lines in Rust (bfbc2.exe) against the Python command lines, both ways live, and Project OPFOR's real FbrbTool / DbxEdit calls replayed read-only", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o1.py'), '--live', '--game']),
        ('cli-menu-crash-navkit-convert:live', "O2: the menu, crash-reader and NavKit / converter command lines in Rust against the Python command lines, both ways live, and Project OPFOR's real calls replayed read-only", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o2.py'), '--live', '--game']),
        ('cli-caches-stockmp-graft-meshview-cull:live', "O3: the caches ... culltree command lines in Rust against the Python command lines, both ways live, and Project OPFOR's real calls replayed read-only", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o3.py'), '--live', '--game']),
        ('cli-navkit-prepare-harness-texqa:live', "O4: NavKit's driver and tools, the harness and texqa in Rust against the Python command lines, both ways live, and Project OPFOR's real calls replayed read-only", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o4.py'), '--live', '--game']),
        ('cli-prepare-level:game', "O4: the editor's level prepare in Rust (bfbc2.exe prepare_level) against the retired prepare_level.py's recorded answers on 3 stock levels in a stand-in OPFOR folder: every file under work\\ (sha256), the output and the day logs", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_prepare_o4.py'), '--game']),
        ('cli-readers-shaders-import-stockmp:live', "O5: the readers, shader tools, BF1943 importer, NavKit's detailed / status and stockmp in Rust against the Python command lines, both ways live, Project OPFOR's real calls replayed read-only, and stockmp's all byte for byte on 3 stock levels in a stand-in OPFOR folder", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_cli_o5.py'), '--live', '--game']),
        ('fbrb-pack-native:game', "Rust FBRB packer against FbrbTool.exe's recorded bundles, every bundle of the game copy unpacked and packed again", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_pack_native.py'), '--game']),
        ('fbrb-unpack-native:game', "Rust FBRB unpack against FbrbTool.exe's recorded unpacks, every game bundle with a payload up to 300 MiB", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_unpack_native.py'), '--game']),
        ('geometry-native:game', "terrain, water and Havok readers (Rust) on every bundle of the game copy, the collision index of every prepared level, against the retired Python readers' recorded answers", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_geometry_native.py'), '--game']),
        ('shaderdb-merge:game', "Rust shader merge against the retired Python Merger's recorded answers on the game copy (ONS_MP_008 + mp_008cq, Dx9 / 10 / 11)", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_merge.py'), '--game']),
        ('shaderdb-levels:game', "Rust level cover index and level scan on every level of the game copy (Dx9 / 10 / 11) against the retired Python index's digests and the Python queries", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_levels.py'), '--game']),
        ('shaderdb-native:game', 'Rust shader databases on every database of the game copy, the Xbox and BF1943 levels and work; the WW2 merges', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_native.py'), '--game']),
        ('shaderdb-queries:game', 'Rust shader database queries on every database of the game copy, the Xbox and BF1943 levels and work', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_queries.py'), '--game']),
        ('xenos-native:game', 'Rust shader translator on every program of the BF1943 and BFBC2 Xbox shader databases (trees, HLSL, errors)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xenos_native.py'), '--game']),
        ('culltree-native:game', "cull trees (Rust) on every tree and level partition of the game copy and work against the retired Python reader's recorded answers; listings; the recorded add run", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree_native.py'), '--game']),
        ('granny-native:game', 'Rust Granny reader and conversion on every Granny resource of your Xbox files, the game copy and BF1943', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_native.py'), '--game']),
        ('culltree-rows-cache:game', 'cull tree add and verify on Onslaught MP_005 against every work\\levels folder: reference, no cache, cold, warm (E3m)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree_e3m.py'), '--game']),
        ('navmesh-cells-routes-native:game', "Rust E3o on the game copy: every path-data sector, navdata.load of DICE's prepared levels, 400 routes, verify-havok's Onslaught files, texqa on the WW2 vehicles", 'game',
         [PY, os.path.join(SUITE, 'tests', 'test_e3o_native.py'), '--game']),
        ('pathdata-native:game', 'Rust F0 path data on every pathdata.hpd and kept original of the game copy and work: parsed, written and every runtime mesh written both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_pathdata_native.py'), '--game']),
        ('bc2loc-native:game', 'Rust F0 localization on every language file of the overlay bundles and the pristine copy, BIGF on every menu / HUD movie, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_bc2loc_native.py'), '--game']),
        ('console-readers-native:game', 'Rust F0 console containers on your Xbox disc image and Update packages, 1,500 damaged packages, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-readers', 'tests', 'test_console_readers_native.py'), '--game']),
        ('wave-native:game', 'Rust F0 Wave resources and streams on every BF1943, BFBC2 Xbox and game-copy Wave, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_wave_native.py'), '--game']),
        ('meshdata-native:game', 'Rust F0 mesh headers and mesh sets on every MeshData and mesh set of BF1943, the Xbox bundles and the game copy, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_meshdata_native.py'), '--game']),
        ('texture-helpers-native:game', 'Rust F0 texture levels and decoding on every PC texture of the game copy, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture_helpers_native.py'), '--game']),
        ('havok-classes-native:game', 'Rust F0 havok.classes on damaged copies of BFBC2Game.exe, against the recorded answers', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_havok_classes_native.py'), '--game']),
        ('console-convert-native:game', 'Rust F2a console converter on every BF1943 / Xbox terrain, soot, physics, shader database and mesh resource and the game copy (readers, decode, relayout, texture.check), both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_console_f2a_native.py'), '--game']),
        ('model-tools-native:game', 'Rust F2a model tools on every game-copy DXT texture encoded again and every skinned LOD, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_model_f2a_native.py'), '--game']),
        ('granny-write-native:game', 'Rust F2a Granny writing on every game-copy animation (shift, orientations, add_tracks), both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_f2a_native.py'), '--game']),
        ('aim-pathmesh-audio-native:game', 'Rust F3 formats on every aim animation, path-data sector, Xbox / BF1943 / PC Wave stream, sample run, packfile and Xbox shader program, both ways and against the recorded answers', 'game',
         [PY, os.path.join(SUITE, 'tests', 'test_f3_native.py'), '--game']),
        ('culltree-strings-disasm-native:game', 'Rust F4 on every cull tree, Xbox shader program, packfile, collision asset, texture, mesh and BF1943 / Xbox / PC Wave, both ways and against the recorded answers', 'game',
         [PY, os.path.join(SUITE, 'tests', 'test_f4_native.py'), '--game']),
        ('shaderdb-open-native:game', "Rust SD1 shader database opening on every database (game copy, Xbox levels, BF1943, OPFOR's work outputs; cut and damaged copies), every bundle's choice and every level's load, both ways and against the recorded answers", 'game',
         [PY, os.path.join(SUITE, 'tests', 'test_sd1_native.py'), '--game']),
        ('scene-native:game', "level loader (Rust scene) against the retired Python loader's recorded Levels, entity by entity (SP_01, MP_005, MP_009GR, ONS_MP_004)", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_scene_native.py')]),
        ('collision-backends:game', "NavKit's collision readers (detailed, ladders, offline sampler, boat mesh) on the Rust core against the retired Python path's recorded results (SP_009, SP_004_B)", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_collision_backends.py')]),
        ('locators:game', 'path locators: the same answers as a reference checkout (main)', 'game', [PY, os.path.join(SUITE, 'tests', 'test_locators.py')]),
        ('crashreader:game', 'crash reader code view (BFBC2Game.exe)', 'game', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_crashreader.py'), '--game']),
        ('havok:game', 'console resource converter: physics (havok)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_havok.py')]),
        ('texture:game', 'console resource converter: textures', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture.py')]),
        ('texture-native:game', 'Rust texture converter: every BF1943 and Onslaught console texture as recorded', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture_native.py'), '--game']),
        ('mesh-decode:game', 'console mesh decoding (Xbox vs PC, BF1943)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_mesh_decode.py')]),
        ('console-terrain:game', 'console terrain conversion (Xbox vs PC, BF1943 Wake)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_terrain.py')]),
        ('soot:game', 'console soot meshes (Xbox vs PC, BF1943)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_soot.py')]),
        ('audio:game', 'console sounds: EA-XMA -> EA-XAS (Xbox vs PC, BF1943)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_audio.py')]),
        ('audio-native:game-every-10th', 'Rust EA-XAS audio encoder: every 10th BF1943 Wave and in-memory BFBC2 Xbox Wave converted as recorded, byte for byte', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_audio_native.py'), '--game', '--every', '10']),
        ('meshview:game', 'mesh viewer on the game\'s models', 'game', [PY, os.path.join(SUITE, 'bfbc2-mesh-viewer', 'tests', 'test_meshview.py')]),
        ('models:game', 'model tools: skinned meshes, mesh sets, shapes', 'game', [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_models.py')]),
        ('texwrite:game', 'model tools: PC textures from pictures (DXT1)', 'game', [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_texwrite.py')]),
        ('decimate:game', 'model tools: mesh decimation (seams closed, bones apart)', 'game', [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_decimate.py')]),
        ('granny:game', 'Granny skeletons and animations (read, shift)', 'game', [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny.py')]),
        ('granny-port:game', 'Granny Xbox 360 -> PC on your Xbox files and BF1943', 'game', [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_port.py'), '--game']),
        ('granny-curves:game', 'Rust granny.curve_keys / sample against the retired Python on every animation curve of the game copy', 'game', [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_curves.py'), '--game']),
        ('aimanim:game', "aim animations: every AimAnimation of the game copy read and written back, BF1943's converted onto BFBC2's soldier", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_aimanim.py'), '--game']),
        ('trees:game', "tree mesh sets: every BFBC2 PC TreeMeshSet written back byte for byte, BF1943's read, its palm's strips as DICE's triangle list", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_trees.py'), '--game']),
        ('bf1943import:game', 'BF1943 import pipeline on your BF1943 copy', 'game', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_bf1943import.py'), '--game']),
        ('meshport:game', 'BF1943 model port (meshport)', 'game', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_meshport.py')]),
        ('dbxport:game', 'DBX writer and BF1943 partition port (dbxport)', 'game', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport.py')]),
        ('dbxport-fair:game', 'dbxport fair ports: every partition both games ship, against DICE', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_fair.py')]),
        ('parityreports:game', "bfbc2 dbxparity / portparity's reports on your BF1943 and game copy: the retired Python's, byte for byte", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_parity_reports.py')]),
        ('dbxport-ranking:game', "dbxport ranking: the four WW2 level ports against a third of BFBC2's 157 levels, every choice and the bytes", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_ranking.py'), '--game']),
        ('dbxport-opfor:game', 'dbxport in Rust against its Python reference: the Project OPFOR BF1943 vehicle, weapon and object builds replayed both ways (outputs to temp)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_opfor.py')]),
        ('dbxport-native:game', 'dbxport in Rust against its Python reference: every fair, counterpart and effect port both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_native.py'), '--game']),
        ('ports-native:game', 'BF1943 ports E3g: every BF1943 mesh set, physics file and effect, the Onslaught console meshes and physics, as recorded and both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_ports_native.py'), '--game']),
        ('havokport-native:game', 'F2b: mopp and havokport in Rust on every BF1943 physics file, every BFBC2 physics resource and their MOPP codes, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_havokport_native.py'), '--game']),
        ('dbxcatalogue-native:game', 'F2b: dbxcatalogue.build in Rust against its Python on the game copy (84,739 partitions)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxcatalogue_native.py'), '--game']),
        ('emitteratlas-native:game', 'F2b: emitteratlas in Rust against its Python on the inputs of the four OPFOR emitter_atlas runs', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_emitteratlas_native.py'), '--game']),
        ('pe-native:game', 'F2b: menucommon.PEImage in Rust against the recorded answers of its retired Python on every executable of the game copy', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_pe_native.py'), '--game']),
        ('xmlinst-native:game', 'F2b: xmlinst in Rust against the recorded answers of its retired patterns on every converted level XML of OPFOR work levels', 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_xmlinst_native.py'), '--game']),
        ('havokport:game', 'BF1943 physics port, Havok 4.6.1 -> 5.5.0 (havokport)', 'game', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_havokport.py')]),
        ('bc2loc-write:game', 'localization files written (bc2loc set)', 'game', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_bc2loc_write.py')]),
        ('console-readers:game', 'console readers on your Xbox files', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-readers', 'tests', 'test_console_readers.py'), '--game']),
        ('menu-tools:game', 'menu tools on the game copy and OPFOR\'s patches', 'game', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_menu_tools.py'), '--game']),
        ('apt-decode-graft-native:game', 'APT decoder (Rust) and aptgraft writer against their references on every movie of the game copy, the pristine menus and the Xbox HUD copies; aptrefs / aptcheck / bc2loc both ways (E3n)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_apt_e3n.py'), '--game']),
        ('apt-model-check-refs-native:game', 'APT model, aptcheck and aptrefs in Rust against the Python on the 133 movies of E3n: every write, roundtrip --pack, grafts, aliases, crosshair-add (E4c)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_apt_e4c.py'), '--game']),
        ('exeref:game', 'exeref on BFBC2Game.exe', 'game', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_exeref.py'), '--game']),
        ('crash-native:game', 'crash reader and exeref: the Rust core against capstone on BFBC2Game.exe, both command lines byte for byte, opfor.log sessions', 'game', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_crash_native.py'), '--game']),
        ('fbrbtool-roundtrip:game-12', 'FbrbTool round trip, 12 game bundles', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb.py'), '--game', '12']),
        ('water:game', 'water, every level', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_water.py')]),
        ('level:game', 'level loader snapshot', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_level.py')]),
        ('terrain-samples:game', 'terrain against in-game samples', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_terrain.py')]),
        ('spec-probes:game', "the specification probes' counts on DICE's files: terrains, undergrowth, emitter chains, grime, menus, levels and cull trees, the stock registries, weapons, vehicles, materials, destruction, lighting (SPG2, SPV1); terrain shapes, biomes and the generator prototype", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_spec_probes.py')]),
        ('hpdtool-roundtrip:game', 'path data round trip (hpdtool)', 'game', lambda root: [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdtool.py'),
                                                               'roundtrip', *pathdata_files(root)]),
        ('hpdtool-meshtrip:game', 'navigation mesh round trip (kymesh)', 'game', lambda root: [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdtool.py'),
                                                                    'meshtrip', *pathdata_files(root)]),
        ('navdata:game', 'navigation data and the path test', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_navdata.py')]),
        ('hpdcheck:game', "path data structure, DICE's files (hpdcheck)", 'game', lambda root: [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdcheck.py'),
                                                                             *dice_pathdata_files(root)]),
        ('navwater-mesh:game', 'NavKit boat mesh and routes on ONS_MP_004', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navwater_mesh.py'), '--game']),
        ('vehicle-level:game', 'NavKit vehicle path data on ONS_MP_002', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_vehicle_level.py')]),
        ('links:game', 'NavKit links and route edges', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_links.py')]),
        ('traversal:game', 'traversals, the apply report, installing', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_traversal.py')]),
        ('apply:game', 'navkit apply with walkability edits', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_apply.py')]),
        ('hub:game', 'the suite window (hub) and the level picker', 'game', [PY, os.path.join(SUITE, 'tests', 'test_hub.py')]),
        ('harness:game', 'test harness on sp_009 and sp_004_b', 'game', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'tests', 'test_harness.py'), '--game']),
        ('stockmp:game', 'stock-MP AI layer, Atacama Conquest built', 'game', [PY, os.path.join(SUITE, 'bfbc2-stockmp-ai', 'tests', 'test_stockmp.py'), '--game']),
        ('native-levels:game', 'stock-MP AI: the Rust core against the Python on every MP level partition', 'game', [PY, os.path.join(SUITE, 'bfbc2-stockmp-ai', 'tests', 'test_native_levels.py')]),
        ('navbuild-native:game', 'Rust NavKit builds: sp_009 and sp_004_b path data as recorded', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navbuild_native.py'), '--game', '--levels', 'sp_009,sp_004_b']),
        ('navtools-native:game', "E4e: NavKit's tools on the game copy as recorded from their Python (ONS_MP_002 vehicle data, ONS_MP_004 boat mesh and domains, sp_009 / sp_004_b parity measures, sp_009 offline samples)", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navtools_native.py'), '--game']),
        ('navregress:small', 'NavKit regression, 2 small levels', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'navregress.py'),
                                                       '--levels', 'sp_004_b,sp_009']),
        ('navregress:all', 'NavKit regression, all levels with DICE path data', 'full', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'navregress.py')]),
        ('dbx-dbxedit-corpus:full', 'DBX reader against DbxEdit, every unpacked .dbx', 'full', lambda root: [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx.py'),
                                                                   os.path.join(root, 'work', 'levels'), os.path.join(root, 'work', 'navkit', 'common')]),
        ('fbrbtool-roundtrip:game-all', 'FbrbTool round trip, every game bundle', 'full', [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb.py'), '--game', '0']),
        ('audio-native:game-all', 'Rust EA-XAS audio encoder: every BF1943 Wave and in-memory BFBC2 Xbox Wave converted as recorded, byte for byte', 'full',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_audio_native.py'), '--game']),
    ]
    return t


def need(cpu: int | str = 1, mem: float = 0.5, setup: bool = False, alone: bool = False, **res: str) -> dict:
    """A test's needs (see the docstring): need(cpu='all', mem=4, regress='r')."""
    return {'cpu': cpu, 'mem': mem, 'setup': setup, 'alone': alone, 'res': res}


# What each test needs to run beside others, by its name in tests(). Shared things (res):
#   regress     OPFOR work\navkit\regress\<lv>.hpd and work\navkit\parity\regression_last.json: navregress rewrites
#               them in place; the apply, links and harness tests read them
#   work_hpd    every pathdata.hpd under OPFOR's work\ (hpdtool walks them all): test_vehicle_level rewrites one
#   navstudio   OPFOR work\navstudio\cache (navdata's cache files, replaced while another process may read them)
#   sp_004_b    OPFOR work\navkit\sp_004_b\detailed.json (NavKit apply writes it)
#   shaderview  the suite's build\ folder: shaderview.exe built on first use, renderqa's index and truth pickles
#   display     a window on the desktop (Tk, OpenGL)
#   cargo       the checkout's cargo build folder (native.target_dir, round D1: rust-targets\<checkout>; cargo's
#               lock) and bin\ (build_native.py copies the binaries there)
#   rusttests   the workspace's test binaries and their record (tests\test_rust.py build writes them, its runs read
#               them; round TS1)
# Tests that share a thing also keep their listed order between them, so each finds it as a run one at a time did.
# Not resources, because the code takes care of them (2026-11-17): a level's terrain unpack and the meshview / dbx
# catalogue caches in work\tools_out are built under a lock between processes (atomicio.locked); the collision cache
# in %LOCALAPPDATA% is written atomically under a per-process name. Every other test writes only its own temp folder.
# mem is an estimate in GB from what the test loads (pools x bundles, whole BF1943 bundles ~0.6 GB); a run records
# each test's measured peak beside its duration. Round QW1 (audit AU2 P3): the declarations below were raised to the
# peaks recorded on the development PC (25 were far below them: the cargo checks 2 GB declared / 28.7 measured, the
# NavKit regression 3 / 15.0, O4 1 / 6.1, ...), and the scheduler takes the larger of a test's declaration and its
# last recorded peak on this PC (peak_gb): a declaration that falls behind its test is corrected by the next run. A
# test needing more than --mem runs with nothing else beside it (Scheduler.mem_of).
NEEDS = {
    # quick
    'rust-module-build': need(mem=2, setup=True),
    # round TS1: the cargo entry (cpu='all', 12 GB, 3,154 s, nothing beside it) split; CARGO_BUILD_JOBS=4 on this PC.
    # The build writes the test binaries the runs read (rusttests): the runs wait for it, then overlap clippy.
    'rust-build': need(cpu=4, mem=6, cargo='w', rusttests='w'),
    'rust-quick': need(cpu=4, mem=4, rusttests='r'),
    'rust-clippy': need(cpu=4, mem=3, cargo='w'),
    'rust-fmt': need(mem=0.3),
    'rust-green-crates': need(),
    'dbx-native': need(cpu=2, mem=1),
    'fbrb-native': need(cpu=3, mem=1),
    'fbrb-cache': need(cpu=4, mem=1),
    'geometry-native': need(),
    'shaderdb-native': need(cpu=4, mem=1),
    'shaderdb-queries': need(mem=0.6),
    'shaderdb-merge': need(mem=0.5),
    'shaderdb-levels': need(cpu=2, mem=0.5),
    'xenos-native': need(mem=0.3),
    'culltree-native': need(cpu=2),
    'granny-native': need(mem=1.2),
    'texture-native': need(cpu=2),
    'audio-native': need(cpu=2, mem=1),
    'navbuild-native': need(),
    'appicon': need(),
    'cli-caches-stockmp-graft-meshview-cull': need(cpu=4, mem=3),
    'cli-readers-shaders-import-stockmp': need(cpu=4, mem=3),
    'shaderdb-open-native': need(),
    'navtools-native': need(cpu=2),
    'native-required': need(),
    'review-regressions': need(),
    'asset-accuracy': need(cpu=2, shaderview='w'),
    'repo': need(cpu=2),
    'envvars': need(),
    'spec-trace': need(),
    'native-census': need(),
    'runner-output': need(),
    'runner-parallel': need(cpu=2),
    'runner-args': need(),
    'lint': need(cpu=2),
    'types': need(mem=1, cargo='r'),
    'core-edges': need(),
    'paths': need(),
    'converter-edges': need(),
    'converter-units': need(),
    'fbrbtool-roundtrip': need(),
    'dbx-write-native': need(cpu=2, mem=1),
    'dbxedit': need(cpu=2),
    'fbrbtool': need(cpu=2, mem=0.5),
    'cli-fbrbtool-dbxedit': need(cpu=4, mem=0.5),
    'fbrb-pack-native': need(cpu=4, mem=1),
    'fbrb-unpack-native': need(cpu=2, mem=1),
    'cli-menu-crash-navkit-convert': need(cpu=4, mem=1),
    'cli-menu-crash-navkit-convert:live': need(cpu=4, mem=1),
    'cli-navkit-prepare-harness-texqa': need(cpu=4, mem=3),
    'cli-navkit-prepare-harness-texqa:live': need(cpu=4, mem=6.5),
    'cli-prepare-level:game': need(cpu=4, mem=1),
    'errors': need(),
    'caches': need(),
    'logs-caches': need(),
    'cli-errors': need(),
    'review-fixes': need(),
    'review-fixes2': need(),
    'deep-scan-fixes': need(),
    'navtune': need(),
    'water-rule': need(),
    'navedits': need(),
    'regress-progress': need(),
    'navboundary': need(),
    'reference-clip': need(),
    'characters': need(),
    'headroom': need(),
    'lips': need(),
    'dynamic-props': need(),
    'roof-prune': need(),
    'objective-floor': need(),
    'detail-features': need(),
    'havok-index': need(),
    'navwater-mesh': need(),
    'navdomains': need(),
    'vehicle': need(),
    'navislands': need(),
    'navsample-offline': need(),
    'tool-edges': need(),
    'overlay-offline': need(),
    'navkit-offline': need(),
    'meshview-index': need(),
    'crashreader': need(),
    'console-readers': need(),
    'menu-tools': need(),
    'aptgraft': need(),
    'apt-decode-graft-native': need(),
    'apt-model-check-refs-native': need(),
    'culltree': need(),
    'culltree-rows-cache': need(),
    'navmesh-cells-routes-native': need(cpu=3, mem=2.2),
    'pathdata-native': need(mem=0.5),
    'bc2loc-native': need(),
    'console-readers-native': need(mem=2.8),
    'wave-native': need(mem=1),
    'meshdata-native': need(mem=0.5),
    'console-convert-native': need(mem=1),
    'model-tools-native': need(mem=0.5),
    'granny-write-native': need(mem=0.5),
    'argfmt': need(),
    'abbrev': need(),
    'aim-pathmesh-audio-native': need(mem=0.5),
    'culltree-strings-disasm-native': need(mem=1),
    'texture-helpers-native': need(),
    'havok-classes-native': need(),
    'exeref': need(),
    'harness': need(mem=1),
    'crash-native': need(cpu=2),
    'stockmp': need(),
    'validate': need(cpu=2),
    'shaderdb': need(),
    'renderqa': need(cpu=2, mem=2, shaderview='w'),
    'xdiff': need(cpu=2, shaderview='w'),
    'pcbuild-batch': need(cpu=2),
    'shaderview-serve': need(cpu=2, shaderview='w'),
    'granny-port': need(),
    'granny-curves': need(),
    'aimanim': need(),
    'trees': need(),
    'bf1943import': need(),
    'dbxport-rules': need(),
    'dbxport-ranking': need(),
    'dbxport-tables': need(),
    'dbxport-native': need(),
    'ports-native': need(),
    'havokport-native': need(),
    'dbxcatalogue-native': need(),
    'emitteratlas-native': need(),
    'pe-native': need(),
    'xmlinst-native': need(),
    'navtune-native': need(),
    'havokport-native:game': need(cpu=2, mem=2.8),
    'dbxcatalogue-native:game': need(cpu=4, mem=2.5),
    'emitteratlas-native:game': need(cpu=2, mem=2),
    'pe-native:game': need(),
    'xmlinst-native:game': need(mem=1),
    'dbxport-fair': need(mem=2),
    'emitteratlas': need(),
    'texqa': need(),
    # game
    'shaderdb:xbox': need(mem=1),
    'renderqa:game': need(cpu=4, mem=2, shaderview='w'),
    'xdiff:game': need(cpu=4, mem=1.5, shaderview='w'),
    'pcbuild-batch:game': need(cpu='all', mem=1.6),
    'xdiff:game-all': need(cpu=4, mem=1.5, shaderview='w'),
    'dbx-native:game': need(cpu=6, mem=3),
    'rust-dbx-corpus': need(cpu='all', mem=3, cargo='w'),
    # whole levels two at a time (the binaries' gate()); builds the tests first only when the build entry's are stale
    'rust-game': need(cpu=4, mem=8, rusttests='r'),
    'fbrb-native:game': need(cpu=3, mem=3),    # 3 processes, a bundle read in groups of 256 MB
    'fbrb-cache:game': need(cpu=2, mem=1.5),
    'dbx-write-native:game': need(cpu=6, mem=3),
    'dbxedit:game': need(cpu=4, mem=1),
    'fbrbtool:game': need(cpu=2, mem=1),
    'cli-fbrbtool-dbxedit:live': need(cpu=4, mem=1),
    'fbrb-pack-native:game': need(cpu=4, mem=4),
    'fbrb-unpack-native:game': need(cpu=2, mem=2),
    'geometry-native:game': need(cpu=3, mem=2),
    'shaderdb-native:game': need(cpu=8, mem=3),
    'shaderdb-merge:game': need(mem=1),
    'shaderdb-queries:game': need(cpu=3, mem=1.5),   # 2 workers
    'xenos-native:game': need(cpu=3, mem=1),          # 2 workers of ~360 MB
    'culltree-native:game': need(cpu=3, mem=3),
    'granny-native:game': need(mem=1),
    'culltree-rows-cache:game': need(cpu=2, mem=3),
    'apt-decode-graft-native:game': need(cpu=2, mem=1.5),
    'apt-model-check-refs-native:game': need(cpu=2, mem=1.5),
    'navmesh-cells-routes-native:game': need(cpu='all', mem=5.5),
    'pathdata-native:game': need(mem=1, work_hpd='r'),
    'bc2loc-native:game': need(),
    'console-readers-native:game': need(mem=2.2),
    'wave-native:game': need(mem=1.5),
    'meshdata-native:game': need(mem=1.5),
    'console-convert-native:game': need(mem=3),
    'model-tools-native:game': need(mem=5.7),
    'granny-write-native:game': need(mem=1),
    'aim-pathmesh-audio-native:game': need(mem=2, alone=True),
    'culltree-strings-disasm-native:game': need(mem=2, alone=True),
    'texture-helpers-native:game': need(mem=5.4),
    'havok-classes-native:game': need(),
    'scene-native:game': need(cpu=8, mem=3),
    'collision-backends:game': need(cpu=4, mem=2),
    'locators:game': need(),
    'crashreader:game': need(mem=1.8),
    'havok:game': need(mem=1),
    'texture:game': need(mem=2),
    'texture-native:game': need(cpu='all', mem=2),
    'mesh-decode:game': need(mem=1.5),
    'console-terrain:game': need(mem=1.5),
    'soot:game': need(mem=1.5),
    'audio:game': need(cpu=2, mem=3),
    'audio-native:game-every-10th': need(cpu=4, mem=2),
    'meshview:game': need(mem=1.5),
    'models:game': need(mem=1),
    'texwrite:game': need(mem=1),
    'decimate:game': need(),
    'granny:game': need(mem=1),
    'granny-port:game': need(mem=1.5),
    'granny-curves:game': need(mem=1),
    'aimanim:game': need(),
    'trees:game': need(mem=1),
    'bf1943import:game': need(mem=1),      # E3r: the lazy Source (0.86 GB peak; was ~2)
    'meshport:game': need(mem=1.5),
    'dbxport:game': need(mem=2),
    'dbxport-fair:game': need(cpu=4, mem=1.5),  # E3r: DICE's read on all cores
    'dbxport-ranking:game': need(mem=2.5),
    'dbxport-opfor:game': need(cpu=4, mem=2),
    'dbxport-native:game': need(cpu=4, mem=1.5),
    'ports-native:game': need(cpu=4, mem=1.5),
    'havokport:game': need(mem=1.5),
    'bc2loc-write:game': need(),
    'console-readers:game': need(),
    'menu-tools:game': need(),
    'exeref:game': need(),
    'crash-native:game': need(mem=1.5),
    'fbrbtool-roundtrip:game-12': need(mem=1),
    'water:game': need(),
    'level:game': need(cpu=4, mem=1.5),
    'terrain-samples:game': need(mem=1),
    'spec-probes:game': need(mem=1),
    'hpdtool-roundtrip:game': need(work_hpd='r'),
    'hpdtool-meshtrip:game': need(work_hpd='r'),
    'navdata:game': need(),
    'hpdcheck:game': need(),
    'navwater-mesh:game': need(mem=1),
    'vehicle-level:game': need(mem=1.5, work_hpd='w'),
    'links:game': need(mem=1, regress='r'),
    'traversal:game': need(),
    'apply:game': need(mem=3, regress='r', sp_004_b='w'),
    'hub:game': need(display='w'),
    'harness:game': need(mem=2.6, regress='r'),
    'stockmp:game': need(cpu=2, mem=1.5),
    'native-levels:game': need(cpu=2, mem=1),
    'navbuild-native:game': need(mem=3),
    # writes only temporary folders, and navwater's navdata cache as 'NavKit boat mesh and routes on ONS_MP_004' does
    'navtools-native:game': need(cpu='all', mem=4),
    'navregress:small': need(mem=3, regress='w'),
    # full
    'navregress:all': need(mem=15, regress='w'),     # 15.0 GB measured: runs alone
    'dbx-dbxedit-corpus:full': need(mem=1),
    'fbrbtool-roundtrip:game-all': need(mem=3.2),
    'audio-native:game-all': need(cpu=4, mem=2.5),
}


# Round P1: `quick --no-game` runs the quick entries that need nothing but the checkout, as CI does on a GitHub runner
# (.github/workflows/checks.yml): no game copy, no Project OPFOR folder, no BF1943 or Xbox copies, no graphics card.
# Every quick entry is such already (2026-11-19): they test built files and recorded answers, skip their own
# game-data parts with a note when the game copy is not found ("8 game bundles" becomes 0, "skipped: the game runs"),
# and draw on WARP or skip their GPU parts without a graphics adapter (shaderview, the editor's offscreen tests).
# Checked with the game copy out of reach: a checkout outside the projects folder, so no project-opfor-bfbc2 is found
# beside or above it (OPFOR_ROOT alone does not hide it: the locators walk up from the suite), and OPFOR_ROOT /
# BF1943_XENON / BF1943_SUITE_FBRB pointing at a folder that does not exist: 121 of 121 passed. An entry that cannot
# run so goes here (name in tests() -> why) and is skipped with the reason.
NOT_GAME_FREE: dict[str, str] = {
}

# The entries CI runs as steps of its own (checks.yml: cargo fmt, clippy, the workspace's tests, ruff, mypy):
# `--ci-steps` skips them, so they are not built and run twice.
CI_STEPS: dict[str, str] = {
    'rust-fmt': 'cargo fmt is a CI step of its own',
    'rust-clippy': 'clippy is a CI step of its own',
    'rust-build': "the workspace's tests are a CI step of their own",
    'rust-quick': "the workspace's tests are a CI step of their own",
    'lint': 'ruff is a CI step of its own',
    'types': 'mypy is a CI step of its own',
}


def skipped_entries(chosen, no_game, ci_steps):
    """(entries to run, [(name, why)] skipped) for --no-game / --ci-steps (NOT_GAME_FREE / CI_STEPS by id)."""
    run, skipped = [], []
    for entry in chosen:
        why = (NOT_GAME_FREE.get(entry[0]) if no_game else None) or (CI_STEPS.get(entry[0]) if ci_steps else None)
        if why:
            skipped.append((entry[1], why))
        else:
            run.append(entry)
    return run, skipped


def unknown_skip_names():
    """Ids in NOT_GAME_FREE / CI_STEPS that are no entry of tests() (a removed test): they would skip nothing."""
    ids = {sid for sid, _, _, _ in tests()}
    return sorted(n for n in (*NOT_GAME_FREE, *CI_STEPS) if n not in ids)


def safe_console():
    """Any child's output prints on any console: characters the console's code page lacks (maturin's emoji on a
    cp1252 console) are written as escapes instead of crashing the runner and hiding the failure (2026-11-16)."""
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, 'reconfigure', None)     # a TextIOWrapper's; a replaced stream may lack it
        if reconfigure is not None:
            with contextlib.suppress(ValueError):
                reconfigure(errors='backslashreplace')


# --- running a test --------------------------------------------------------------------------------------------

def _extended_limits():
    """JOBOBJECT_EXTENDED_LIMIT_INFORMATION (class 9 of Query / SetInformationJobObject)."""
    import ctypes
    from ctypes import wintypes

    class Basic(ctypes.Structure):
        _fields_ = [('a', ctypes.c_int64), ('b', ctypes.c_int64), ('flags', wintypes.DWORD),
                    ('wmin', ctypes.c_size_t), ('wmax', ctypes.c_size_t), ('procs', wintypes.DWORD),
                    ('affinity', ctypes.c_size_t), ('prio', wintypes.DWORD), ('sched', wintypes.DWORD)]

    class Extended(ctypes.Structure):
        _fields_ = [('basic', Basic), ('io', ctypes.c_uint64 * 6), ('proc_limit', ctypes.c_size_t),
                    ('job_limit', ctypes.c_size_t), ('peak_proc', ctypes.c_size_t), ('peak_job', ctypes.c_size_t)]

    return Extended()


KILL_ON_JOB_CLOSE = 0x2000          # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE


class _ProcessGroup:
    """A Windows job object around a test's process: its peak memory, all its children included (process pools,
    the C# tools, cargo), and a way to end them all. The job is made KILL_ON_JOB_CLOSE (audit AU4 DF4): when the
    runner ends in any way - Ctrl+C, a crash, its console closed, killed - Windows closes the handle and ends every
    process of every test with it, so none is left holding a build folder or a game file. Does nothing elsewhere or
    when the job cannot be made."""

    def __init__(self):
        self.handle = None
        if sys.platform != 'win32':
            return
        import ctypes
        self.k32 = ctypes.WinDLL('kernel32', use_last_error=True)
        self.k32.CreateJobObjectW.restype = ctypes.c_void_p
        self.k32.AssignProcessToJobObject.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        self.k32.QueryInformationJobObject.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_ulong,
                                                       ctypes.c_void_p]
        self.k32.SetInformationJobObject.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_ulong]
        self.k32.TerminateJobObject.argtypes = [ctypes.c_void_p, ctypes.c_uint]
        self.k32.CloseHandle.argtypes = [ctypes.c_void_p]
        self.handle = self.k32.CreateJobObjectW(None, None)
        if self.handle:
            info = _extended_limits()
            info.basic.flags = KILL_ON_JOB_CLOSE
            self.k32.SetInformationJobObject(self.handle, 9, ctypes.byref(info), ctypes.sizeof(info))

    def add(self, proc):
        if self.handle and not self.k32.AssignProcessToJobObject(self.handle, int(proc._handle)):
            self.close()                    # a job the runner's own job forbids: no measure, the test runs as ever

    def kills_on_close(self):
        """The job's KILL_ON_JOB_CLOSE flag is set (test_runner_parallel checks it)."""
        if not self.handle:
            return False
        import ctypes
        info = _extended_limits()
        if not self.k32.QueryInformationJobObject(self.handle, 9, ctypes.byref(info), ctypes.sizeof(info), None):
            return False
        return bool(info.basic.flags & KILL_ON_JOB_CLOSE)

    def peak_mb(self):
        if not self.handle:
            return None
        import ctypes
        info = _extended_limits()
        if not self.k32.QueryInformationJobObject(self.handle, 9, ctypes.byref(info), ctypes.sizeof(info), None):
            return None
        return round(info.peak_job / 2**20)

    def kill(self):
        if self.handle:
            self.k32.TerminateJobObject(self.handle, 1)

    def close(self):
        if self.handle:
            self.k32.CloseHandle(self.handle)
            self.handle = None


RUNNING: dict = {}                   # test name -> its _ProcessGroup, so that Ctrl+C ends every test's processes
SKIPPED = 0                          # entries --no-game / --ci-steps left out (said at the start and in the total)
PART_SKIPS: dict = {}                # test name -> its output's "skipped ..." lines (parts it could not run here)


def run_one(name, args, timeout=None):
    """Runs one test: (ok, its whole output, its summary line, seconds, peak memory in MB or None). A test still
    running after `timeout` seconds is ended with all its processes and fails, saying so (audit AU4 DF4: a hung
    test held the tier for ever)."""
    t0 = time.time()
    group = _ProcessGroup()
    timed_out = False
    try:
        p = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8',
                             errors='replace', cwd=SUITE)
        group.add(p)
        RUNNING[name] = group
        try:
            stdout, stderr = p.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            group.kill()
            p.kill()
            stdout, stderr = p.communicate()
        except BaseException:                       # Ctrl+C: the test ends with the runner (as subprocess.run did)
            group.kill()
            p.kill()
            raise
        peak = group.peak_mb()
    finally:
        RUNNING.pop(name, None)
        group.close()
    if timed_out:
        stderr += f'\nrun_tests: timed out after {timeout:.0f} s: the test and its processes were ended (--timeout)'
    out = (stdout + stderr).strip()
    PART_SKIPS[name] = sum(1 for line in stdout.splitlines() if line.strip().lower().startswith(SKIP_LINE))
    main_out = stdout.strip() or out            # the summary line is the test's own last line, not a warning it printed
    last = main_out.splitlines()[-1] if main_out else ''
    if timed_out:
        last = f'timed out after {timeout:.0f} s'
    return p.returncode == 0 and not timed_out, out, last, time.time() - t0, peak


# A test's time limit (round TS1, audit AU4 DF4): --timeout S for every test, else TIMEOUT_FACTOR times its kept
# duration on this PC, at least its tier's floor (a test no run has timed gets the floor).
TIMEOUT_FLOOR = {'quick': 1800.0, 'game': 3 * 3600.0, 'full': 8 * 3600.0}
TIMEOUT_FACTOR = 4.0


def time_limit(tier, known_secs, given=None):
    """The seconds a test of `tier` may run: `given` (--timeout), else max(floor, TIMEOUT_FACTOR x kept duration)."""
    if given is not None:
        return given if given > 0 else None
    return max(TIMEOUT_FLOOR[tier], TIMEOUT_FACTOR * (known_secs or 0.0))


SKIP_LINE = ('skipped', 'skip:')          # a test's summary line that says it ran nothing (no game copy, no Rust ...)


def skipped(ok, last):
    """A passing test whose summary line (its last line) says it was skipped: data, a program or a graphics adapter
    it needs is not on this PC. Counted as skipped, not passed (round QW1, audit AU2 Q9)."""
    return ok and last.strip().lower().startswith(SKIP_LINE)


def result_line(ok, name, secs, last):
    tag = 'skip' if skipped(ok, last) else 'ok  ' if ok else 'FAIL'
    return f'{tag} {name:42s} {secs:6.1f} s   {last[:90]}'


# --- durations ---------------------------------------------------------------------------------------------------

TIMES_FILE = os.path.join(os.environ.get('LOCALAPPDATA') or os.path.expanduser('~'), 'bfbc2-tool-suite', 'test_times.json')
SEED_FILE = os.path.join(SUITE, 'tests', 'test_durations.json')
GUESS = {'quick': 2.0, 'game': 30.0, 'full': 600.0}     # seconds, for a test no run has timed yet


def durations_from_log(path, names):
    """name -> seconds from a run_tests.py log: each result line is 'ok   <name padded> <seconds> s   <summary>'."""
    import re
    out = {}
    by_length = sorted(names, key=len, reverse=True)       # a name that starts another is tried after the longer one
    with open(path, encoding='utf-8', errors='replace') as f:
        for line in f:
            if not line.startswith(('ok   ', 'FAIL ')):
                continue
            rest = line[5:]
            for n in by_length:
                m = re.match(r'\s+(\d+(?:\.\d+)?) s(?:\s|$)', rest[len(n):]) if rest.startswith(n) else None
                if m:
                    out[n] = float(m.group(1))
                    break
    return out


def load_json(path):
    try:
        with open(path, encoding='utf-8') as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def ids_by_name():
    """{display name: stable id} of every entry of tests()."""
    return {name: sid for sid, name, _t, _a in tests()}


def by_id(kept, ids=None):
    """A durations file's entries keyed by stable id: a key that is a display name (a file kept before round PR1a
    keyed its tests so) is read as that test's id; an id's own entry wins over one read from its old name."""
    ids = ids_by_name() if ids is None else ids
    out = {ids[k]: v for k, v in kept.items() if k in ids}
    out.update({k: v for k, v in kept.items() if k not in ids})
    return out


def known_times(names, times_file=None):
    """(name -> (seconds, peak MB or None), where they came from): --times F, else this PC's kept durations, else
    the suite's seed (tests\\test_durations.json). The JSON files are keyed by the tests' stable ids (by_id: older
    files' display names are read too); a run log names tests by display name."""
    ids = ids_by_name()
    name_of = {ids.get(n, n): n for n in names}

    def named(kept):
        return {name_of[k]: v for k, v in by_id(kept, ids).items() if k in name_of}
    if times_file:
        if times_file.lower().endswith('.json'):
            return {n: (v['s'], v.get('peak_mb')) if isinstance(v, dict) else (float(v), None)
                    for n, v in named(load_json(times_file)).items()}, times_file
        return {n: (s, None) for n, s in durations_from_log(times_file, names).items()}, times_file
    out = {n: (float(s), None) for n, s in named(load_json(SEED_FILE)).items()}
    for n, v in named(load_json(TIMES_FILE)).items():
        if isinstance(v, dict) and 's' in v:
            out[n] = (v['s'], v.get('peak_mb'))
    return out, f'{TIMES_FILE} (else {SEED_FILE})'


STALE_DAYS = 14             # a kept duration whose test is in no tier of this checkout goes after this many days


def prune_times(kept, names, today=None):
    """kept without the ids no tier of this checkout has (a removed test, or a display name kept before the ids)
    that no run has kept for STALE_DAYS (round QW1, audit AU2 S9: 60 such names were kept). An id only a sibling
    worktree has (a new test on a branch) stays while that worktree runs it."""
    import datetime
    day = datetime.date.fromisoformat(today or time.strftime('%Y-%m-%d'))

    def old(v):
        try:
            return (day - datetime.date.fromisoformat(str(v.get('when')))).days > STALE_DAYS
        except (AttributeError, ValueError):
            return True
    return {n: v for n, v in kept.items() if n in names or not old(v)}


def keep_times(results):
    """Keeps each passed test's duration and peak memory for the next run's order and --plan, by stable id."""
    if not results:
        return
    ids = ids_by_name()
    kept = prune_times(by_id(load_json(TIMES_FILE), ids), set(ids.values()))
    day = time.strftime('%Y-%m-%d')
    for name, secs, peak in results:
        kept[ids.get(name, name)] = {'s': round(secs, 1), 'peak_mb': peak, 'when': day}
    with contextlib.suppress(OSError):
        os.makedirs(os.path.dirname(TIMES_FILE), exist_ok=True)
        tmp = f'{TIMES_FILE}.tmp-{os.getpid()}'
        with open(tmp, 'w', encoding='utf-8') as f:
            json.dump(kept, f, indent=1, ensure_ascii=False, sort_keys=True)
        os.replace(tmp, TIMES_FILE)


# --- the schedule --------------------------------------------------------------------------------------------------

class Job:
    def __init__(self, idx, name, tier, args, needs, est, sid=None):
        self.idx, self.name, self.tier, self.args, self.est = idx, name, tier, args, est
        self.id = sid if sid is not None else name
        self.declared = needs is not None
        needs = needs if needs is not None else need(alone=True)
        self.cpu, self.mem, self.res = needs['cpu'], needs['mem'], needs['res']
        self.setup, self.alone = needs['setup'], needs['alone']

    def __repr__(self):
        return f'Job({self.name!r})'


class Scheduler:
    """Which tests start now. Build steps first, one at a time, in list order; then the longest chain first (a test's
    own time plus the longest chain of later tests that must wait for it), while a slot, the CPU threads and the
    memory they declare are free and no running test holds what they write or read. The first test held back for
    room gets a reservation: a shorter one starts before it only if it is expected to end by then or fits in what
    that test leaves over (backfilling), so a long or heavy test is never starved."""

    def __init__(self, jobs, slots, cpus, mem_gb):
        self.slots, self.cpus, self.mem = max(1, slots), max(1, cpus), max(0.5, mem_gb)
        self.setup = [j for j in jobs if j.setup]
        rest = sorted((j for j in jobs if not j.setup), key=lambda j: j.idx)
        chain: dict = {}
        for i in range(len(rest) - 1, -1, -1):
            j = rest[i]
            chain[j] = j.est + max((chain[k] for k in rest[i + 1:] if self.conflicts(j, k)), default=0.0)
        self.pending = sorted(rest, key=lambda j: (-chain[j], j.idx))
        self.running: dict = {}                             # job -> expected end

    def cpu_of(self, j):
        return self.cpus if j.cpu == 'all' else min(j.cpu, self.cpus)      # more than the PC has: fits when alone

    def mem_of(self, j):
        return min(j.mem, self.mem)

    def conflicts(self, a, b):
        return any(m == 'w' or b.res[r] == 'w' for r, m in a.res.items() if r in b.res)

    def fits(self, j, running):
        if any(r.alone for r in running):
            return False
        if j.alone:
            return not running
        return (len(running) < self.slots
                and sum(self.cpu_of(r) for r in running) + self.cpu_of(j) <= self.cpus
                and sum(self.mem_of(r) for r in running) + self.mem_of(j) <= self.mem + 1e-9)

    def reserve(self, j, now):
        """When j will fit as running tests end (their expected ends), and the room it leaves over then."""
        left = set(self.running)
        when = now
        for r, end in sorted(self.running.items(), key=lambda kv: (kv[1], kv[0].idx)):
            if self.fits(j, left):
                break
            left.discard(r)
            when = max(end, now)
        if j.alone or any(r.alone for r in left):
            return {'when': when, 'slots': 0, 'cpu': -1, 'mem': -1}
        return {'when': when, 'slots': self.slots - len(left) - 1,
                'cpu': self.cpus - sum(self.cpu_of(r) for r in left) - self.cpu_of(j),
                'mem': self.mem - sum(self.mem_of(r) for r in left) - self.mem_of(j)}

    def backfills(self, j, now, res):
        if now + j.est <= res['when']:
            return True
        if not j.alone and res['slots'] >= 1 and self.cpu_of(j) <= res['cpu'] and self.mem_of(j) <= res['mem'] + 1e-9:
            res['slots'] -= 1
            res['cpu'] -= self.cpu_of(j)
            res['mem'] -= self.mem_of(j)
            return True
        return False

    def start(self, now):
        """The tests to start now (marked running)."""
        out = []
        if self.setup:
            if not self.running:
                j = self.setup.pop(0)
                self.running[j] = now + j.est
                out.append(j)
            return out
        held = None
        for j in list(self.pending):
            # tests sharing a thing run in their listed order, as one at a time: each sees that thing as it did
            # before (test_apply reads the regress file the regression test, listed after it, rewrites)
            if (any(self.conflicts(j, r) for r in self.running)
                    or any(p.idx < j.idx and self.conflicts(j, p) for p in self.pending)):
                continue
            if self.fits(j, self.running):
                if held is not None and not self.backfills(j, now, held):
                    continue
                self.pending.remove(j)
                self.running[j] = now + j.est
                out.append(j)
            elif held is None:
                held = self.reserve(j, now)
        return out

    def finish(self, j):
        del self.running[j]

    def busy(self):
        return bool(self.setup or self.pending or self.running)


def simulate(sched):
    """The schedule as it would run if every test took its expected time: [(start, end, job)] in start order."""
    now, out = 0.0, []
    while sched.busy():
        for j in sched.start(now):
            out.append((now, now + j.est, j))
        if not sched.running:
            raise RuntimeError('scheduler stalled')         # cannot happen: an idle PC fits any test
        j, end = min(sched.running.items(), key=lambda kv: (kv[1], kv[0].idx))
        now = end
        sched.finish(j)
    return out


# --- the command line ----------------------------------------------------------------------------------------------

def physical_memory_gb():
    try:
        import ctypes

        class Status(ctypes.Structure):
            _fields_ = [('len', ctypes.c_ulong), ('load', ctypes.c_ulong), ('total', ctypes.c_uint64),
                        ('avail', ctypes.c_uint64), ('a', ctypes.c_uint64), ('b', ctypes.c_uint64),
                        ('c', ctypes.c_uint64), ('d', ctypes.c_uint64), ('e', ctypes.c_uint64)]
        s = Status()
        s.len = ctypes.sizeof(s)
        if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(s)):
            return s.total / 2**30
    except (AttributeError, OSError):
        pass
    return 16.0


def option(argv, names, convert, default):
    for n in names:
        if n in argv[:-1]:
            return convert(argv[argv.index(n) + 1])
    return default


def peak_gb(peak_mb):
    """A recorded peak (MB, the job object's PeakJobMemoryUsed) as GB, rounded up to a tenth; None stays None."""
    return None if not peak_mb else -(-int(peak_mb) * 10 // 1024) / 10


def make_jobs(chosen, times):
    """The chosen entries (id, name, tier, argv) as jobs: their NEEDS by id, their kept duration (times, by name)."""
    jobs = []
    for i, (sid, name, t, args) in enumerate(chosen):
        secs, peak = times.get(name, (GUESS[t], None))
        job = Job(i, name, t, args, NEEDS.get(sid), secs, sid)
        recorded = peak_gb(peak)
        if recorded is not None and job.declared and not job.setup and recorded > job.mem:
            job.mem = recorded                  # round QW1: never less than this PC measured last time
        jobs.append(job)
    return jobs


def print_plan(tier, jobs, slots, cpus, mem, source, known):
    sched = Scheduler(jobs, slots, cpus, mem)
    timeline = simulate(sched)
    total = max((e for _, e, _ in timeline), default=0.0)
    serial = sum(j.est for j in jobs)
    print(f'Plan: {tier}, {len(jobs)} test(s), {slots} at a time, {cpus} CPU threads, {mem:.1f} GB')
    print(f'durations: {known} of {len(jobs)} from {source}; the others guessed ({", ".join(f"{t} {s:.0f} s" for t, s in GUESS.items())})')
    print(f'{"start":>7s} {"end":>7s} {"cpu":>4s} {"GB":>4s}  test')
    for s, e, j in timeline:
        cpu = 'all' if j.cpu == 'all' else str(j.cpu)
        flags = ''.join([' [build step]' if j.setup else '', ' [alone: not in NEEDS]' if not j.declared else
                         ' [alone]' if j.alone else '',
                         ''.join(f' {r}:{m}' for r, m in sorted(j.res.items()))])
        print(f'{s:7.0f} {e:7.0f} {cpu:>4s} {j.mem:4.1f}  {j.name[:90]}{flags}')
    longest = max(jobs, key=lambda j: j.est) if jobs else None
    print(f'\nexpected wall-clock: {total:.0f} s ({total / 60:.1f} min); one at a time: {serial:.0f} s '
          f'({serial / 60:.1f} min)' + (f'; longest test: {longest.est:.0f} s' if longest else ''))
    print('(from durations measured one at a time: tests side by side run somewhat slower than this)')


class UsageError(ValueError):
    what_to_do = 'python run_tests.py --help lists the tiers and options'


FLAGS = ('--list', '--plan', '--no-game', '--ci-steps')
VALUED = {'--only': str, '-j': int, '--jobs': int, '--cpus': int, '--mem': float, '--times': str, '--timeout': float}
HELP = ('-h', '--help', '/?')


def bad_arguments(argv):
    """What is wrong with the command line in words, else None (round RT3: an argument the runner does not know -
    '--help' among them - was ignored before, so `run_tests.py --help` ran the quick tier)."""
    args, i, tiers = argv[1:], 0, []
    while i < len(args):
        a = args[i]
        if a in TIERS:
            tiers.append(a)
        elif a in VALUED:
            if i + 1 >= len(args):
                return f'{a} needs a value'
            try:
                VALUED[a](args[i + 1])
            except ValueError:
                return f'{a} takes {"a whole number" if VALUED[a] is int else "a number"}, not {args[i + 1]!r}'
            i += 1
        elif a not in FLAGS:
            return f'unknown argument {a!r}'
        i += 1
    if len(tiers) > 1:
        return f'one tier at a time, not {" and ".join(tiers)}'
    return None


def main(argv):
    safe_console()
    if any(a in HELP for a in argv[1:]):
        print(__doc__.rstrip())
        return 0
    wrong = bad_arguments(argv)
    if wrong:
        print('usage: python run_tests.py [quick | game | full] [--only <name>] [--list] [-j N] [--cpus N] '
              '[--mem GB] [--plan] [--times F] [--timeout S] [--no-game] [--ci-steps]', file=sys.stderr)
        suite_paths.use('core')
        from bfbc2core import errors
        errors.report(UsageError(f'run_tests: {wrong}'), show=False)       # logged, the cause and what to do
        return 2
    tier = next((a for a in argv[1:] if a in TIERS), 'quick')
    only = argv[argv.index('--only') + 1].lower() if '--only' in argv else None
    chosen = [x for x in tests() if TIERS.index(x[2]) <= TIERS.index(tier)
              and (only is None or only in x[1].lower() or only in x[0])]
    if only is not None and not chosen:
        # audit AU4 DF4: a mistyped --only ran nothing and said "0 of 0 passed", exit code 0
        print(f'run_tests: --only {only!r} matches no test of the {tier} tier (python run_tests.py {tier} --list '
              f'names them)', file=sys.stderr)
        return 2
    no_game, ci_steps = '--no-game' in argv, '--ci-steps' in argv
    if no_game and tier != 'quick':
        print(f'run_tests: --no-game runs the quick tier\'s game-free entries; the {tier} tier needs the game copy')
        return 2
    if no_game or ci_steps:
        unknown = unknown_skip_names()
        if unknown:
            print('run_tests: NOT_GAME_FREE / CI_STEPS name no test: ' + '; '.join(unknown))
            return 2
    chosen, skipped = skipped_entries(chosen, no_game, ci_steps)
    if '--list' in argv:
        for sid, name, t, _ in chosen:
            print(f'{t:6s} {name}  [{sid}]')
        for name, why in skipped:
            print(f'skip   {name}  ({why})')
        return 0
    threads = os.cpu_count() or 2
    slots = option(argv, ('-j', '--jobs'), int, max(1, min(8, threads // 2)))
    cpus = option(argv, ('--cpus',), int, threads)
    mem = option(argv, ('--mem',), float, round(physical_memory_gb() * 0.75, 1))
    names = {name for _, name, _, _ in chosen}
    times, source = known_times(names, option(argv, ('--times',), str, None))
    if '--plan' in argv:
        print_plan(tier, make_jobs(chosen, times), slots, cpus, mem, source, len(times))
        return 0
    given = option(argv, ('--timeout',), float, None)
    limits = {name: time_limit(t, times.get(name, (None, None))[0], given) for _, name, t, _ in chosen}
    if tier == 'full':
        os.environ[RUST_NO_SKIP_ENV] = '1'      # the full tier runs every crate's checks, green or not (round TS1)
    log(f'{tier}, {len(chosen)} test(s): {" ".join(argv[1:])}')
    root = opfor() if any(callable(a) or t != 'quick' for _, _, t, a in chosen) else None
    print(f'BFBC2 Tool Suite tests: {tier}, {len(chosen)} test(s)' + (f', OPFOR at {root}' if root else '')
          + (f', {slots} at a time' if slots > 1 else '') + (', game-free entries only' if no_game else ''))
    for name, why in skipped:
        print(f'skip {name}: {why}')
    global SKIPPED
    SKIPPED = len(skipped)
    if any(not callable(a) and len(a) > 1 and str(a[1]).endswith(('test_rust.py', 'build_native.py'))
           for _, _, _, a in chosen):
        suite_paths.use('core')
        from bfbc2core import native
        native.start_compile_cache()        # outside every test's job object (they end their processes)
    # the tests read bundles without the bundle cache's disk files (round E3e): a sweep of the game copy would fill
    # the user's cache with every bundle; the cache's own tests give theirs a folder of their own
    os.environ.setdefault('BFBC2_FBRB_CACHE', '0')
    warm_matplotlib()
    if slots <= 1:
        return run_in_turn(chosen, root, limits)
    return run_side_by_side(make_jobs(chosen, times), root, slots, cpus, mem, limits)


RUST_NO_SKIP_ENV = 'BFBC2_RUST_NO_SKIP'     # tests\test_rust.py: every crate checked, green or not


def log(message):
    """A line in the run's day log (round TS1, audit AU3 TS-12: the log held only "started" lines, so a tier's
    results could not be read back). Nothing when the runner was imported without errors.setup (the tests)."""
    import logging
    logging.getLogger('bfbc2').info(message)


MPL_DIR = os.path.join(os.path.dirname(TIMES_FILE), 'matplotlib')


def warm_matplotlib():
    """matplotlib's config and font cache in a stable folder of the suite's (MPLCONFIGDIR, unless set), built once
    here before any test: a test's first import of matplotlib otherwise printed "Matplotlib is building the font
    cache; this may take a moment." on its stderr, which failed a test comparing stderr (round QW1, audit AU2 Q10;
    RT1's O4 navtools). Nothing happens without matplotlib."""
    os.environ.setdefault('MPLCONFIGDIR', MPL_DIR)
    folder = os.environ['MPLCONFIGDIR']
    with contextlib.suppress(OSError):
        os.makedirs(folder, exist_ok=True)
        if any(f.startswith('fontlist') for f in os.listdir(folder)):
            return
    with contextlib.suppress(OSError, subprocess.SubprocessError):
        subprocess.run([PY, '-c', 'import matplotlib.font_manager'], capture_output=True, timeout=300)


def report(chosen_names, failures, start, passed_times, no_data=()):
    keep_times(passed_times)
    for name, out in failures:
        print(f'\n--- {name} (last 25 lines) ---')
        print('\n'.join(out.splitlines()[-25:]))
    if no_data:
        print(f'\nskipped for want of data, a program or a graphics adapter ({len(no_data)}):')
        for name, last in no_data:
            print(f'  {name}: {last[:110]}')
    out_of = {n for n, _ in failures} | {n for n, _ in no_data}
    partly = [n for n in chosen_names if PART_SKIPS.get(n) and n not in out_of]
    if partly:
        print(f'\npassed, with parts skipped for want of data (their "skipped" lines; {len(partly)}):')
        for name in partly:
            print(f'  {name}: {PART_SKIPS[name]} part(s)')
    passed = len(chosen_names) - len(failures) - len(no_data)
    total = (f'{passed} of {len(chosen_names)} passed in {time.time() - start:.0f} s'
             + (f'; {len(no_data)} skipped (ran nothing: listed above)' if no_data else '')
             + (f'; {len(partly)} of them with parts skipped' if partly else '')
             + (f'; {SKIPPED} left out (listed at the start)' if SKIPPED else ''))
    print('\n' + total)
    log(total + (f'; failed: {"; ".join(n for n, _ in failures)}' if failures else ''))
    return 1 if failures else 0


def run_in_turn(chosen, root, limits=None):
    """-j 1: one test after another in the listed order (the runner as it was before 2026-11-17)."""
    failures, passed, no_data, start = [], [], [], time.time()
    for _sid, name, _t, args in chosen:
        if callable(args):
            args = args(root)
        ok, out, last, secs, peak = run_one(name, args, (limits or {}).get(name))
        print(result_line(ok, name, secs, last), flush=True)
        log(result_line(ok, name, secs, last))
        if skipped(ok, last):
            no_data.append((name, last))
        elif ok:
            passed.append((name, secs, peak))
        else:
            failures.append((name, out))
    return report([c[1] for c in chosen], failures, start, passed, no_data)


def run_side_by_side(jobs, root, slots, cpus, mem, limits=None):
    import queue
    import threading
    import traceback
    undeclared = [j.name for j in jobs if not j.declared]
    if undeclared:
        print(f'{len(undeclared)} test(s) not in NEEDS run alone: ' + '; '.join(undeclared), flush=True)
    sched = Scheduler(jobs, slots, cpus, mem)
    done: queue.Queue = queue.Queue()

    def work(j):
        try:
            args = j.args(root) if callable(j.args) else j.args
            done.put((j, *run_one(j.name, args, (limits or {}).get(j.name))))
        except BaseException:                       # noqa: BLE001 - said as this test's failure, not lost in a thread
            done.put((j, False, traceback.format_exc(), 'the runner could not start it', 0.0, None))

    results, passed, start = {}, [], time.time()
    try:
        while sched.busy():
            for j in sched.start(time.time() - start):
                threading.Thread(target=work, args=(j,), daemon=True).start()
            j, ok, out, last, secs, peak = done.get()
            sched.finish(j)
            print(result_line(ok, j.name, secs, last), flush=True)
            log(result_line(ok, j.name, secs, last))
            results[j.name] = (ok, out, last)
            if ok and not skipped(ok, last):
                passed.append((j.name, secs, peak))
    except KeyboardInterrupt:
        for group in list(RUNNING.values()):
            group.kill()
        raise
    ordered = sorted(jobs, key=lambda j: j.idx)
    failures = [(j.name, results[j.name][1]) for j in ordered if not results[j.name][0]]
    no_data = [(j.name, results[j.name][2]) for j in ordered if skipped(results[j.name][0], results[j.name][2])]
    return report([j.name for j in ordered], failures, start, passed, no_data)


if __name__ == '__main__':
    suite_paths.use('core')
    from bfbc2core import errors
    errors.setup('run-tests', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
