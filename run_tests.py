r"""Runs the suite's tests: one line per test, the failures' output at the end, exit code 1 if any failed.

  python run_tests.py [quick | game | full] [--only <name>] [--list] [-j N] [--cpus N] [--mem GB] [--plan] [--times F]

  quick   no game data, about a minute: the Rust core (built when stale, its cargo checks, its DBX reader against
          the retired Python reader's recorded answers), repository rules, DbxEditGUI's unit tests, and on files
          built by the tests: DbxEdit's and FbrbTool's commands against the retired exes' recorded answers,
          LevelDiff, the crash reader and exeref, the console readers, the menu tools, the test harness (needs
          Windows for the C# tools: DbxEditGUI, LevelDiff)
  game    + Project OPFOR BFBC2's game copy, about 13 minutes: the DBX reader against the retired Python reader's
          recorded answers and DbxEdit's fingerprints on every unpacked .dbx, FbrbTool's commands on 12 game
          bundles, water on every level,
          the level loader against its snapshot, terrain against the in-game samples, path-data and mesh
          round trips, LevelDiff on MP_005, the crash reader's code view and exeref on BFBC2Game.exe, the menu
          tools on the game's movies and OPFOR's menu patches, the console
          readers on your own Xbox 360 files (skipped when they are not on the PC), the 3D tools opening a
          level (needs a display), the structure of DICE's path data (hpdcheck), the test harness on two small
          levels, NavKit's regression test on two small levels
  full    + the corpus runs, about 90 minutes: DbxEditGUI's DBX classes and the DBX reader on every unpacked
          .dbx, FbrbTool's commands on every game bundle, NavKit's regression test on the 13 campaign levels and the imported
          Onslaught maps (about 5 hours)

Run `quick` before every commit and `game` before every push; `full` after changes to a format reader.

Side by side (2026-11-17). Tests run N at a time (-j N; default half the CPU threads, at most 8); -j 1 runs them one
after another in the listed order, exactly as before. Each test declares what it needs in NEEDS (below tests()):
  cpu     the CPU threads it keeps busy (1; its process pool's size; 'all' for Rust batch work and cargo)
  mem     its peak memory in GB (an estimate; the runner measures every test's peak and keeps it, see --plan)
  res     shared things it reads ('r') or writes ('w'): a writer never runs beside a reader or another writer of the
          same thing (OPFOR's work\navkit\regress, the Navigation Studio cache, renderqa's build\ folder, a display ...),
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


def stale_build(folder, exes, sources):
    """A build step that runs folder's build.bat only when its .exe files are older than their sources
    (tests\\build_if_stale.py): rebuilding every run gave the tools new bytes and times (QA 2026-11-15)."""
    d = os.path.join(SUITE, folder)
    return [PY, os.path.join(SUITE, 'tests', 'build_if_stale.py'), os.path.join(d, 'build.bat'),
            *[os.path.join(d, e) for e in exes], '--', *[os.path.join(SUITE, s) for s in sources]]


DBX_BUILD = stale_build('bfbc2-dbx-tools', ['DbxEditGUI.exe'],
                        [os.path.join('bfbc2-dbx-tools', 'DbxEdit.cs'), os.path.join('bfbc2-dbx-tools', 'DbxEditGUI.cs')])


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


def tests():
    """(name, tier, argv). Game paths are only looked up for the tiers that need them."""
    t = [
        ('Rust core build (when stale; skipped without Rust)', 'quick', [PY, os.path.join(SUITE, 'rust', 'build_native.py'), '--if-stale']),
        ('Rust core: cargo fmt, clippy, tests (skipped without Rust)', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_rust.py')]),
        ("DBX reader (Rust) against the retired Python reader's recorded answers (built, damaged, 200 corpus files)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx_native.py')]),
        ("FBRB reader (Rust) against the retired Python reader's recorded answers and FbrbTool.exe's recorded unpacks (built, damaged, 8 game bundles)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_native.py')]),
        ("Rust FBRB bundle cache against the reader without it and the Python reader's recorded answers (built, damaged, changed, 4 processes, 3 game bundles)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_cache.py')]),
        ("terrain, water and collision readers (Rust) against the retired Python readers' recorded answers (built, damaged, aliases from binary partitions)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_geometry_native.py')]),
        ('Rust shader database reader / writer against shaderdb.py (built, damaged, writer fallback, merge, mp_008cq)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_native.py')]),
        ('Rust shader database handle and queries against shaderdb.py (never made, made, changed lists; blocks, checks, writer, mp_008cq)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_queries.py')]),
        ('Rust shader translator: decoder against xenos.py, translations against the recorded Python (built programs, damaged blobs, random microcode, dispatch, hand-built trees)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xenos_native.py')]),
        ("cull tree element reader (Rust) against the retired Python reader's recorded answers, level search against glob (built, damaged, listing, 2 work trees)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree_native.py')]),
        ('Rust Granny reader against granny.py, Xbox 360 -> PC conversion against its recorded results (built, damaged, batch, writes)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_native.py')]),
        ('Rust texture converter against its recorded results (built, damaged, batch)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture_native.py')]),
        ("Rust EA-XAS audio encoder / decoder against its recorded results (shift rule, seeded PCM of every shape, random frames, DICE's blocks)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_audio_native.py')]),
        ('Rust NavKit grid walks against their recorded results, E2a / E3c / E3h pieces against their Python (random levels, whole stages)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navbuild_native.py')]),
        ("E4e: NavKit's tools in the Rust core against their Python (vehicle obstacle distance, domain searches, level scan, parity measures; built inputs)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navtools_native.py')]),
        ("map editor (Rust), headless: theme from tokens.json, outliner against the Map Inspector's lists (4 levels), offscreen render, outline, picking, palette, views", 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_editor.py')]),
        ('Rust core required: no module, a stale one, the source fingerprint', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_native_required.py')]),
        ('review regressions: HPD selection, launcher completion, mypy failures', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_review_regressions.py')]),
        ('BF1943 asset accuracy: texture palettes, masks, emitters, shader fetches', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_asset_accuracy.py')]),
        ('repository rules', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_repo.py')]),
        ('test runner: a failure printed on any console encoding', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_runner_output.py')]),
        ('test runner: tests side by side (resources, failures, exit code, output per test)', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_runner_parallel.py')]),
        ('lint (ruff check, every folder)', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_lint.py')]),
        ('types (mypy, every folder)', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_types.py')]),
        ('core edges (terrain / water names, errors in threads)', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_core_edges.py')]),
        ('paths: OPFOR, game copy, tools and sibling projects on a stand-in tree', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_paths.py')]),
        ('converter edges (dbx numbers, dbxport references, texture check)', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_converter_edges.py')]),
        ('converter units (havok w lanes, MOPP byte code, L8 textures and flags, mesh rename, texcoords, mesh relayout)', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_converter_units.py')]),
        ('DbxEditGUI build (when stale)', 'quick', DBX_BUILD),
        ('DbxEditGUI and the DBX classes it is built on (DbxEdit.cs): unit tests and the window', 'quick', ['cmd', '/c', os.path.join(SUITE, 'bfbc2-dbx-tools', 'tests', 'run_tests.bat')]),
        ('FbrbTool round trip, built bundle', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb.py')]),
        ("DBX writer (Rust) against the retired Python writer's recorded bytes (edge, random and edited trees, 200 corpus files, DbxEdit roundtrip)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx_write_native.py')]),
        ("DbxEdit's commands in process against DbxEdit.exe's recorded answers (the unit tests' 82 cases, the drop-in runner, the batch, the log)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbxedit.py')]),
        ("FbrbTool's commands in process against FbrbTool.exe's recorded answers (228 command lines, 100 damaged bundles, .NET's gzip reader, the drop-in runner)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrbtool.py')]),
        ("Rust FBRB packer against FbrbTool.exe's recorded bundles, zlib against Python's (built folders in every layout, 8 game bundles)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_pack_native.py')]),
        ("Rust FBRB unpack against FbrbTool.exe's recorded unpacks (built, odd and damaged bundles, 8 game bundles)", 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_unpack_native.py')]),
        ('error handling and logs', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_errors.py')]),
        ('cache caps and pruning (bfbc2core.caches): least recently used first, writers and readers beside it, the command', 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_caches.py')]),
        ('log caps and day rollover, project caches in work\\ (bfbc2core.caches), the command', 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_logs_caches.py')]),
        ('every command-line tool reports errors', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_cli_errors.py')]),
        ('review fixes: apply inputs, stock-MP reinstall, studio flags, cache key', 'quick', [PY, os.path.join(SUITE, 'tests', 'test_review_fixes.py')]),
        ('review fixes 2: harness missing files, navkit no-original reinstall, old fbrb manifests', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_review_fixes2.py')]),
        ('deep scan fixes: atomic installs and writes, honest caches, damaged files said', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_deep_scan_fixes.py')]),
        ('navigation side files (navtune)', 'quick', [PY, os.path.join(SUITE, 'core', 'tests', 'test_navtune.py')]),
        ('NavKit water rule (ground too deep under water)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_water_rule.py')]),
        ('NavKit walkability edits (navedits)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navedits.py')]),
        ('NavKit area link rule', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_area_link.py')]),
        ('NavKit regression progress estimates', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_regress_progress.py')]),
        ('NavKit boundary diagnostic (navboundary)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navboundary.py')]),
        ('NavKit portal symmetry', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_portal_symmetry.py')]),
        ('NavKit reference extent clip', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_reference_clip.py')]),
        ('NavKit soldiers absent from path data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_characters.py')]),
        ('NavKit headroom band', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_headroom.py')]),
        ('NavKit narrow lips and door sills', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_lips.py')]),
        ('NavKit dynamic props (floor under a prop, door leaves)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_dynamic_props.py')]),
        ('NavKit roof pruning', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_roof_prune.py')]),
        ('NavKit objective floors (M-COM)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_objective_floor.py')]),
        ('NavKit 45-degree bridges: closed outlines', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_bridge_union.py')]),
        ('NavKit D lines and F cells', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_detail_features.py')]),
        ('NavKit collision index (Havok packfile names)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_havok_index.py')]),
        ('NavKit boat mesh (navwater_mesh)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navwater_mesh.py')]),
        ('NavKit routes across domains (navdomains)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navdomains.py')]),
        ('NavKit vehicle path data (navvehicle)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_vehicle.py')]),
        ('NavKit shadow islands (navislands)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navislands.py')]),
        ('NavKit offline sampler (navsample_offline)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navsample_offline.py')]),
        ('NavKit tool edges (ordercheck, detailed cache, navdiff pictures)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_tool_edges.py')]),
        ('Navigation Studio edit mode, no game (editing, overlay usage)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-navigation-studio', 'tests', 'test_editing_offline.py')]),
        ('NavKit driver, stand-in OPFOR folder (navkit.py steps, install, main)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navkit_offline.py')]),
        ('Navigation Studio logic, no window (studio.py)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-navigation-studio', 'tests', 'test_studio_offline.py')]),
        ('Map Inspector logic, no window (inspector.py)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-map-inspector', 'tests', 'test_inspector_offline.py')]),
        ('mesh viewer index and cache, stand-in bundles', 'quick', [PY, os.path.join(SUITE, 'bfbc2-mesh-viewer', 'tests', 'test_meshview_index.py')]),
        ('LevelDiff build (when stale)', 'quick', stale_build(os.path.join('bfbc2-diff-workbench', 'level-diff'), ['LevelDiff.exe'],
                                                              [os.path.join('bfbc2-diff-workbench', 'level-diff', 'LevelDiff.cs'),
                                                               os.path.join('bfbc2-dbx-tools', 'DbxEdit.cs')])),
        ('LevelDiff, built levels', 'quick', [PY, os.path.join(SUITE, 'bfbc2-diff-workbench', 'level-diff', 'tests', 'test_leveldiff.py')]),
        ('crash reader, built reports', 'quick', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_crashreader.py')]),
        ('console readers, built images', 'quick', [PY, os.path.join(SUITE, 'bfbc2-console-readers', 'tests', 'test_console_readers.py')]),
        ('menu tools (aptcheck, aptrefs, bc2loc), built data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_menu_tools.py')]),
        ('APT writer and grafter (aptgraft), built movies', 'quick', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_aptgraft.py')]),
        ('APT decoder (Rust) and aptgraft writer against their references (E3n; built movies, 600 damaged, 3,000 floats, grafts)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_apt_e3n.py')]),
        ('APT model, aptcheck and aptrefs in Rust against the Python (E4c; built movies, object rules, 300 damaged, grafts, edits, crosshair-add)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_apt_e4c.py')]),
        ('static cull trees (culltree), built data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree.py')]),
        ('cull tree reference rows cache and shape memo against the reference (E3m; built levels: no cache, cold, warm, changed)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree_e3m.py')]),
        ('Rust E3o: mesh reader, navdata cells and union-find, route search, texqa workers against their references, havok verify record against its recorded results (built inputs)', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_e3o_native.py')]),
        ('Rust F0 path data and runtime-mesh writer against hpdtool / kymesh (built files and meshes, 1,500 damaged, 600 changed, 4 game files)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_pathdata_native.py')]),
        ('Rust F0 localization files and BIGF archives against bc2loc / menucommon (built, damaged, 3,000 keys, the English file, the menu movies)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_bc2loc_native.py')]),
        ('Rust F0 console containers against stfs.py / xiso.py (built packages and discs, 60 damaged packages, 30 damaged discs)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-readers', 'tests', 'test_console_readers_native.py')]),
        ('Rust F0 Wave resources and EA streams against audio.py (built, 3,000 damaged, 600 changed, 40 BF1943 Waves)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_wave_native.py')]),
        ('Rust F0 mesh headers and mesh sets against mesh.py / meshset.py (built, 3,000 damaged, 800 changed, 60 BF1943 resources)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_meshdata_native.py')]),
        ("Rust F0 texture.py helpers and PC texture reader against their Python (tiled, packed, layout, levels, decode; built, damaged)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture_helpers_native.py')]),
        ('Rust F0 havok.classes against its Python (built and damaged executables, BFBC2Game.exe)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_havok_classes_native.py')]),
        ('Rust F2a console converter against terrain.py / soot.py / mesh.py / havok.py / texture.py (built, damaged, edited relayouts, 40 BF1943 resources)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_console_f2a_native.py')]),
        ('Rust F2a model tools against texwrite.py / skinmesh.py (built pictures and skinned LODs, damaged, the Garand and Type 5 LODs)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_model_f2a_native.py')]),
        ("Rust F2a Granny writing against granny.py (CRC, shift, orientations, add_tracks on the WW2 rifles' animations, damaged)", 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_f2a_native.py')]),
        ('Rust F3 formats against aimanim / hpdtool mesh layout / audio framing / navsample / hkpack / xenos.disasm (built, damaged, changed, a few real files)', 'quick',
         [PY, os.path.join(SUITE, 'tests', 'test_f3_native.py')]),
        ('exe string / address references (exeref), built exe', 'quick', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_exeref.py')]),
        ('test harness, built data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'tests', 'test_harness.py')]),
        ('crash reader, exeref and test harness: the Rust core against the reference (seeded x86, built exes, logs, graphs)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_crash_native.py')]),
        ('stock-MP AI layer, synthetic levels', 'quick', [PY, os.path.join(SUITE, 'bfbc2-stockmp-ai', 'tests', 'test_stockmp.py')]),
        ('stock-MP AI validation checks, synthetic builds (validate.py)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-stockmp-ai', 'tests', 'test_validate.py')]),
        ('shader databases (shaderdb), built files', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb.py')]),
        ('shader databases: Xbox 360 (BFBC2 twins, BF1943 Wake)', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb.py'), '--xbox']),
        ('render harness (renderqa): built mesh, shaders and texture drawn and checked', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_renderqa.py')]),
        ('render harness (renderqa): stock meshes with their own level\'s shaders', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_renderqa.py'), '--game']),
        ('shader translator differential test: Xenos interpreter semantics, cube fetch direction', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xdiff.py')]),
        ('HLSL compile cache and batches (pcbuild): deterministic compiler, compile_many / prefetch, disk cache', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_pcbuild_batch.py')]),
        ('shaderview -serve against one process per job (built jobs, failures, state between jobs, threads, memory)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderview_serve.py')]),
        ('shader translator differential test: BF1943 shaders behind each translator fix exact', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xdiff.py'), '--game']),
        ('HLSL batches (pcbuild): every Wake pixel shader translated, compile_many against one at a time', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_pcbuild_batch.py'), '--game']),
        ('shader translator differential test: every Wake pixel shader served on threads against one process each, one at a time', 'game', [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xdiff.py'), '--game-all']),
        ('Granny Xbox 360 -> PC: DICE bone tables', 'quick', [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_port.py')]),
        ('BF1943 import pipeline, no data', 'quick', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_bf1943import.py')]),
        ('dbxport rules: the 19 edge cases, templates, references, schema, value passes', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_rules.py')]),
        ('dbxport ranking index against the reference scan (20,000 random rankings, Templates.choose)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_ranking.py')]),
        ('dbxport tables in Rust against the Python reference: every entry, changed in place as OPFOR changes them', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_tables.py')]),
        ('dbxport in Rust against its Python reference (600 random partitions, pools, catalogues, options)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_native.py')]),
        ('BF1943 ports E3g: Rust mesh vertices and Havok walk against their recorded results, layout and emitter indexes against their references (built inputs)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_ports_native.py')]),
        ('F2b: mopp and havokport in Rust against their Python (seeded MOPP codes, 4.6.1 packfiles, PC resources)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_havokport_native.py')]),
        ('F2b: dbxcatalogue.build in Rust against its Python (seeded games of built bundles)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxcatalogue_native.py')]),
        ('F2b: emitteratlas build, prove and same_but_uv in Rust against their Python (40 synthetic levels)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_emitteratlas_native.py')]),
        ('F2b: menucommon.PEImage in Rust against its Python (400 built executables)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_pe_native.py')]),
        ('F2b: xmlinst in Rust against its patterns (3,000 seeded texts)', 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_xmlinst_native.py')]),
        ('F2b: navtune validate, side-file text and summary in Rust against its Python (seeded operations and files)', 'quick',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_navtune_native.py')]),
        ('dbxport fair ports, 7 partitions against DICE (skipped without game data)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_fair.py'), '--quick']),
        ('emitter atlases merged, emitters re-pointed (synthetic)', 'quick', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_emitteratlas.py')]),
        ('texture QA: decoding, Xbox untiling, flips / swizzles / degenerate, baseline (synthetic)', 'quick',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texqa.py')]),
        ("DBX reader (Rust) against the retired Python reader's recorded answers, every corpus file", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx_native.py'), '--game']),
        ('Rust DBX reader against DbxEdit, every fingerprinted file; every bundle extracted (cargo)', 'game',
         [PY, os.path.join(SUITE, 'tests', 'test_rust.py'), '--corpus']),
        ("FBRB reader (Rust) against the retired Python reader's recorded answers, every bundle", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_native.py'), '--game']),
        ('Rust FBRB bundle cache: every 3rd game bundle read in a random order, the stored bundles', 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_cache.py'), '--game']),
        ("DBX writer (Rust) against the retired Python writer's recorded bytes, every corpus file written back and edited", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx_write_native.py'), '--game']),
        ("DbxEdit's commands in process against DbxEdit.exe's recorded answers, 1,592 corpus files through every command and every edit fingerprint", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbxedit.py'), '--game']),
        ("FbrbTool's commands in process against FbrbTool.exe's recorded answers, 40 game bundles unpacked, listed and packed, 600 damaged bundles, the logged invocations' bundles", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrbtool.py'), '--game']),
        ("Rust FBRB packer against FbrbTool.exe's recorded bundles, every bundle of the game copy unpacked and packed again", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_pack_native.py'), '--game']),
        ("Rust FBRB unpack against FbrbTool.exe's recorded unpacks, every game bundle with a payload up to 300 MiB", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb_unpack_native.py'), '--game']),
        ("terrain, water and Havok readers (Rust) on every bundle of the game copy, the collision index of every prepared level, against the retired Python readers' recorded answers", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_geometry_native.py'), '--game']),
        ('Rust shader databases on every database of the game copy, the Xbox and BF1943 levels and work; the WW2 merges', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_native.py'), '--game']),
        ('Rust shader database queries on every database of the game copy, the Xbox and BF1943 levels and work', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_shaderdb_queries.py'), '--game']),
        ('Rust shader translator on every program of the BF1943 and BFBC2 Xbox shader databases (trees, HLSL, errors)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests', 'test_xenos_native.py'), '--game']),
        ("cull trees (Rust) on every tree and level partition of the game copy and work against the retired Python reader's recorded answers; listings; the recorded add run", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree_native.py'), '--game']),
        ('Rust Granny reader and conversion on every Granny resource of your Xbox files, the game copy and BF1943', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_native.py'), '--game']),
        ('cull tree add and verify on Onslaught MP_005 against every work\\levels folder: reference, no cache, cold, warm (E3m)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-culltree', 'tests', 'test_culltree_e3m.py'), '--game']),
        ("Rust E3o on the game copy: every path-data sector, navdata.load of DICE's prepared levels, 400 routes, verify-havok's Onslaught files, texqa on the WW2 vehicles", 'game',
         [PY, os.path.join(SUITE, 'tests', 'test_e3o_native.py'), '--game']),
        ('Rust F0 path data on every pathdata.hpd and kept original of the game copy and work: parsed, written and every runtime mesh written both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_pathdata_native.py'), '--game']),
        ('Rust F0 localization on every language file of the overlay bundles and the pristine copy, BIGF on every menu / HUD movie, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_bc2loc_native.py'), '--game']),
        ('Rust F0 console containers on your Xbox disc image and Update packages, 1,500 damaged packages, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-readers', 'tests', 'test_console_readers_native.py'), '--game']),
        ('Rust F0 Wave resources and streams on every BF1943, BFBC2 Xbox and game-copy Wave, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_wave_native.py'), '--game']),
        ('Rust F0 mesh headers and mesh sets on every MeshData and mesh set of BF1943, the Xbox bundles and the game copy, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_meshdata_native.py'), '--game']),
        ('Rust F0 texture levels and decoding on every PC texture of the game copy, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture_helpers_native.py'), '--game']),
        ('Rust F0 havok.classes on damaged copies of BFBC2Game.exe', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_havok_classes_native.py'), '--game']),
        ('Rust F2a console converter on every BF1943 / Xbox terrain, soot, physics, shader database and mesh resource and the game copy (readers, decode, relayout, texture.check), both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_console_f2a_native.py'), '--game']),
        ('Rust F2a model tools on every game-copy DXT texture encoded again and every skinned LOD, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_model_f2a_native.py'), '--game']),
        ('Rust F2a Granny writing on every game-copy animation (shift, orientations, add_tracks), both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_f2a_native.py'), '--game']),
        ('Rust F3 formats on every aim animation, path-data sector, Xbox / BF1943 / PC Wave stream, sample run, packfile and Xbox shader program, both ways and against the recorded answers', 'game',
         [PY, os.path.join(SUITE, 'tests', 'test_f3_native.py'), '--game']),
        ("level loader (Rust scene) against the retired Python loader's recorded Levels, entity by entity (SP_01, MP_005, MP_009GR, ONS_MP_004)", 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_scene_native.py')]),
        ("NavKit's collision readers (detailed, ladders, offline sampler, boat mesh) on the Rust core against the retired Python path's recorded results (SP_009, SP_004_B)", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_collision_backends.py')]),
        ('path locators: the same answers as a reference checkout (main)', 'game', [PY, os.path.join(SUITE, 'tests', 'test_locators.py')]),
        ('LevelDiff on MP_005', 'game', [PY, os.path.join(SUITE, 'bfbc2-diff-workbench', 'level-diff', 'tests', 'test_leveldiff.py'), '--game']),
        ('crash reader code view (BFBC2Game.exe)', 'game', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_crashreader.py'), '--game']),
        ('console resource converter: physics (havok)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_havok.py')]),
        ('console resource converter: textures', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture.py')]),
        ('Rust texture converter: every BF1943 and Onslaught console texture as recorded', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_texture_native.py'), '--game']),
        ('console mesh decoding (Xbox vs PC, BF1943)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_mesh_decode.py')]),
        ('console terrain conversion (Xbox vs PC, BF1943 Wake)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_terrain.py')]),
        ('console soot meshes (Xbox vs PC, BF1943)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_soot.py')]),
        ('console sounds: EA-XMA -> EA-XAS (Xbox vs PC, BF1943)', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_audio.py')]),
        ('Rust EA-XAS audio encoder: every 10th BF1943 Wave and in-memory BFBC2 Xbox Wave converted as recorded, byte for byte', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_audio_native.py'), '--game', '--every', '10']),
        ('mesh viewer on the game\'s models', 'game', [PY, os.path.join(SUITE, 'bfbc2-mesh-viewer', 'tests', 'test_meshview.py')]),
        ('model tools: skinned meshes, mesh sets, shapes', 'game', [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_models.py')]),
        ('model tools: PC textures from pictures (DXT1)', 'game', [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_texwrite.py')]),
        ('model tools: mesh decimation (seams closed, bones apart)', 'game', [PY, os.path.join(SUITE, 'bfbc2-model-tools', 'tests', 'test_decimate.py')]),
        ('Granny skeletons and animations (read, shift)', 'game', [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny.py')]),
        ('Granny Xbox 360 -> PC on your Xbox files and BF1943', 'game', [PY, os.path.join(SUITE, 'bfbc2-anim-tools', 'tests', 'test_granny_port.py'), '--game']),
        ('BF1943 import pipeline on your BF1943 copy', 'game', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_bf1943import.py'), '--game']),
        ('BF1943 model port (meshport)', 'game', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_meshport.py')]),
        ('DBX writer and BF1943 partition port (dbxport)', 'game', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport.py')]),
        ('dbxport fair ports: every partition both games ship, against DICE', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_fair.py')]),
        ("dbxport ranking: the four WW2 level ports against a third of BFBC2's 157 levels, every choice and the bytes", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_ranking.py'), '--game']),
        ('dbxport in Rust against its Python reference: the Project OPFOR BF1943 vehicle, weapon and object builds replayed both ways (outputs to temp)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_opfor.py')]),
        ('dbxport in Rust against its Python reference: every fair, counterpart and effect port both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxport_native.py'), '--game']),
        ('BF1943 ports E3g: every BF1943 mesh set, physics file and effect, the Onslaught console meshes and physics, as recorded and both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_ports_native.py'), '--game']),
        ('F2b: mopp and havokport in Rust on every BF1943 physics file, every BFBC2 physics resource and their MOPP codes, both ways', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_havokport_native.py'), '--game']),
        ('F2b: dbxcatalogue.build in Rust against its Python on the game copy (84,739 partitions)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_dbxcatalogue_native.py'), '--game']),
        ('F2b: emitteratlas in Rust against its Python on the inputs of the four OPFOR emitter_atlas runs', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_emitteratlas_native.py'), '--game']),
        ('F2b: menucommon.PEImage in Rust against its Python on every executable of the game copy', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_pe_native.py'), '--game']),
        ('F2b: xmlinst in Rust against its patterns on every converted level XML of OPFOR work levels', 'game',
         [PY, os.path.join(SUITE, 'core', 'tests', 'test_xmlinst_native.py'), '--game']),
        ('BF1943 physics port, Havok 4.6.1 -> 5.5.0 (havokport)', 'game', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests', 'test_havokport.py')]),
        ('localization files written (bc2loc set)', 'game', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_bc2loc_write.py')]),
        ('console readers on your Xbox files', 'game', [PY, os.path.join(SUITE, 'bfbc2-console-readers', 'tests', 'test_console_readers.py'), '--game']),
        ('menu tools on the game copy and OPFOR\'s patches', 'game', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_menu_tools.py'), '--game']),
        ('APT decoder (Rust) and aptgraft writer against their references on every movie of the game copy, the pristine menus and the Xbox HUD copies; aptrefs / aptcheck / bc2loc both ways (E3n)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_apt_e3n.py'), '--game']),
        ('APT model, aptcheck and aptrefs in Rust against the Python on the 133 movies of E3n: every write, roundtrip --pack, grafts, aliases, crosshair-add (E4c)', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests', 'test_apt_e4c.py'), '--game']),
        ('exeref on BFBC2Game.exe', 'game', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_exeref.py'), '--game']),
        ('crash reader and exeref: the Rust core against capstone on BFBC2Game.exe, both command lines byte for byte, opfor.log sessions', 'game', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'tests', 'test_crash_native.py'), '--game']),
        ('FbrbTool round trip, 12 game bundles', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb.py'), '--game', '12']),
        ('water, every level', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_water.py')]),
        ('level loader snapshot', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_level.py')]),
        ('terrain against in-game samples', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_terrain.py')]),
        ('path data round trip (hpdtool)', 'game', lambda root: [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdtool.py'),
                                                               'roundtrip', *pathdata_files(root)]),
        ('navigation mesh round trip (kymesh)', 'game', lambda root: [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdtool.py'),
                                                                    'meshtrip', *pathdata_files(root)]),
        ('navigation data and the path test', 'game', [PY, os.path.join(SUITE, 'core', 'tests', 'test_navdata.py')]),
        ("path data structure, DICE's files (hpdcheck)", 'game', lambda root: [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdcheck.py'),
                                                                             *dice_pathdata_files(root)]),
        ('NavKit boat mesh and routes on ONS_MP_004', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navwater_mesh.py'), '--game']),
        ('NavKit vehicle path data on ONS_MP_002', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_vehicle_level.py')]),
        ('NavKit links and route edges', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_links.py')]),
        ('traversals, the apply report, installing', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_traversal.py')]),
        ('navkit apply with walkability edits', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_apply.py')]),
        ('Navigation Studio edit mode', 'game', [PY, os.path.join(SUITE, 'bfbc2-navigation-studio', 'tests', 'test_editing.py')]),
        ('the suite window (hub) and the level picker', 'game', [PY, os.path.join(SUITE, 'tests', 'test_hub.py')]),
        ('3D tools open and draw (Map Inspector, Navigation Studio)', 'game', [PY, os.path.join(SUITE, 'tests', 'test_viewers.py')]),
        ('test harness on sp_009 and sp_004_b', 'game', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'tests', 'test_harness.py'), '--game']),
        ('stock-MP AI layer, Atacama Conquest built', 'game', [PY, os.path.join(SUITE, 'bfbc2-stockmp-ai', 'tests', 'test_stockmp.py'), '--game']),
        ('stock-MP AI: the Rust core against the Python on every MP level partition', 'game', [PY, os.path.join(SUITE, 'bfbc2-stockmp-ai', 'tests', 'test_native_levels.py')]),
        ('Rust NavKit builds: sp_009 and sp_004_b path data as recorded', 'game',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navbuild_native.py'), '--game', '--levels', 'sp_009,sp_004_b']),
        ("E4e: NavKit's tools on the game copy as recorded from their Python (ONS_MP_002 vehicle data, ONS_MP_004 boat mesh and domains, sp_009 / sp_004_b parity measures, sp_009 offline samples)", 'game',
         [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests', 'test_navtools_native.py'), '--game']),
        ('NavKit regression, 2 small levels', 'game', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'navregress.py'),
                                                       '--levels', 'sp_004_b,sp_009']),
        ('NavKit regression, all levels with DICE path data', 'full', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'navregress.py')]),
        ("DbxEditGUI's DBX classes (DbxEdit.cs) on the corpus: round trip, XML import identity, edited imports", 'full', lambda root: ['cmd', '/c', os.path.join(SUITE, 'bfbc2-dbx-tools', 'tests', 'run_tests.bat'),
                                                                 os.path.join(root, 'work', 'levels'), os.path.join(root, 'work', 'navkit', 'common')]),
        ('DBX reader against DbxEdit, every unpacked .dbx', 'full', lambda root: [PY, os.path.join(SUITE, 'core', 'tests', 'test_dbx.py'),
                                                                   os.path.join(root, 'work', 'levels'), os.path.join(root, 'work', 'navkit', 'common')]),
        ('FbrbTool round trip, every game bundle', 'full', [PY, os.path.join(SUITE, 'core', 'tests', 'test_fbrb.py'), '--game', '0']),
        ('Rust EA-XAS audio encoder: every BF1943 Wave and in-memory BFBC2 Xbox Wave converted as recorded, byte for byte', 'full',
         [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_audio_native.py'), '--game']),
    ]
    return t


def need(cpu: int | str = 1, mem: float = 0.5, setup: bool = False, alone: bool = False, **res: str) -> dict:
    """A test's needs (see the docstring): need(cpu='all', mem=4, regress='r')."""
    return {'cpu': cpu, 'mem': mem, 'setup': setup, 'alone': alone, 'res': res}


# What each test needs to run beside others, by its name in tests(). Shared things (res):
#   regress     OPFOR work\navkit\regress\<lv>.hpd and work\navkit\parity\regression_last.json: navregress rewrites
#               them in place; the apply, links, Studio, viewer and harness tests read them
#   work_hpd    every pathdata.hpd under OPFOR's work\ (hpdtool walks them all): test_vehicle_level rewrites one
#   navstudio   OPFOR work\navstudio\cache (navdata's cache files, replaced while another process may read them)
#   sp_004_b    OPFOR work\navkit\sp_004_b\detailed.json (NavKit apply writes it)
#   shaderview  the suite's build\ folder: shaderview.exe built on first use, renderqa's index and truth pickles
#   display     a window on the desktop (Tk, OpenGL, WinForms)
#   dbxtests    bfbc2-dbx-tools\tests\DbxTests.exe, compiled by every run of run_tests.bat
#   cargo       rust\target (cargo's lock; mypy walks it)
# Tests that share a thing also keep their listed order between them, so each finds it as a run one at a time did.
# Not resources, because the code takes care of them (2026-11-17): a level's terrain unpack and the meshview / dbx
# catalogue caches in work\tools_out are built under a lock between processes (atomicio.locked); the collision cache
# in %LOCALAPPDATA% is written atomically under a per-process name. Every other test writes only its own temp folder.
# mem is an estimate in GB from what the test loads (pools x bundles, whole BF1943 bundles ~0.6 GB); a run records
# each test's measured peak beside its duration.
NEEDS = {
    # quick
    'Rust core build (when stale; skipped without Rust)': need(setup=True),
    'Rust core: cargo fmt, clippy, tests (skipped without Rust)': need(cpu='all', mem=2, cargo='w'),
    "DBX reader (Rust) against the retired Python reader's recorded answers (built, damaged, 200 corpus files)": need(cpu=2, mem=1),
    "FBRB reader (Rust) against the retired Python reader's recorded answers and FbrbTool.exe's recorded unpacks (built, damaged, 8 game bundles)": need(cpu=3, mem=1),
    "Rust FBRB bundle cache against the reader without it and the Python reader's recorded answers (built, damaged, changed, 4 processes, 3 game bundles)": need(cpu=4, mem=1),
    "terrain, water and collision readers (Rust) against the retired Python readers' recorded answers (built, damaged, aliases from binary partitions)": need(),
    'Rust shader database reader / writer against shaderdb.py (built, damaged, writer fallback, merge, mp_008cq)': need(cpu=4, mem=1),
    'Rust shader database handle and queries against shaderdb.py (never made, made, changed lists; blocks, checks, writer, mp_008cq)': need(mem=0.6),
    'Rust shader translator: decoder against xenos.py, translations against the recorded Python (built programs, damaged blobs, random microcode, dispatch, hand-built trees)': need(mem=0.3),
    "cull tree element reader (Rust) against the retired Python reader's recorded answers, level search against glob (built, damaged, listing, 2 work trees)": need(cpu=2),
    'Rust Granny reader against granny.py, Xbox 360 -> PC conversion against its recorded results (built, damaged, batch, writes)': need(),
    'Rust texture converter against its recorded results (built, damaged, batch)': need(cpu=2),
    "Rust EA-XAS audio encoder / decoder against its recorded results (shift rule, seeded PCM of every shape, random frames, DICE's blocks)": need(cpu=2, mem=1),
    'Rust NavKit grid walks against their recorded results, E2a / E3c / E3h pieces against their Python (random levels, whole stages)': need(),
    "E4e: NavKit's tools in the Rust core against their Python (vehicle obstacle distance, domain searches, level scan, parity measures; built inputs)": need(cpu=2),
    # cargo test of bc2-editor: builds into rust\target (cargo's lock) and draws offscreen on the GPU (no window)
    "map editor (Rust), headless: theme from tokens.json, outliner against the Map Inspector's lists (4 levels), offscreen render, outline, picking, palette, views": need(cpu=2, mem=1, cargo='w'),
    'Rust core required: no module, a stale one, the source fingerprint': need(),
    'review regressions: HPD selection, launcher completion, mypy failures': need(),
    'BF1943 asset accuracy: texture palettes, masks, emitters, shader fetches': need(cpu=2, shaderview='w'),
    'repository rules': need(cpu=2),
    'test runner: a failure printed on any console encoding': need(),
    'test runner: tests side by side (resources, failures, exit code, output per test)': need(cpu=2),
    'lint (ruff check, every folder)': need(cpu=2),
    'types (mypy, every folder)': need(mem=1, cargo='r'),
    'core edges (terrain / water names, errors in threads)': need(),
    'paths: OPFOR, game copy, tools and sibling projects on a stand-in tree': need(),
    'converter edges (dbx numbers, dbxport references, texture check)': need(),
    'converter units (havok w lanes, MOPP byte code, L8 textures and flags, mesh rename, texcoords, mesh relayout)': need(),
    'DbxEditGUI build (when stale)': need(setup=True),
    'DbxEditGUI and the DBX classes it is built on (DbxEdit.cs): unit tests and the window': need(display='w', dbxtests='w'),
    'FbrbTool round trip, built bundle': need(),
    "DBX writer (Rust) against the retired Python writer's recorded bytes (edge, random and edited trees, 200 corpus files, DbxEdit roundtrip)": need(cpu=2, mem=1),
    "DbxEdit's commands in process against DbxEdit.exe's recorded answers (the unit tests' 82 cases, the drop-in runner, the batch, the log)": need(cpu=2),
    "FbrbTool's commands in process against FbrbTool.exe's recorded answers (228 command lines, 100 damaged bundles, .NET's gzip reader, the drop-in runner)": need(cpu=2, mem=0.5),
    "Rust FBRB packer against FbrbTool.exe's recorded bundles, zlib against Python's (built folders in every layout, 8 game bundles)": need(cpu=4, mem=1),
    "Rust FBRB unpack against FbrbTool.exe's recorded unpacks (built, odd and damaged bundles, 8 game bundles)": need(cpu=2, mem=1),
    'error handling and logs': need(),
    'cache caps and pruning (bfbc2core.caches): least recently used first, writers and readers beside it, the command': need(),
    'log caps and day rollover, project caches in work\\ (bfbc2core.caches), the command': need(),
    'every command-line tool reports errors': need(),
    'review fixes: apply inputs, stock-MP reinstall, studio flags, cache key': need(),
    'review fixes 2: harness missing files, navkit no-original reinstall, old fbrb manifests': need(),
    'deep scan fixes: atomic installs and writes, honest caches, damaged files said': need(),
    'navigation side files (navtune)': need(),
    'NavKit water rule (ground too deep under water)': need(),
    'NavKit walkability edits (navedits)': need(),
    'NavKit area link rule': need(),
    'NavKit regression progress estimates': need(),
    'NavKit boundary diagnostic (navboundary)': need(),
    'NavKit portal symmetry': need(),
    'NavKit reference extent clip': need(),
    'NavKit soldiers absent from path data': need(),
    'NavKit headroom band': need(),
    'NavKit narrow lips and door sills': need(),
    'NavKit dynamic props (floor under a prop, door leaves)': need(),
    'NavKit roof pruning': need(),
    'NavKit objective floors (M-COM)': need(),
    'NavKit 45-degree bridges: closed outlines': need(),
    'NavKit D lines and F cells': need(),
    'NavKit collision index (Havok packfile names)': need(),
    'NavKit boat mesh (navwater_mesh)': need(),
    'NavKit routes across domains (navdomains)': need(),
    'NavKit vehicle path data (navvehicle)': need(),
    'NavKit shadow islands (navislands)': need(),
    'NavKit offline sampler (navsample_offline)': need(),
    'NavKit tool edges (ordercheck, detailed cache, navdiff pictures)': need(),
    'Navigation Studio edit mode, no game (editing, overlay usage)': need(),
    'NavKit driver, stand-in OPFOR folder (navkit.py steps, install, main)': need(),
    'Navigation Studio logic, no window (studio.py)': need(),
    'Map Inspector logic, no window (inspector.py)': need(),
    'mesh viewer index and cache, stand-in bundles': need(),
    'LevelDiff build (when stale)': need(setup=True),
    'LevelDiff, built levels': need(),
    'crash reader, built reports': need(),
    'console readers, built images': need(),
    'menu tools (aptcheck, aptrefs, bc2loc), built data': need(),
    'APT writer and grafter (aptgraft), built movies': need(),
    'APT decoder (Rust) and aptgraft writer against their references (E3n; built movies, 600 damaged, 3,000 floats, grafts)': need(),
    'APT model, aptcheck and aptrefs in Rust against the Python (E4c; built movies, object rules, 300 damaged, grafts, edits, crosshair-add)': need(),
    'static cull trees (culltree), built data': need(),
    'cull tree reference rows cache and shape memo against the reference (E3m; built levels: no cache, cold, warm, changed)': need(),
    'Rust E3o: mesh reader, navdata cells and union-find, route search, texqa workers against their references, havok verify record against its recorded results (built inputs)': need(cpu=3, mem=1),
    'Rust F0 path data and runtime-mesh writer against hpdtool / kymesh (built files and meshes, 1,500 damaged, 600 changed, 4 game files)': need(mem=0.5),
    'Rust F0 localization files and BIGF archives against bc2loc / menucommon (built, damaged, 3,000 keys, the English file, the menu movies)': need(),
    'Rust F0 console containers against stfs.py / xiso.py (built packages and discs, 60 damaged packages, 30 damaged discs)': need(),
    'Rust F0 Wave resources and EA streams against audio.py (built, 3,000 damaged, 600 changed, 40 BF1943 Waves)': need(mem=0.5),
    'Rust F0 mesh headers and mesh sets against mesh.py / meshset.py (built, 3,000 damaged, 800 changed, 60 BF1943 resources)': need(mem=0.5),
    'Rust F2a console converter against terrain.py / soot.py / mesh.py / havok.py / texture.py (built, damaged, edited relayouts, 40 BF1943 resources)': need(mem=0.5),
    'Rust F2a model tools against texwrite.py / skinmesh.py (built pictures and skinned LODs, damaged, the Garand and Type 5 LODs)': need(mem=0.5),
    "Rust F2a Granny writing against granny.py (CRC, shift, orientations, add_tracks on the WW2 rifles' animations, damaged)": need(mem=0.5),
    'Rust F3 formats against aimanim / hpdtool mesh layout / audio framing / navsample / hkpack / xenos.disasm (built, damaged, changed, a few real files)': need(mem=0.5),
    'Rust F0 texture.py helpers and PC texture reader against their Python (tiled, packed, layout, levels, decode; built, damaged)': need(),
    'Rust F0 havok.classes against its Python (built and damaged executables, BFBC2Game.exe)': need(),
    'exe string / address references (exeref), built exe': need(),
    'test harness, built data': need(),
    'crash reader, exeref and test harness: the Rust core against the reference (seeded x86, built exes, logs, graphs)': need(cpu=2),
    'stock-MP AI layer, synthetic levels': need(),
    'stock-MP AI validation checks, synthetic builds (validate.py)': need(cpu=2),
    'shader databases (shaderdb), built files': need(),
    'render harness (renderqa): built mesh, shaders and texture drawn and checked': need(cpu=2, mem=2, shaderview='w'),
    'shader translator differential test: Xenos interpreter semantics, cube fetch direction': need(cpu=2, shaderview='w'),
    'HLSL compile cache and batches (pcbuild): deterministic compiler, compile_many / prefetch, disk cache': need(cpu=2),
    'shaderview -serve against one process per job (built jobs, failures, state between jobs, threads, memory)': need(cpu=2, shaderview='w'),
    'Granny Xbox 360 -> PC: DICE bone tables': need(),
    'BF1943 import pipeline, no data': need(),
    'dbxport rules: the 19 edge cases, templates, references, schema, value passes': need(),
    'dbxport ranking index against the reference scan (20,000 random rankings, Templates.choose)': need(),
    'dbxport tables in Rust against the Python reference: every entry, changed in place as OPFOR changes them': need(),
    'dbxport in Rust against its Python reference (600 random partitions, pools, catalogues, options)': need(),
    'BF1943 ports E3g: Rust mesh vertices and Havok walk against their recorded results, layout and emitter indexes against their references (built inputs)': need(),
    'F2b: mopp and havokport in Rust against their Python (seeded MOPP codes, 4.6.1 packfiles, PC resources)': need(),
    'F2b: dbxcatalogue.build in Rust against its Python (seeded games of built bundles)': need(),
    'F2b: emitteratlas build, prove and same_but_uv in Rust against their Python (40 synthetic levels)': need(),
    'F2b: menucommon.PEImage in Rust against its Python (400 built executables)': need(),
    'F2b: xmlinst in Rust against its patterns (3,000 seeded texts)': need(),
    'F2b: navtune validate, side-file text and summary in Rust against its Python (seeded operations and files)': need(),
    'F2b: mopp and havokport in Rust on every BF1943 physics file, every BFBC2 physics resource and their MOPP codes, both ways': need(cpu=2, mem=1.5),
    'F2b: dbxcatalogue.build in Rust against its Python on the game copy (84,739 partitions)': need(cpu=4, mem=1.5),
    'F2b: emitteratlas in Rust against its Python on the inputs of the four OPFOR emitter_atlas runs': need(cpu=2, mem=2),
    'F2b: menucommon.PEImage in Rust against its Python on every executable of the game copy': need(),
    'F2b: xmlinst in Rust against its patterns on every converted level XML of OPFOR work levels': need(mem=1),
    'dbxport fair ports, 7 partitions against DICE (skipped without game data)': need(mem=2),
    'emitter atlases merged, emitters re-pointed (synthetic)': need(),
    'texture QA: decoding, Xbox untiling, flips / swizzles / degenerate, baseline (synthetic)': need(),
    # game
    'shader databases: Xbox 360 (BFBC2 twins, BF1943 Wake)': need(mem=1),
    'render harness (renderqa): stock meshes with their own level\'s shaders': need(cpu=4, mem=2, shaderview='w'),
    'shader translator differential test: BF1943 shaders behind each translator fix exact': need(cpu=4, mem=1.5, shaderview='w'),
    'HLSL batches (pcbuild): every Wake pixel shader translated, compile_many against one at a time': need(cpu='all', mem=1.6),
    'shader translator differential test: every Wake pixel shader served on threads against one process each, one at a time': need(cpu=4, mem=1.5, shaderview='w'),
    "DBX reader (Rust) against the retired Python reader's recorded answers, every corpus file": need(cpu=6, mem=3),
    'Rust DBX reader against DbxEdit, every fingerprinted file; every bundle extracted (cargo)': need(cpu='all', mem=3, cargo='w'),
    "FBRB reader (Rust) against the retired Python reader's recorded answers, every bundle": need(cpu=3, mem=3),    # 3 processes, a bundle read in groups of 256 MB
    'Rust FBRB bundle cache: every 3rd game bundle read in a random order, the stored bundles': need(cpu=2, mem=1.5),
    "DBX writer (Rust) against the retired Python writer's recorded bytes, every corpus file written back and edited": need(cpu=6, mem=3),
    "DbxEdit's commands in process against DbxEdit.exe's recorded answers, 1,592 corpus files through every command and every edit fingerprint": need(cpu=4, mem=1),
    "FbrbTool's commands in process against FbrbTool.exe's recorded answers, 40 game bundles unpacked, listed and packed, 600 damaged bundles, the logged invocations' bundles": need(cpu=2, mem=1),
    "Rust FBRB packer against FbrbTool.exe's recorded bundles, every bundle of the game copy unpacked and packed again": need(cpu=4, mem=4),
    "Rust FBRB unpack against FbrbTool.exe's recorded unpacks, every game bundle with a payload up to 300 MiB": need(cpu=2, mem=2),
    "terrain, water and Havok readers (Rust) on every bundle of the game copy, the collision index of every prepared level, against the retired Python readers' recorded answers": need(cpu=3, mem=2),
    'Rust shader databases on every database of the game copy, the Xbox and BF1943 levels and work; the WW2 merges': need(cpu=8, mem=3),
    'Rust shader database queries on every database of the game copy, the Xbox and BF1943 levels and work': need(cpu=3, mem=1.5),   # 2 workers
    'Rust shader translator on every program of the BF1943 and BFBC2 Xbox shader databases (trees, HLSL, errors)': need(cpu=3, mem=1),          # 2 workers of ~360 MB
    "cull trees (Rust) on every tree and level partition of the game copy and work against the retired Python reader's recorded answers; listings; the recorded add run": need(cpu=3, mem=3),
    'Rust Granny reader and conversion on every Granny resource of your Xbox files, the game copy and BF1943': need(mem=1),
    'cull tree add and verify on Onslaught MP_005 against every work\\levels folder: reference, no cache, cold, warm (E3m)': need(cpu=2, mem=3),
    'APT decoder (Rust) and aptgraft writer against their references on every movie of the game copy, the pristine menus and the Xbox HUD copies; aptrefs / aptcheck / bc2loc both ways (E3n)': need(cpu=2, mem=1.5),
    'APT model, aptcheck and aptrefs in Rust against the Python on the 133 movies of E3n: every write, roundtrip --pack, grafts, aliases, crosshair-add (E4c)': need(cpu=2, mem=1.5),
    "Rust E3o on the game copy: every path-data sector, navdata.load of DICE's prepared levels, 400 routes, verify-havok's Onslaught files, texqa on the WW2 vehicles": need(cpu='all', mem=2),
    'Rust F0 path data on every pathdata.hpd and kept original of the game copy and work: parsed, written and every runtime mesh written both ways': need(mem=1, work_hpd='r'),
    'Rust F0 localization on every language file of the overlay bundles and the pristine copy, BIGF on every menu / HUD movie, both ways': need(),
    'Rust F0 console containers on your Xbox disc image and Update packages, 1,500 damaged packages, both ways': need(mem=0.5),
    'Rust F0 Wave resources and streams on every BF1943, BFBC2 Xbox and game-copy Wave, both ways': need(mem=1.5),
    'Rust F0 mesh headers and mesh sets on every MeshData and mesh set of BF1943, the Xbox bundles and the game copy, both ways': need(mem=1.5),
    'Rust F2a console converter on every BF1943 / Xbox terrain, soot, physics, shader database and mesh resource and the game copy (readers, decode, relayout, texture.check), both ways': need(mem=1),
    'Rust F2a model tools on every game-copy DXT texture encoded again and every skinned LOD, both ways': need(mem=1),
    'Rust F2a Granny writing on every game-copy animation (shift, orientations, add_tracks), both ways': need(mem=1),
    'Rust F3 formats on every aim animation, path-data sector, Xbox / BF1943 / PC Wave stream, sample run, packfile and Xbox shader program, both ways and against the recorded answers': need(mem=2, alone=True),
    'Rust F0 texture levels and decoding on every PC texture of the game copy, both ways': need(mem=1.5),
    'Rust F0 havok.classes on damaged copies of BFBC2Game.exe': need(),
    "level loader (Rust scene) against the retired Python loader's recorded Levels, entity by entity (SP_01, MP_005, MP_009GR, ONS_MP_004)": need(cpu=8, mem=3),
    "NavKit's collision readers (detailed, ladders, offline sampler, boat mesh) on the Rust core against the retired Python path's recorded results (SP_009, SP_004_B)": need(cpu=4, mem=2),
    'path locators: the same answers as a reference checkout (main)': need(),
    'LevelDiff on MP_005': need(),
    'crash reader code view (BFBC2Game.exe)': need(),
    'console resource converter: physics (havok)': need(mem=1),
    'console resource converter: textures': need(mem=2),
    'Rust texture converter: every BF1943 and Onslaught console texture as recorded': need(cpu='all', mem=2),
    'console mesh decoding (Xbox vs PC, BF1943)': need(mem=1.5),
    'console terrain conversion (Xbox vs PC, BF1943 Wake)': need(mem=1.5),
    'console soot meshes (Xbox vs PC, BF1943)': need(mem=1.5),
    'console sounds: EA-XMA -> EA-XAS (Xbox vs PC, BF1943)': need(cpu=2, mem=3),
    'Rust EA-XAS audio encoder: every 10th BF1943 Wave and in-memory BFBC2 Xbox Wave converted as recorded, byte for byte': need(cpu=4, mem=2),
    'mesh viewer on the game\'s models': need(mem=1.5),
    'model tools: skinned meshes, mesh sets, shapes': need(mem=1),
    'model tools: PC textures from pictures (DXT1)': need(mem=1),
    'model tools: mesh decimation (seams closed, bones apart)': need(),
    'Granny skeletons and animations (read, shift)': need(mem=1),
    'Granny Xbox 360 -> PC on your Xbox files and BF1943': need(mem=1.5),
    'BF1943 import pipeline on your BF1943 copy': need(mem=1),      # E3r: the lazy Source (0.86 GB peak; was ~2)
    'BF1943 model port (meshport)': need(mem=1.5),
    'DBX writer and BF1943 partition port (dbxport)': need(mem=2),
    'dbxport fair ports: every partition both games ship, against DICE': need(cpu=4, mem=1.5),  # E3r: DICE's read on all cores
    "dbxport ranking: the four WW2 level ports against a third of BFBC2's 157 levels, every choice and the bytes": need(mem=2.5),
    'dbxport in Rust against its Python reference: the Project OPFOR BF1943 vehicle, weapon and object builds replayed both ways (outputs to temp)': need(cpu=4, mem=2),
    'dbxport in Rust against its Python reference: every fair, counterpart and effect port both ways': need(cpu=4, mem=1.5),
    'BF1943 ports E3g: every BF1943 mesh set, physics file and effect, the Onslaught console meshes and physics, as recorded and both ways': need(cpu=4, mem=1.5),
    'BF1943 physics port, Havok 4.6.1 -> 5.5.0 (havokport)': need(mem=1.5),
    'localization files written (bc2loc set)': need(),
    'console readers on your Xbox files': need(),
    'menu tools on the game copy and OPFOR\'s patches': need(),
    'exeref on BFBC2Game.exe': need(),
    'crash reader and exeref: the Rust core against capstone on BFBC2Game.exe, both command lines byte for byte, opfor.log sessions': need(mem=1.5),
    'FbrbTool round trip, 12 game bundles': need(mem=1),
    'water, every level': need(),
    'level loader snapshot': need(cpu=4, mem=1.5),
    'terrain against in-game samples': need(mem=1),
    'path data round trip (hpdtool)': need(work_hpd='r'),
    'navigation mesh round trip (kymesh)': need(work_hpd='r'),
    'navigation data and the path test': need(),
    "path data structure, DICE's files (hpdcheck)": need(),
    'NavKit boat mesh and routes on ONS_MP_004': need(mem=1),
    'NavKit vehicle path data on ONS_MP_002': need(mem=1.5, work_hpd='w'),
    'NavKit links and route edges': need(regress='r'),
    'traversals, the apply report, installing': need(),
    'navkit apply with walkability edits': need(mem=3, regress='r', sp_004_b='w'),
    'Navigation Studio edit mode': need(cpu=4, mem=4, regress='r', navstudio='w', sp_004_b='w'),
    'the suite window (hub) and the level picker': need(display='w'),
    '3D tools open and draw (Map Inspector, Navigation Studio)': need(cpu=4, mem=3, display='w', navstudio='w', regress='r'),
    'test harness on sp_009 and sp_004_b': need(mem=1.5, regress='r'),
    'stock-MP AI layer, Atacama Conquest built': need(cpu=2, mem=1.5),
    'stock-MP AI: the Rust core against the Python on every MP level partition': need(cpu=2, mem=1),
    'Rust NavKit builds: sp_009 and sp_004_b path data as recorded': need(mem=3),
    # writes only temporary folders, and navwater's navdata cache as 'NavKit boat mesh and routes on ONS_MP_004' does
    "E4e: NavKit's tools on the game copy as recorded from their Python (ONS_MP_002 vehicle data, ONS_MP_004 boat mesh and domains, sp_009 / sp_004_b parity measures, sp_009 offline samples)": need(cpu='all', mem=2.5),
    'NavKit regression, 2 small levels': need(mem=3, regress='w'),
    # full
    'NavKit regression, all levels with DICE path data': need(mem=3, regress='w'),
    "DbxEditGUI's DBX classes (DbxEdit.cs) on the corpus: round trip, XML import identity, edited imports": need(mem=1, display='w', dbxtests='w'),
    'DBX reader against DbxEdit, every unpacked .dbx': need(mem=1),
    'FbrbTool round trip, every game bundle': need(mem=2),
    'Rust EA-XAS audio encoder: every BF1943 Wave and in-memory BFBC2 Xbox Wave converted as recorded, byte for byte': need(cpu=4, mem=2.5),
}


def safe_console():
    """Any child's output prints on any console: characters the console's code page lacks (maturin's emoji on a
    cp1252 console) are written as escapes instead of crashing the runner and hiding the failure (2026-11-16)."""
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, 'reconfigure', None)     # a TextIOWrapper's; a replaced stream may lack it
        if reconfigure is not None:
            with contextlib.suppress(ValueError):
                reconfigure(errors='backslashreplace')


# --- running a test --------------------------------------------------------------------------------------------

class _ProcessGroup:
    """A Windows job object around a test's process: its peak memory, all its children included (process pools,
    the C# tools, cargo), and a way to end them all. Does nothing elsewhere or when the job cannot be made."""

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
        self.k32.TerminateJobObject.argtypes = [ctypes.c_void_p, ctypes.c_uint]
        self.k32.CloseHandle.argtypes = [ctypes.c_void_p]
        self.handle = self.k32.CreateJobObjectW(None, None)

    def add(self, proc):
        if self.handle and not self.k32.AssignProcessToJobObject(self.handle, int(proc._handle)):
            self.close()                    # a job the runner's own job forbids: no measure, the test runs as ever

    def peak_mb(self):
        if not self.handle:
            return None
        import ctypes
        from ctypes import wintypes

        class Basic(ctypes.Structure):
            _fields_ = [('a', ctypes.c_int64), ('b', ctypes.c_int64), ('flags', wintypes.DWORD),
                        ('wmin', ctypes.c_size_t), ('wmax', ctypes.c_size_t), ('procs', wintypes.DWORD),
                        ('affinity', ctypes.c_size_t), ('prio', wintypes.DWORD), ('sched', wintypes.DWORD)]

        class Extended(ctypes.Structure):
            _fields_ = [('basic', Basic), ('io', ctypes.c_uint64 * 6), ('proc_limit', ctypes.c_size_t),
                        ('job_limit', ctypes.c_size_t), ('peak_proc', ctypes.c_size_t), ('peak_job', ctypes.c_size_t)]

        info = Extended()
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


def run_one(name, args):
    """Runs one test: (ok, its whole output, its summary line, seconds, peak memory in MB or None)."""
    t0 = time.time()
    group = _ProcessGroup()
    try:
        p = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8',
                             errors='replace', cwd=SUITE)
        group.add(p)
        RUNNING[name] = group
        try:
            stdout, stderr = p.communicate()
        except BaseException:                       # Ctrl+C: the test ends with the runner (as subprocess.run did)
            group.kill()
            p.kill()
            raise
        peak = group.peak_mb()
    finally:
        RUNNING.pop(name, None)
        group.close()
    out = (stdout + stderr).strip()
    main_out = stdout.strip() or out            # the summary line is the test's own last line, not a warning it printed
    last = main_out.splitlines()[-1] if main_out else ''
    return p.returncode == 0, out, last, time.time() - t0, peak


def result_line(ok, name, secs, last):
    return f'{"ok  " if ok else "FAIL"} {name:42s} {secs:6.1f} s   {last[:90]}'


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


def known_times(names, times_file=None):
    """(name -> (seconds, peak MB or None), where they came from): --times F, else this PC's kept durations, else
    the suite's seed (tests\\test_durations.json)."""
    if times_file:
        if times_file.lower().endswith('.json'):
            got = {n: v for n, v in load_json(times_file).items() if n in names}
            return {n: (v['s'], v.get('peak_mb')) if isinstance(v, dict) else (float(v), None)
                    for n, v in got.items()}, times_file
        return {n: (s, None) for n, s in durations_from_log(times_file, names).items()}, times_file
    out = {n: (float(s), None) for n, s in load_json(SEED_FILE).items() if n in names}
    for n, v in load_json(TIMES_FILE).items():
        if n in names and isinstance(v, dict) and 's' in v:
            out[n] = (v['s'], v.get('peak_mb'))
    return out, f'{TIMES_FILE} (else {SEED_FILE})'


def keep_times(results):
    """Keeps each passed test's duration and peak memory for the next run's order and --plan."""
    if not results:
        return
    kept = load_json(TIMES_FILE)
    day = time.strftime('%Y-%m-%d')
    for name, secs, peak in results:
        kept[name] = {'s': round(secs, 1), 'peak_mb': peak, 'when': day}
    with contextlib.suppress(OSError):
        os.makedirs(os.path.dirname(TIMES_FILE), exist_ok=True)
        tmp = f'{TIMES_FILE}.tmp-{os.getpid()}'
        with open(tmp, 'w', encoding='utf-8') as f:
            json.dump(kept, f, indent=1, ensure_ascii=False, sort_keys=True)
        os.replace(tmp, TIMES_FILE)


# --- the schedule --------------------------------------------------------------------------------------------------

class Job:
    def __init__(self, idx, name, tier, args, needs, est):
        self.idx, self.name, self.tier, self.args, self.est = idx, name, tier, args, est
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


def make_jobs(chosen, times):
    tier_of = {name: t for name, t, _ in chosen}
    return [Job(i, name, t, args, NEEDS.get(name), times.get(name, (GUESS[tier_of[name]], None))[0])
            for i, (name, t, args) in enumerate(chosen)]


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


def main(argv):
    safe_console()
    tier = next((a for a in argv[1:] if a in TIERS), 'quick')
    only = argv[argv.index('--only') + 1].lower() if '--only' in argv else None
    chosen = [x for x in tests() if TIERS.index(x[1]) <= TIERS.index(tier) and (only is None or only in x[0].lower())]
    if '--list' in argv:
        for name, t, _ in chosen:
            print(f'{t:6s} {name}')
        return 0
    threads = os.cpu_count() or 2
    slots = option(argv, ('-j', '--jobs'), int, max(1, min(8, threads // 2)))
    cpus = option(argv, ('--cpus',), int, threads)
    mem = option(argv, ('--mem',), float, round(physical_memory_gb() * 0.75, 1))
    names = {name for name, _, _ in chosen}
    times, source = known_times(names, option(argv, ('--times',), str, None))
    if '--plan' in argv:
        print_plan(tier, make_jobs(chosen, times), slots, cpus, mem, source, len(times))
        return 0
    root = opfor() if any(callable(a) or t != 'quick' for _, t, a in chosen) else None
    print(f'BFBC2 Tool Suite tests: {tier}, {len(chosen)} test(s)' + (f', OPFOR at {root}' if root else '')
          + (f', {slots} at a time' if slots > 1 else ''))
    os.environ['DBX_SKIP_BUILD'] = '1'          # run_tests.bat: DbxEditGUI is the 'DbxEditGUI build (when stale)' step's
    # the tests read bundles without the bundle cache's disk files (round E3e): a sweep of the game copy would fill
    # the user's cache with every bundle; the cache's own tests give theirs a folder of their own
    os.environ.setdefault('BFBC2_FBRB_CACHE', '0')
    if slots <= 1:
        return run_in_turn(chosen, root)
    return run_side_by_side(make_jobs(chosen, times), root, slots, cpus, mem)


def report(chosen_names, failures, start, passed_times):
    keep_times(passed_times)
    for name, out in failures:
        print(f'\n--- {name} (last 25 lines) ---')
        print('\n'.join(out.splitlines()[-25:]))
    print(f'\n{len(chosen_names) - len(failures)} of {len(chosen_names)} passed in {time.time() - start:.0f} s')
    return 1 if failures else 0


def run_in_turn(chosen, root):
    """-j 1: one test after another in the listed order (the runner as it was before 2026-11-17)."""
    failures, passed, start = [], [], time.time()
    for name, _t, args in chosen:
        if callable(args):
            args = args(root)
        ok, out, last, secs, peak = run_one(name, args)
        print(result_line(ok, name, secs, last), flush=True)
        if ok:
            passed.append((name, secs, peak))
        else:
            failures.append((name, out))
    return report([c[0] for c in chosen], failures, start, passed)


def run_side_by_side(jobs, root, slots, cpus, mem):
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
            done.put((j, *run_one(j.name, args)))
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
            results[j.name] = (ok, out)
            if ok:
                passed.append((j.name, secs, peak))
    except KeyboardInterrupt:
        for group in list(RUNNING.values()):
            group.kill()
        raise
    ordered = sorted(jobs, key=lambda j: j.idx)
    failures = [(j.name, results[j.name][1]) for j in ordered if not results[j.name][0]]
    return report([j.name for j in ordered], failures, start, passed)


if __name__ == '__main__':
    suite_paths.use('core')
    from bfbc2core import errors
    errors.setup('run-tests', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
