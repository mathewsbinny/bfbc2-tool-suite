r"""Project OPFOR's BF1943 builds replayed with dbxport.port run both ways (round E4d's fix; game tier).

Runs OPFOR's bf1943_vehicles, bf1943_weapons and bf1943_objects (--wake) builds - read only: OPFOR is never written,
their outputs go to a temporary folder - against this checkout's suite (OPFOR_TOOLKIT: a temporary junction to it),
with BFBC2_DBXPORT_DUAL: every dbxport.port call they make runs in Rust and in Python, and a difference in the bytes,
report lines, records or exception fails the test (DualMismatch). The builds change dbxport's tables around their
ports (bf1943_vehicles: the planes' gear input in ENUM_NUMBERS) - what the first Rust port missed (the Zero's and
the Corsair's ChildRotationBodyData.RotationInput 16 as EiaUndefined, not EiaDynamicGadget2).

  python test_dbxport_opfor.py [vehicles] [weapons] [objects] [--out DIR]     (none: all three; --out: the builds'
                                                                                outputs kept in DIR/<build>)

Skipped without Project OPFOR (its src\tools\ww2) or the game data.
"""
import builtins
import io
import os
import shutil
import sys
import tempfile
import time
import traceback
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'mesh-viewer', 'shader-tools', 'console-convert', 'model-tools', 'anim-tools',
                'core')
import dbxport  # noqa: E402
import shaderdb  # noqa: E402

BUILDS = {'vehicles': ('bf1943_vehicles', []), 'weapons': ('bf1943_weapons', []),
          'objects': ('bf1943_objects', ['--wake'])}


class Guard:
    """Refuses every write, rename or removal under a folder (OPFOR's), for the duration."""

    def __init__(self, root: str) -> None:
        self.root = os.path.normcase(os.path.realpath(root)) + os.sep
        self.saved: list[tuple[Any, str, Any]] = []
        self.refused: list[str] = []

    def under(self, p: Any) -> bool:
        try:
            return os.path.normcase(os.path.realpath(os.fspath(p))).startswith(self.root)
        except (TypeError, ValueError):
            return False

    def refuse(self, what: str, p: Any) -> None:
        self.refused.append(f'{what} {p}')
        raise PermissionError(f'test_dbxport_opfor: {what} under Project OPFOR refused: {p}')

    def wrap(self, owner: Any, name: str, check: Any) -> None:
        real = getattr(owner, name)
        self.saved.append((owner, name, real))

        def guarded(*a: Any, **kw: Any) -> Any:
            check(a, kw)
            return real(*a, **kw)
        setattr(owner, name, guarded)

    def __enter__(self) -> 'Guard':
        def opening(a: tuple, kw: dict) -> None:
            mode = kw.get('mode', a[1] if len(a) > 1 else 'r')
            if isinstance(mode, str) and any(c in mode for c in 'wax+') and a and self.under(a[0]):
                self.refuse(f'open({mode!r})', a[0])

        def first(what: str) -> Any:
            def check(a: tuple, kw: dict) -> None:
                if a and self.under(a[0]):
                    self.refuse(what, a[0])
            return check

        def second(what: str) -> Any:
            def check(a: tuple, kw: dict) -> None:
                if (a and self.under(a[0]) and what == 'move') or (len(a) > 1 and self.under(a[1])):
                    self.refuse(what, a[1] if len(a) > 1 else a[0])
            return check
        self.wrap(builtins, 'open', opening)
        self.wrap(io, 'open', opening)
        for name in ('makedirs', 'mkdir', 'remove', 'unlink', 'rmdir'):
            self.wrap(os, name, first(name))
        for name in ('replace', 'rename'):
            self.wrap(os, name, lambda a, kw, n=name: (first(n)(a, kw), second(n)(a, kw)))
        self.wrap(shutil, 'rmtree', first('rmtree'))
        for name in ('copy', 'copy2', 'copyfile', 'copytree', 'move'):
            self.wrap(shutil, name, second(name))
        return self

    def __exit__(self, *exc: Any) -> None:
        for owner, name, real in reversed(self.saved):
            setattr(owner, name, real)


def main(argv: list[str]) -> int:
    want = [a for a in argv[1:] if a in BUILDS] or list(BUILDS)
    keep = argv[argv.index('--out') + 1] if '--out' in argv else None     # the outputs kept there (not under OPFOR)
    try:
        opfor = shaderdb.find_opfor(None)
    except Exception as e:                                      # noqa: BLE001 - only missing data skips
        print(f'SKIP: no Project OPFOR ({type(e).__name__}: {e})')
        print('all passed')
        return 0
    ww2 = os.path.join(opfor, 'src', 'tools', 'ww2')
    if not os.path.isdir(ww2) or dbxport._native() is None:
        print(f'SKIP: no {ww2}' if not os.path.isdir(ww2) else 'SKIP: no Rust core built (nothing to compare)')
        print('all passed')
        return 0
    # the suite modules OPFOR's builds import, loaded from this checkout first (they find BF1943's copy and the
    # BF1943 suite beside the suite they were loaded from: here, not the junction below)
    import bf1943import
    import dbxcatalogue  # noqa: F401 - loaded here for OPFOR's import to reuse
    import effectport  # noqa: F401
    import meshview  # noqa: F401
    try:
        os.environ.setdefault('BF1943_XENON', bf1943import.default_source())
        bf1943import.fbrb1943()
    except Exception as e:                                      # noqa: BLE001 - only missing data skips
        print(f'SKIP: no BF1943 copy ({type(e).__name__}: {e})')
        print('all passed')
        return 0
    import _winapi
    tmp = tempfile.mkdtemp(prefix='dbxport_opfor_')
    toolkit = os.path.join(tmp, 'toolkit')
    os.makedirs(toolkit)
    junction = os.path.join(toolkit, 'bfbc2-tool-suite')
    _winapi.CreateJunction(_ROOT, junction)                     # OPFOR's tools find this checkout as the suite
    os.environ['OPFOR_TOOLKIT'] = toolkit
    fails: list[str] = []
    try:
        sys.path.insert(0, ww2)
        # --rust: the Rust port alone (the outputs its own, to compare a kept --out with a reference run)
        dbxport.DUAL = '--rust' not in argv
        with Guard(opfor) as guard:
            for key in want:
                module, args = BUILDS[key]
                out = os.path.join(keep or os.path.join(tmp, 'out'), key)
                before = dict(dbxport.STATS, why=dict(dbxport.STATS['why']))
                t = time.perf_counter()
                try:
                    m = __import__(module)
                    import attachments
                    import weapons
                    for mod in (m, attachments, weapons):
                        vars(mod)['OUT'] = out if mod is m else os.path.join(tmp, 'out', f'{key}_{mod.__name__}')
                    vars(attachments)['PREVIEW'] = os.path.join(tmp, 'preview')
                    rc = m.main([module] + args)
                    err = None if not rc else f'the build returned {rc}'
                except dbxport.DualMismatch as e:
                    err = f'Rust and Python differ: {e}'
                except Exception as e:                          # noqa: BLE001 - reported as the failure
                    err = f'{type(e).__name__}: {e}\n' + ''.join(traceback.format_exc(limit=6))
                compared = dbxport.STATS['dual' if dbxport.DUAL else 'rust'] - before['dual' if dbxport.DUAL else 'rust']
                left = dbxport.STATS['fallback'] - before['fallback']
                why = {k: v - before['why'].get(k, 0) for k, v in dbxport.STATS['why'].items()
                       if v != before['why'].get(k, 0)}
                ok = err is None and compared > 0
                how = ('the same both ways (bytes, report lines, records, exceptions)' if dbxport.DUAL else
                       'ported in Rust (--rust: not compared)')
                print(f'{"ok  " if ok else "FAIL"} {module}: {compared} dbxport.port calls {how}, {left} left to Python '
                      f'{why or ""} '
                      f'({time.perf_counter() - t:.0f} s)' + (f'\n     {err}' if err else ''), flush=True)
                if not ok:
                    fails.append(module)
            if guard.refused:
                print(f'FAIL writes under Project OPFOR refused: {guard.refused[:5]}')
                fails.append('writes')
    finally:
        os.rmdir(junction)                                      # the junction only (never what it points to)
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not fails else f'{len(fails)} failed')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
