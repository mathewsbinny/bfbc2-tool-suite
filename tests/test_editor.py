r"""The map editor's headless proof (rust\bc2-editor, rounds G1 and E2; rust\bc2-levelwrite): their tests, without a
window.

  python tests\test_editor.py

Not in run_tests.py's tiers since round QW1 (audit AU2 P2): the quick tier's "Rust core: cargo fmt, clippy, tests"
(tests\test_rust.py) runs these crates' tests in its workspace run and prints the same evidence lines (KEEP). This
script stays for a run of the three crates by hand, one test at a time (--test-threads 1) with their output.
Each crate's integration tests are one test binary (round QW1): --test editor / levelwrite / render, a file's tests
by its module name (`cargo test -p bc2-editor --test editor g2::`).

  - the theme generated from design\project\tokens.json: every colour, type, spacing, radius, size and shadow token
    reaches the egui style, the dock style or the view's palette, in Dark and Light;
  - the outliner against the Python Map Inspector's lists recorded for SP_01, MP_005, MP_009GR and ONS_MP_004
    (entities, order, categories and layers, teams, positions; the level found and loaded by the editor itself);
  - the 3D view drawn offscreen: not blank, the selection outlined in the `selection` colour and only there, picking,
    a click in the running editor selecting the entity under it, the command palette opening a top view, and each
    of the four levels drawn with a flag selected and outlined;
  - round G2: the view-* and scene-* tokens found in rendered frames and the overlays' contrast measured on them,
    the panel contrast and the third neutral's hue distance, team labels by faction on BFBC2, Vietnam and WW2 levels
    (read only), Help generated from the action registry, and opening a level that needs preparing (a stand-in
    script; the real preparation only with BC2_EDITOR_PREPARE=<level>, on a stand-in OPFOR folder in temp);
  - round E2 (rust\bc2-levelwrite and the editor's editing): the map project's log (save / load / undo / redo,
    autosave and recovery, the base-changed report); levelwrite on MP_005, SP_01 and Guadalcanal copied to a temp
    folder (parse, exactly the expected field-level changes, writer round trips, the scene loads them; DbxEdit and the
    Python dbxbin reference byte for byte); surface placement with sink depths (the demo and MP_005's terrain); the
    gizmo's maths; the editing interface driven headless (the demo project and an MP_005 project).
  - round R5 (rust\bc2-render): the game-accurate view - MP_005's warehouse drawn as the C++ render harness draws
    it (its recorded picture's coverage and colour), and the editor's Game-accurate switch on MP_005 (Direct3D 11).
  - round E2b: the context menus per workspace, Alt+wheel sinking (one operation per burst), placing from the picker
    and the content browser, deep duplicate, the view modes, polygon selection and Mixed fields, driven headless on
    a made-up level with a partition; levelwrite's scripted log on every prepared level (work\levels and every
    game\Package level, in memory), exactly the expected changes, replayed to the same bytes.
  - round GR1: the Map Inspector's views folded in - the property tree, references and referrers against what the Map
    Inspector showed (1,074 objects of four levels, its answers kept as digests), the Links panel (followed and back),
    All fields, the drawn links and Measure, on the demo level (viewed and as a project) and MP_005 in both themes.
  - round G4: the start flow (Open project, Edit existing level, Create level) driven through its buttons on a
    made-up game copy - no campaign level under Edit existing level, a WW2 map opened, a project from a campaign
    level with its kind recorded, a project opened from the recent list with the recovery offer, the height-map
    form's checks and its saved project - and the top bar's name never under its buttons at 1500 px.
  - round GR2: the Navigation Studio folded into the AI workspace - NavKit's and DICE's navigation data, the five
    colourings, the spawns' islands and the islands' gaps, the anomaly overlay, path tests and the compare overlay
    against what the Studio showed (SP_01, ONS_MP_004, ONS_MP_002, SP_009 and the WW2 level GUADAL_CANAL, its answers
    kept as digests); the AI workspace on ONS_MP_004 in Dark and Light; edit, build (navkit apply), install and
    restore on a scratch OPFOR copy, its game folder byte for byte as before.
  - round EX1: every name of an export checked together and landing where the game and OPFOR read it (an E3d
    canary renamed: the registration, the description, each language's text in the overlay; the rules' refusals in
    words), the Export window's Names section, the export's preview beside the original (Atacama Rush as Conquest)
    and the Diff panel on Aurora made WW2 Conquest (rows, filters, tints, a row clicked), in Dark and Light.

Read only for the game copy and OPFOR's work folder: projects and edited partitions go to temp folders. The
game-data parts are skipped (with a note) without the game copy, the GPU parts without any graphics adapter; the
whole is skipped (exit 0) without Rust. BC2_EDITOR_SHOTS=<folder> saves the screenshots there.
"""
import os
import shutil
import subprocess
import sys

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('rust')
import build_native  # noqa: E402

RUST = os.path.join(SUITE, 'rust')
# the editor tests' evidence lines (tests\test_rust.py prints them from the workspace run too)
KEEP = ('as the Map Inspector lists them', 'outline pixels', 'levels compared', 'levels in this game copy', 'median',
        'instances,', 'levels edited and proven', 'editing through the interface', 'placement on MP_005',
        'MP_005 project', 'draws at', 'loaded in', 'e2b through the interface', 'levels, ', 'g4 ',
        'as the Map Inspector showed them', 'compared with the Map Inspector', 'gr1 ', 'as the Navigation Studio showed them',
        'compared with the Navigation Studio', 'gr2 ', 'the same AI layer', 'WW2', 'ex1 ')


def main() -> int:
    env = build_native.cargo_env()
    if env is None:
        print('skipped: Rust (cargo) is not installed')
        return 0
    cargo = shutil.which('cargo', path=env.get('PATH')) or 'cargo'
    r = subprocess.run([cargo, 'test', '-p', 'bc2-editor', '-p', 'bc2-levelwrite', '-p', 'bc2-render', '--',
                        '--nocapture', '--test-threads', '1'],
                       cwd=RUST, env=env,
                       capture_output=True, text=True, encoding='utf-8', errors='replace')
    out = (r.stdout + r.stderr).splitlines()
    for line in out:
        if any(k in line for k in ('test result:', 'skipped', *KEEP)) or (line.startswith('test ') and ' ... ' in line):
            print(line.encode('ascii', errors='backslashreplace').decode('ascii'))
    if r.returncode:
        print('\n'.join(out[-60:]).encode('ascii', errors='backslashreplace').decode('ascii'))
        print('FAILED')
        return 1
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main())
