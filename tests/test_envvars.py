r"""The environment-variable registry (core\bfbc2core\envvars.py, round QW1): every quoted BFBC2_* / BC2_* / BF1943_* /
OPFOR_* name in the suite's code (Python, Rust, workflows, batch, C#, C++) is a registered variable or a registered
non-variable, and every registered variable is still read somewhere (the 'opfor' group: by Project OPFOR). No new
name carries a round code (round TS1, audit AU3 TS-13: BC2_GW1_OPFOR, BC2_R5C_PACKAGE_OPFOR ...).

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
# a round code as a part of a name (BC2_GW1_OPFOR, BFBC2_O5_REF): names say what a variable does (round TS1, audit AU3
# TS-13); the 22 named so before it (XF1's BC2_XF1_STAGE landed while TS1 ran) stay until PR1c renames them, a new
# one fails
ROUND = re.compile(r'_[A-Z]{1,2}[0-9]+[A-Z]?(?=_)')
ROUND_CODED = frozenset({
    'BC2_EDITOR_G3', 'BC2_EDITOR_G3_ART', 'BC2_EDITOR_G3_KEEP', 'BC2_EDITOR_G3_OUT', 'BC2_EDITOR_G3_PROJECTS',
    'BC2_EDITOR_G3_ROOT', 'BC2_EDITOR_G3_SHOTS', 'BC2_EDITOR_G4_REAL', 'BC2_EDITOR_G4_SHOTS', 'BC2_EX1_STAGE',
    'BC2_GW1_LEVELS', 'BC2_GW1_OPFOR', 'BC2_GW1_PACKAGE_LEVELS', 'BC2_R5C_PACKAGE_LEVELS', 'BC2_R5C_PACKAGE_OPFOR',
    'BFBC2_O2_REF', 'BFBC2_O3_REF', 'BFBC2_O5_LEVELS', 'BFBC2_O5_REF', 'BFBC2_O5_STANDIN', 'BFBC2_SD1_SIBLINGS_OF',
    'BC2_XF1_STAGE',
})


def round_coded(name: str) -> bool:
    """name carries a round code after its prefix (BC2_GW1_OPFOR: GW1)."""
    return bool(ROUND.search('_' + name.split('_', 1)[-1] + '_'))


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
    for n in sorted(known | set(names)):
        if round_coded(n) and n not in ROUND_CODED:
            fails.append(f'{n} is named by a round code: name it by what it does (audit AU3 TS-13)')
    if not (round_coded('BC2_GW1_OPFOR') and round_coded('BFBC2_O5_REF') and not round_coded('BF1943_XENON')
            and not round_coded('BFBC2_RUST_NO_SKIP')):
        fails.append('the round-code rule does not tell BC2_GW1_OPFOR from BF1943_XENON')
    for f in fails:
        print('FAIL', f)
    print(f'{len(envvars.VARIABLES)} variables registered, {len(names)} quoted names in the code; '
          + ('all passed' if not fails else f'{len(fails)} failures'))
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
