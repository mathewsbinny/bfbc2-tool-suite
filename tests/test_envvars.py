r"""The environment-variable registry (core\bfbc2core\envvars.py, round QW1): every quoted BFBC2_* / BC2_* / BF1943_* /
OPFOR_* name in the suite's code (Python, Rust, workflows, batch, C#, C++) is a registered variable or a registered
non-variable, and every registered variable is still read somewhere (the 'opfor' group: by Project OPFOR).

  python tests\test_envvars.py
"""
import os
import re
import sys

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(SUITE, 'core'))
from bfbc2core import envvars  # noqa: E402

NAME = re.compile(r'''["'](?:\$env:)?((?:BFBC2|BC2|BF1943|OPFOR)_[A-Z0-9_]+)["']''')
SKIP_DIRS = {'.git', '__pycache__', 'target', '.mypy_cache', '.ruff_cache', 'build', 'bin', 'node_modules'}
EXTS = ('.py', '.pyw', '.rs', '.yml', '.yaml', '.bat', '.cs', '.cpp', '.h')
OWN = {os.path.join(SUITE, 'core', 'bfbc2core', 'envvars.py'), os.path.abspath(__file__)}


def used() -> dict[str, set[str]]:
    out: dict[str, set[str]] = {}
    for top, dirs, files in os.walk(SUITE):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
        for f in files:
            p = os.path.join(top, f)
            if not f.endswith(EXTS) or p in OWN:
                continue
            with open(p, encoding='utf-8', errors='replace') as fh:
                for m in NAME.finditer(fh.read()):
                    out.setdefault(m.group(1), set()).add(os.path.relpath(p, SUITE))
    return out


def main() -> int:
    names = used()
    known = set(envvars.VARIABLES) | set(envvars.NOT_VARIABLES)
    missing = sorted(set(names) - known)
    unused = sorted(n for n, (group, _) in envvars.VARIABLES.items() if n not in names and group != 'opfor')
    groups = {g for g, _ in envvars.VARIABLES.values()}
    fails = []
    for n in missing:
        fails.append(f'{n} is read in {", ".join(sorted(names[n])[:3])} but not in bfbc2core.envvars.VARIABLES')
    for n in unused:
        fails.append(f'{n} is registered but nothing in the suite reads it any more (remove its row)')
    if not groups <= {'cache', 'log', 'native', 'switch', 'paths', 'workers', 'build', 'opfor', 'test'}:
        fails.append(f'unknown groups: {sorted(groups)}')
    for f in fails:
        print('FAIL', f)
    print(f'{len(envvars.VARIABLES)} variables registered, {len(names)} quoted names in the code; '
          + ('all passed' if not fails else f'{len(fails)} failures'))
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
