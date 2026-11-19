r"""Round O3's command lines in Rust (rust\bc2-cli, bfbc2.exe: caches, stockmp, aptgraft, audio, dbxcatalogue,
dbxparity, portparity, meshview, culltree) against the Python command lines they port, as processes: the same exit
codes, the same bytes on stdout and stderr (each alone, and both on one pipe), the same files, log records and
local-app-data state (logs left, archives of pruned logs) (rust/BENCHMARKS.md, "O3"). The method and the machinery
are round O2's (test_cli_o2.py), with the cases in cli_o3_<group>.py and the goldens in cli_o3_<group>_golden*.json.

  python core\tests\test_cli_o3.py            quick: every recorded command line through bfbc2.exe
  python core\tests\test_cli_o3.py --live     + the Python command lines run again beside it, both ways compared
  python core\tests\test_cli_o3.py --game     + Project OPFOR's real calls replayed read-only, both ways, timed
  python core\tests\test_cli_o3.py --record   record the goldens from the Python command lines (before the port)
     --group caches|stockmp|graft|audio|bf1943|meshview|cull   one group only (default: every group with cases)
     --who / --exe / --ref                    as test_cli_o2.py (--ref default: BFBC2_O3_REF, else this checkout)
  BFBC2_CLI_WORKERS=N                         N workers (default 4)

Skipped (exit 0) without the binary or without the Rust core module (the Python command lines need it).
"""
from __future__ import annotations

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import test_cli_o2 as base  # noqa: E402

# the groups in the order they were ported; a group whose case module is not there yet is left out
ALL_GROUPS = ('caches', 'stockmp', 'graft', 'audio', 'bf1943', 'meshview', 'cull')


def main(argv: list[str]) -> int:
    base.ROUND = 'o3'
    base.GROUPS = tuple(g for g in ALL_GROUPS if os.path.isfile(os.path.join(HERE, f'cli_o3_{g}.py')))
    ref = argv[argv.index('--ref') + 1] if '--ref' in argv[:-1] else os.environ.get('BFBC2_O3_REF')
    base.REF = os.path.abspath(ref or base.SUITE)
    if '--group' in argv and argv[argv.index('--group') + 1] not in base.GROUPS:
        print(f'no cases for group {argv[argv.index("--group") + 1]!r} (cli_o3_<group>.py): {", ".join(base.GROUPS)}')
        return 2
    if not base.GROUPS:
        print('skipped: no O3 case modules yet')
        return 0
    return base.main(argv)


if __name__ == '__main__':
    sys.exit(main(sys.argv))
