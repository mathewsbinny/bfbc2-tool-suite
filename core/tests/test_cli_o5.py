r"""Round O5's command lines in Rust (rust\bc2-cli, bfbc2.exe: the shader tools' command lines - shaderdb, renderqa,
xdiff; the BF1943 importer's; the console readers' - apt, stfs, xiso; and the Python cores O3 / O4 still handed off:
stockmp's generate / build / validate / all, detailed's sector choice, navkit status, texqa's BF1943 sources and DICE
copies, dbxcatalogue's cache build) against the Python command lines they port, as processes: the same exit codes,
the same bytes on stdout and stderr (each alone, and both on one pipe), the same files, log records and local-app-data
state (rust/BENCHMARKS.md, "O5"). The method and the machinery are round O2's (test_cli_o2.py), with round O3's
isolation (test_cli_o3.py: the cases run from an isolated copy of the checkout whenever it sits where the tools find
Project OPFOR by walking up, and nothing a run inherits changes its answers or reaches OPFOR's real work\).

  python core\tests\test_cli_o5.py            quick: every recorded command line through bfbc2.exe
  python core\tests\test_cli_o5.py --live     + the Python command lines run again beside it, both ways compared
  python core\tests\test_cli_o5.py --game     + Project OPFOR's real calls replayed read-only, both ways, timed
  python core\tests\test_cli_o5.py --record   record the goldens from the Python command lines (before the port)
     --group <group>                          one group only (default: every group with cases, cli_o5_<group>.py)
     --who / --exe / --ref                    as test_cli_o2.py (--ref default: BFBC2_O5_REF, else this checkout)
  BFBC2_CLI_WORKERS=N                         N workers (default 4)

A group module may also give game_check(check, exe, ref) for a game-tier proof of its own (stockmp's builds byte for
byte in a stand-in OPFOR folder).

Skipped (exit 0) without the binary or without the Rust core module (the Python command lines need it).
"""
from __future__ import annotations

import os
import sys
from collections.abc import Callable
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import test_cli_o2 as base  # noqa: E402
import test_cli_o3 as o3  # noqa: E402

# the groups in the order they were ported; a group whose case module is not there yet is left out
ALL_GROUPS = ('readers', 'shader', 'bf1943', 'nav', 'texqa', 'stockmp')


def _log_named(log: Callable[[Any], str]) -> Callable[[Any], str]:
    """Place.log with the names of temporary folders a tool makes itself taken out (stockmp's validate runs DbxEdit
    on files in tempfile.mkdtemp('stockmp_validate_'): its log records name the folder, random on each run)."""
    import re

    def named(self: Any) -> str:
        return re.sub(r'stockmp_validate_[a-z0-9_]+', 'stockmp_validate_N', log(self))
    return named


def main(argv: list[str]) -> int:
    import shutil
    base.ROUND = 'o5'
    base.Place.log = _log_named(base.Place.log)
    base.GROUPS = tuple(g for g in ALL_GROUPS if os.path.isfile(os.path.join(HERE, f'cli_o5_{g}.py')))
    ref = argv[argv.index('--ref') + 1] if '--ref' in argv[:-1] else os.environ.get('BFBC2_O5_REF')
    base.REF = os.path.abspath(ref or base.SUITE)
    if '--group' in argv and argv[argv.index('--group') + 1] not in base.GROUPS:
        print(f'no cases for group {argv[argv.index("--group") + 1]!r} (cli_o5_<group>.py): {", ".join(base.GROUPS)}')
        return 2
    if not base.GROUPS:
        print('skipped: no O5 case modules yet')
        return 0
    copy = None
    if o3.under_opfor(base.REF):
        if base.REF != base.SUITE:
            print(f'refused: the reference {base.REF} sits where the tools find Project OPFOR by walking up; its '
                  'cases would read and prune the real project caches. Pass a --ref outside it.')
            return 1
        copy = base.REF = o3.isolated_copy()
        print(f'     (the reference: an isolated copy of this checkout in {copy})')
    game_checks: list[tuple[str, Callable[..., Any]]] = []
    if '--game' in argv:
        groups = [argv[argv.index('--group') + 1]] if '--group' in argv else list(base.GROUPS)
        for g in groups:
            fn = getattr(base.group_module(g), 'game_check', None)
            if fn is not None:
                game_checks.append((g, fn))
    try:
        code = base.main(argv)
        exe = base.find_exe(argv)
        if game_checks and exe and os.path.isfile(exe):
            failed = 0

            def check(ok: bool, what: str, detail: str = '') -> None:
                nonlocal failed
                failed += not ok
                print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''),
                      flush=True)
            for _g, fn in game_checks:
                fn(check, exe, base.REF)
            code = code or (1 if failed else 0)
        return code
    finally:
        if copy:
            shutil.rmtree(copy, ignore_errors=True)


if __name__ == '__main__':
    sys.exit(main(sys.argv))
