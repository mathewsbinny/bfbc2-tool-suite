r"""Types (quick tier): mypy, as pyproject.toml sets it up, finds nothing in any of the suite's Python folders. The
fully annotated modules (pyproject.toml's override list) are checked strictly. Run one folder at a time (several
folders have modules of the same name).

  python tests\test_types.py
"""
import os
import subprocess
import sys

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
FOLDERS = ['bfbc2-auto-navkit', 'bfbc2-automated-map-and-bot-test-harness', 'bfbc2-console-readers', 'bfbc2-culltree',
           'bfbc2-diff-workbench', 'bfbc2-map-inspector', 'bfbc2-menu-tools', 'bfbc2-mesh-viewer',
           'bfbc2-navigation-studio', 'bfbc2-stockmp-ai', 'tests', 'core', 'bfbc2-bf1943-import',
           'bfbc2-console-convert', 'bfbc2-shader-tools', 'bfbc2-anim-tools', 'bfbc2-model-tools']


def main(argv: list[str]) -> int:
    failed = 0
    for d in FOLDERS:
        r = subprocess.run([sys.executable, '-m', 'mypy', d], capture_output=True, text=True, encoding='utf-8',
                           errors='replace', cwd=SUITE)
        out = r.stdout + r.stderr
        if 'No module named mypy' in out:
            print('FAIL mypy is not installed: python -m pip install --user mypy')
            return 1
        mine = [l for l in out.splitlines() if ': error:' in l]
        bad = r.returncode != 0 or bool(mine)
        failed += bad
        details = mine or ([out.strip() or f'mypy exited with code {r.returncode}'] if bad else [])
        print(f'{"FAIL" if bad else "ok  "} mypy {d}' + ('' if not details else ':\n  ' + '\n  '.join(details[:20])))
    print('all passed' if not failed else f'{failed} folder(s) with type errors')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
