r"""The map editor's headless proof (rust\bc2-editor, rounds G1 and E2; rust\bc2-levelwrite): their tests, without a
window.

  python tests\test_editor.py

  - the theme generated from design\project\tokens.json: every colour, type, spacing, radius, size and shadow token
    reaches the egui style, the dock style or the view's palette, in Dark and Light;
  - the outliner against the Python Map Inspector's lists recorded for SP_01, MP_005, MP_009GR and ONS_MP_004
    (entities, order, categories and layers, teams, positions; the level found and loaded by the editor itself);
  - the 3D view drawn offscreen: not blank, the selection outlined in the `selection` colour and only there, picking,
    a click in the running editor selecting the entity under it, the command palette opening a top view, and each
    of the four levels drawn with a flag selected and outlined;
  - round E2 (rust\bc2-levelwrite and the editor's editing): the map project's log (save / load / undo / redo,
    autosave and recovery, the base-changed report); levelwrite on MP_005, SP_01 and Guadalcanal copied to a temp
    folder (parse, exactly the expected field-level changes, writer round trips, the scene loads them; DbxEdit and the
    Python dbxbin reference byte for byte); surface placement with sink depths (the demo and MP_005's terrain); the
    gizmo's maths; the editing interface driven headless (the demo project and an MP_005 project).

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


def main() -> int:
    env = build_native.cargo_env()
    if env is None:
        print('skipped: Rust (cargo) is not installed')
        return 0
    cargo = shutil.which('cargo', path=env.get('PATH')) or 'cargo'
    r = subprocess.run([cargo, 'test', '-p', 'bc2-editor', '-p', 'bc2-levelwrite', '--', '--nocapture', '--test-threads', '1'],
                       cwd=RUST, env=env,
                       capture_output=True, text=True, encoding='utf-8', errors='replace')
    out = (r.stdout + r.stderr).splitlines()
    keep = ('test result:', 'skipped', 'as the Map Inspector lists them', 'outline pixels', 'levels compared',
            'instances,', 'levels edited and proven', 'editing through the interface', 'placement on MP_005',
            'MP_005 project')
    for line in out:
        if any(k in line for k in keep) or (line.startswith('test ') and ' ... ' in line):
            print(line.encode('ascii', errors='backslashreplace').decode('ascii'))
    if r.returncode:
        print('\n'.join(out[-60:]).encode('ascii', errors='backslashreplace').decode('ascii'))
        print('FAILED')
        return 1
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main())
