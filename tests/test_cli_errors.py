r"""Every command-line tool's error handling (no game data): each is run with input that must fail - a file that is
missing, or not the kind the tool reads - and must exit with an error code, tell the cause and what to do in
words, name its log, and record the error in that log (%LOCALAPPDATA%\bfbc2-tool-suite\logs; a temporary folder
here). bfbc2.exe leveldiff (LevelDiff.exe's port, round EX1; when the binary is built), FbrbTool's and DbxEdit's command
lines (fbrbtool.py / dbxedit.py: the Rust core) and the Python ones (NavKit's, the crash reader, the console readers, the test harness) alike.

  python tests\test_cli_errors.py
"""
import glob
import os
import shutil
import subprocess
import sys
import tempfile

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
PY = sys.executable


def main(argv: list[str]) -> int:
    failed = 0

    def check(ok: bool, what: str) -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}', flush=True)

    sys.path.insert(0, os.path.join(SUITE, 'core'))
    from bfbc2core import native
    bfbc2 = native.find_binary('bfbc2')
    tmp = tempfile.mkdtemp(prefix='cli_errors_')
    env = {**os.environ, 'LOCALAPPDATA': tmp}
    logs = os.path.join(tmp, 'bfbc2-tool-suite', 'logs')
    junk = os.path.join(tmp, 'junk.bin')
    open(junk, 'wb').write(b'this is not any kind of game file')
    missing = os.path.join(tmp, 'missing.file')
    notrun = os.path.join(tmp, 'notrun.json')
    open(notrun, 'w').write('{"format": "something else"}')
    cases = [
        ('FbrbTool, a file that is not a bundle', [PY, os.path.join(SUITE, 'bfbc2-fbrb-tool', 'fbrbtool.py'), 'list', junk], 'fbrbtool', 'not the kind the tool reads'),
        ('FbrbTool, a missing bundle', [PY, os.path.join(SUITE, 'bfbc2-fbrb-tool', 'fbrbtool.py'), 'list', missing], 'fbrbtool', 'missing'),
        ('DbxEdit, a missing file', [PY, os.path.join(SUITE, 'bfbc2-dbx-tools', 'dbxedit.py'), 'edit', missing, junk + '.out', 'drop=x'], 'dbxedit', 'missing'),
        ('hpdtool, a file that is not path data', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdtool.py'), 'info', junk], 'hpdtool', 'what to do'),
        ('STFS reader, a missing package', [PY, os.path.join(SUITE, 'bfbc2-console-readers', 'stfs', 'stfs.py'), 'info', missing], 'stfs', 'missing'),
        ('aptcheck, a file that is not a menu movie', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'aptcheck.py'), 'check', junk], 'aptcheck', 'not a menu movie'),
        ('bc2loc, a file that is not a language file', [PY, os.path.join(SUITE, 'bfbc2-menu-tools', 'bc2loc.py'), 'get', 'X', '--file', junk], 'bc2loc', 'not a localization'),
        ('exeref, a missing exe', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'exeref.py'), 'str', 'x', '--exe', missing], 'exeref', 'missing'),
        ('navdiff, missing files', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'navdiff.py'), missing, missing], 'navdiff', 'missing'),
        ('navboundary, a missing NavKit file', [PY, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'navboundary.py'), 'sp_004_b', missing], 'navboundary', 'what to do'),
        ('test harness, a missing saved run', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'harness.py'), 'show', missing + '.json'], 'harness', 'missing'),
        ('test harness, a file that is not a run', [PY, os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'harness.py'), 'compare', notrun, notrun], 'harness', 'what to do'),
        ('test runner, --only without its value (a usage error since round RT3)', [PY, os.path.join(SUITE, 'run_tests.py'), 'quick', '--only'], 'run-tests', 'what to do'),
        ('BF1943 import, a missing source folder', [PY, os.path.join(SUITE, 'bfbc2-bf1943-import', 'bf1943import.py'), 'list', '--source', missing], 'bf1943import', 'what to do'),
        ('texture QA, a missing build folder', [PY, os.path.join(SUITE, 'bfbc2-console-convert', 'texqa.py'), missing, '--no-dice', '--no-bf1943'], 'texqa', 'what to do'),
    ]
    if bfbc2:                                   # LevelDiff.exe's port (round EX1): the binary only
        cases.insert(2, ('leveldiff, a missing file', [bfbc2, 'leveldiff', missing, junk], 'leveldiff', 'what to do'))
        # round RT3: the crash reader's and aptrefs' Python command lines were retired: the binary only
        cases.append(('crash reader, a missing report', [bfbc2, 'crashreader', missing, '--no-code'], 'crashreader',
                      'missing'))
        cases.append(('aptrefs, an exe that is not one', [bfbc2, 'aptrefs', 'table', '--exe', junk], 'aptrefs',
                      'not a windows program'))
    else:
        print('skip leveldiff, crashreader, aptrefs: bin\\bfbc2.exe is not built (python rust\\build_native.py)')
    try:
        for label, args, log_name, says in cases:
            r = subprocess.run(args, capture_output=True, text=True, errors='replace', env=env, timeout=300)
            out = r.stdout + r.stderr
            log = sorted(glob.glob(os.path.join(logs, f'{log_name}-*.log')))
            text = open(log[-1], encoding='utf-8', errors='replace').read() if log else ''
            ok = r.returncode != 0 and says in out.lower() and 'details' in out.lower() and 'ERROR' in text
            check(ok, f'{label}: exit {r.returncode}, the cause in words, the log ({os.path.basename(log[-1]) if log else "none"})')
            if not ok:
                print('       ' + out.strip().replace('\n', '\n       ')[:600])
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
