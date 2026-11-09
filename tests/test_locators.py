r"""Every path locator gives the same answer as in a reference checkout of the suite (game tier: needs this machine's
Project OPFOR BFBC2 and a second checkout; written for step M1, bfbc2core.paths).

The reference is BFBC2_SUITE_REFERENCE, else the "bfbc2-tool-suite" folder beside this suite (the main checkout
when this one is a worktree beside it). Each suite is asked in its own process (python -I -B: nothing is written
into the reference, not even bytecode), importing each tool module from its own folder. Three runs per suite:
- plain:  the machine as it is (OPFOR_ROOT and BF1943_XENON unset);
- env:    OPFOR_ROOT set to Project OPFOR BFBC2 spelled with "work\.." (it must pass through as each locator passed
          it), BF1943_XENON set;
- hide:   every project-opfor-bfbc2 and bf1943-tool-suite folder made invisible (os.path patched in that process):
          each locator's failure - exception type, message, errors.explain's cause.
Answers are compared exactly, with the suite's own folder written <SUITE>. Known, intended differences:
- convert.default_exe spells the folder beside the suite without "..\.." now (the same file);
- pipeline.Paths and level.load raise bfbc2core.paths.GameCopyNotFound instead of navkit.NavKitError (the same
  message; they no longer import NavKit to find the folder);
- detailed.py is compared in the plain and env runs only: with no OPFOR it used a fixed path, now it searches.
Nothing is written to the game copy, and the game is not started.

  python tests\test_locators.py [--reference <suite>]
"""
import importlib
import json
import os
import subprocess
import sys
from collections.abc import Callable
from typing import Any

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HIDDEN = ('project-opfor-bfbc2', 'bf1943-tool-suite')
OLD_DETAILED = r'C:\Programming\Projects\project-opfor-bfbc2'      # detailed.py's fixed path before M1
SPELLING_ONLY = {'convert.default_exe'}                              # the same file, spelled differently
TYPE_CHANGED = {'pipeline.Paths': ('NavKitError', 'GameCopyNotFound'), 'level.load': ('NavKitError', 'GameCopyNotFound')}


# ------------------------------------------------------------------------------------- inside the probed suite
def probe(suite: str, mode: str) -> dict[str, Any]:
    def imp(folder: str, name: str) -> Any:
        sys.path.insert(0, os.path.join(suite, folder))
        return importlib.import_module(name)

    navkit = imp('bfbc2-auto-navkit', 'navkit')
    menucommon = imp('bfbc2-menu-tools', 'menucommon')
    shaderdb = imp('bfbc2-shader-tools', 'shaderdb')
    navparity = imp(os.path.join('bfbc2-auto-navkit', 'tools'), 'navparity')
    navvehicle = imp(os.path.join('bfbc2-auto-navkit', 'tools'), 'navvehicle')
    detailed = imp(os.path.join('bfbc2-auto-navkit', 'tools'), 'detailed')
    harness = imp('bfbc2-automated-map-and-bot-test-harness', 'harness')
    crashreader = imp(os.path.join('bfbc2-automated-map-and-bot-test-harness', 'crash-reader'), 'crashreader')
    convert = imp('bfbc2-console-convert', 'convert')
    bf1943import = imp('bfbc2-bf1943-import', 'bf1943import')
    pipeline = imp('bfbc2-stockmp-ai', 'pipeline')
    level = imp('core', 'bfbc2core.level')
    errors = importlib.import_module('bfbc2core.errors')

    calls: list[tuple[str, Callable[[], Any]]] = [
        ('navkit.find_opfor()', lambda: navkit.find_opfor(None)),
        ('navkit.find_opfor(given)', lambda: navkit.find_opfor(os.path.join(os.path.dirname(suite), 'project-opfor-bfbc2'))),
        ('navkit.find_tool(missing)', lambda: navkit.find_tool(suite, 'NoSuchTool.exe', 'bfbc2-fbrb-tool')),
        ('navkit.Project().game', lambda: navkit.Project().game),
        ('menucommon.find_opfor', lambda: menucommon.find_opfor()),
        ('menucommon.find_exe', lambda: menucommon.find_exe()),
        ('shaderdb.find_opfor', lambda: shaderdb.find_opfor(None)),
        ('navparity.opfor_root', lambda: navparity.opfor_root()),
        ('navvehicle.opfor_root', lambda: navvehicle.opfor_root()),
        ('harness.opfor', lambda: harness.opfor(None)),
        ('crashreader.find_exe', lambda: crashreader.find_exe()),
        ('convert.default_exe', lambda: convert.default_exe()),
        ('bf1943import.default_source', lambda: bf1943import.default_source()),
        ('bf1943import.fbrb1943', lambda: os.path.dirname(bf1943import.fbrb1943().__file__)),
        ('pipeline.FBRB', lambda: pipeline.FBRB),
        ('pipeline.DBX', lambda: pipeline.DBX),
        ('pipeline.Paths', lambda: pipeline.Paths('sp_009').opfor),
    ]
    if mode != 'hide':
        calls.append(('detailed', lambda: detailed.opfor_root() if hasattr(detailed, 'opfor_root')
                      else os.environ.get('OPFOR_ROOT') or OLD_DETAILED))
    else:
        calls.append(('level.load', lambda: level.load('sp_009')))      # fails at the search, before anything else
        isdir, isfile, exists = os.path.isdir, os.path.isfile, os.path.exists

        def hide(f: Callable[[Any], bool]) -> Callable[[Any], bool]:
            return lambda p: not any(h in os.path.normcase(os.fspath(p)) for h in HIDDEN) and f(p)
        os.path.isdir, os.path.isfile, os.path.exists = hide(isdir), hide(isfile), hide(exists)  # type: ignore[assignment]

    def rel(v: Any) -> Any:
        if not isinstance(v, str):
            return v
        i = v.lower().find(suite.lower())
        while i >= 0:
            v = v[:i] + '<SUITE>' + v[i + len(suite):]
            i = v.lower().find(suite.lower())
        return v

    out: dict[str, Any] = {}
    for name, fn in calls:
        try:
            out[name] = {'value': rel(fn())}
        except Exception as e:
            out[name] = {'error': type(e).__name__, 'message': rel(str(e)), 'cause': errors.explain(e)[0]}
    return out


# ------------------------------------------------------------------------------------------------- the test
def ask(suite: str, mode: str) -> dict[str, Any]:
    env = {k: v for k, v in os.environ.items() if k not in ('OPFOR_ROOT', 'BF1943_XENON', 'BF1943_SUITE_FBRB')}
    if mode == 'env':
        env['OPFOR_ROOT'] = os.path.join(os.path.dirname(suite), 'project-opfor-bfbc2', 'work', '..')
        env['BF1943_XENON'] = r'X:\bf1943\dist\xenon'
    r = subprocess.run([sys.executable, '-I', '-B', os.path.abspath(__file__), '--probe', suite, mode],
                       capture_output=True, text=True, env=env, cwd=os.path.dirname(suite))
    lines = [ln for ln in r.stdout.splitlines() if ln.startswith('{')]
    if r.returncode or not lines:
        raise RuntimeError(f'probe of {suite} ({mode}) failed: {r.stdout[-1500:]}\n{r.stderr[-1500:]}')
    return json.loads(lines[-1])


def real(v: str, suite: str) -> str:
    """A probed path with <SUITE> put back, normalised: what file it names."""
    return os.path.normcase(os.path.normpath(v.replace('<SUITE>', suite)))


def main(argv: list[str]) -> int:
    if '--probe' in argv:
        i = argv.index('--probe')
        print(json.dumps(probe(argv[i + 1], argv[i + 2])))
        return 0
    ref = (argv[argv.index('--reference') + 1] if '--reference' in argv else
           os.environ.get('BFBC2_SUITE_REFERENCE') or os.path.join(os.path.dirname(SUITE), 'bfbc2-tool-suite'))
    ref = os.path.abspath(ref)
    if os.path.normcase(ref) == os.path.normcase(SUITE) or not os.path.isdir(os.path.join(ref, 'core')):
        print(f'skip: no second checkout to compare with ({ref}); set BFBC2_SUITE_REFERENCE')
        return 0
    if not os.path.isdir(os.path.join(os.path.dirname(SUITE), 'project-opfor-bfbc2', 'game')):
        print('skip: no Project OPFOR BFBC2 game copy beside the suite')
        return 0
    print(f'reference: {ref}')
    failed = 0
    for mode in ('plain', 'env', 'hide'):
        new, old = ask(SUITE, mode), ask(ref, mode)
        for name in sorted(set(new) | set(old)):
            a, b = new.get(name), old.get(name)
            if a == b:
                print(f'ok   {mode:5s} {name}: {next(iter(a.values())) if a else a}')
                continue
            if (a and b and name in SPELLING_ONLY and 'value' in a and 'value' in b
                    and real(a['value'], SUITE) == real(b['value'], ref)):
                print(f'ok   {mode:5s} {name}: the same path, spelled {a["value"]} (was {b["value"]})')
                continue
            if (a and b and name in TYPE_CHANGED and (b.get('error'), a.get('error')) == TYPE_CHANGED[name]
                    and a['message'] == b['message'] and a['cause'] == b['cause']):
                print(f'ok   {mode:5s} {name}: {a["error"]} (was {b["error"]}), the same message and cause')
                continue
            failed += 1
            print(f'FAIL {mode:5s} {name}\n       now: {a}\n       was: {b}')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
