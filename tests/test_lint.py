r"""Lint (quick tier): ruff's checks from pyproject.toml (pyflakes, pycodestyle errors and warnings, bugbear) find
nothing in the tool folders it covers. The folders under heavy work in November 2026 (core, bf1943-import,
console-convert, shader-tools, anim-tools, model-tools) are not covered yet: QUALITY_AUDIT.md lists their findings.

  python tests\test_lint.py
"""
import os
import subprocess
import sys

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
COVERED = ['bfbc2-auto-navkit', 'bfbc2-automated-map-and-bot-test-harness', 'bfbc2-console-readers', 'bfbc2-culltree',
           'bfbc2-diff-workbench', 'bfbc2-map-inspector', 'bfbc2-menu-tools', 'bfbc2-mesh-viewer',
           'bfbc2-navigation-studio', 'bfbc2-stockmp-ai', 'tests', 'run_tests.py', 'BFBC2 Tool Suite.pyw',
           os.path.join('bfbc2-auto-navkit', 'navkitGUI.pyw')]


def main(argv):
    try:
        r = subprocess.run([sys.executable, '-m', 'ruff', 'check', '--output-format', 'concise', *COVERED],
                           capture_output=True, text=True, encoding='utf-8', errors='replace', cwd=SUITE)
    except OSError as e:
        print(f'FAIL ruff could not run: {e}')
        return 1
    out = (r.stdout + r.stderr).strip()
    if 'No module named ruff' in out:
        print('FAIL ruff is not installed: python -m pip install --user ruff')
        return 1
    if r.returncode != 0:
        print(out)
        print(f'FAIL ruff check found problems in the covered folders (exit {r.returncode})')
        return 1
    print(f'ok   ruff check: nothing found in {len(COVERED)} folders and files')
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
