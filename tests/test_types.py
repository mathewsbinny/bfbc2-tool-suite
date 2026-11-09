r"""Types (quick tier): mypy, as pyproject.toml sets it up, finds nothing in the tool folders test_lint.py covers. The
fully annotated modules (pyproject.toml's override list) are checked strictly; the rest only where annotated.
Run one folder at a time (several folders have modules of the same name). Findings inside core\ are left to the
folder's own audit (see QUALITY_AUDIT.md) and do not fail this test.

  python tests\test_types.py
"""
import os
import subprocess
import sys

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
FOLDERS = ['bfbc2-auto-navkit', 'bfbc2-automated-map-and-bot-test-harness', 'bfbc2-console-readers', 'bfbc2-culltree',
           'bfbc2-diff-workbench', 'bfbc2-map-inspector', 'bfbc2-menu-tools', 'bfbc2-mesh-viewer',
           'bfbc2-navigation-studio', 'bfbc2-stockmp-ai', 'tests']


def main(argv):
    failed = 0
    for d in FOLDERS:
        r = subprocess.run([sys.executable, '-m', 'mypy', d], capture_output=True, text=True, encoding='utf-8',
                           errors='replace', cwd=SUITE)
        out = r.stdout + r.stderr
        if 'No module named mypy' in out:
            print('FAIL mypy is not installed: python -m pip install --user mypy')
            return 1
        mine = [l for l in out.splitlines() if ': error:' in l and not l.replace('\\', '/').startswith('core/')]
        failed += bool(mine)
        print(f'{"ok  " if not mine else "FAIL"} mypy {d}' + ('' if not mine else ':\n  ' + '\n  '.join(mine[:20])))
    print('all passed' if not failed else f'{failed} folder(s) with type errors')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
