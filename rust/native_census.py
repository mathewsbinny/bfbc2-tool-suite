r"""Who calls each function and class the Rust core's Python module exports (bc2_native; audit AU5 ARC-12).

    python rust\native_census.py [--opfor <folder>] [--markdown] [--unused]

The exports are read from rust\bc2-py\src\lib.rs (its `#[pymodule_export] use super::<file>::{...}` lists). Each
name is looked up, as a whole word, in the suite's tracked Python - tools and tests counted apart - and in Project
OPFOR BFBC2's (its src\ folder: --opfor, else OPFOR_ROOT, else project-opfor-bfbc2 beside the suite; left out when
not found). A name with no caller outside the suite's tests is a twin's retired path the module still carries: what
the census is for is telling which exports a retiring round can drop.

  --markdown  a table per bc2-py source file (name, tool callers, test callers, OPFOR callers)
  --unused    only the names no tool of the suite or OPFOR calls

Exit code 0 always: a report, not a check (the check, an export with no caller failing, comes when the twins retire).
"""
import os
import re
import subprocess
import sys

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIB = os.path.join(SUITE, 'rust', 'bc2-py', 'src', 'lib.rs')
EXPORT = re.compile(r'#\[pymodule_export\]\s*use\s+super::(\w+)::(?:\{([^}]*)\}|(\w+))\s*;', re.S)


def exports() -> list[tuple[str, str]]:
    """(source file, name) of every export, in lib.rs's order."""
    with open(LIB, encoding='utf-8') as f:
        text = re.sub(r'//[^\n]*', '', f.read())
    out = []
    for m in EXPORT.finditer(text):
        names = m.group(2) if m.group(2) is not None else m.group(3)
        out += [(m.group(1), n.strip()) for n in names.split(',') if n.strip()]
    return out


def opfor_root(argv: list[str]) -> str | None:
    if '--opfor' in argv:
        return argv[argv.index('--opfor') + 1]
    for c in (os.environ.get('OPFOR_ROOT'), os.path.join(os.path.dirname(SUITE), 'project-opfor-bfbc2')):
        if c and os.path.isdir(os.path.join(c, 'src')):
            return c
    return None


def python_files(root: str, tracked: bool) -> list[str]:
    if tracked:
        r = subprocess.run(['git', '-C', root, 'ls-files', '*.py'], capture_output=True, text=True)
        return [os.path.join(root, p) for p in r.stdout.split()]
    out = []
    for d, dirs, files in os.walk(os.path.join(root, 'src')):
        dirs[:] = [x for x in dirs if x not in ('__pycache__', '.git')]
        out += [os.path.join(d, f) for f in files if f.endswith('.py')]
    return out


def is_test(path: str) -> bool:
    parts = path.replace('\\', '/').split('/')
    return 'tests' in parts or parts[-1].startswith('test_')


def words(paths: list[str]) -> dict[str, set[str]]:
    """identifier -> the files using it."""
    out: dict[str, set[str]] = {}
    for p in paths:
        try:
            with open(p, encoding='utf-8', errors='replace') as f:
                text = f.read()
        except OSError:
            continue
        for w in set(re.findall(r'[A-Za-z_][A-Za-z0-9_]*', text)):
            out.setdefault(w, set()).add(p)
    return out


def main(argv: list[str]) -> int:
    ex = exports()
    suite = python_files(SUITE, True)
    tools = words([p for p in suite if not is_test(p)])
    tests = words([p for p in suite if is_test(p)])
    root = opfor_root(argv)
    opfor = words(python_files(root, False)) if root else {}
    rows = [(src, n, len(tools.get(n, ())), len(tests.get(n, ())), len(opfor.get(n, ()))) for src, n in ex]
    unused = [r for r in rows if r[2] == 0 and r[4] == 0]
    if '--unused' in argv:
        for src, n, _t, te, _o in unused:
            print(f'{n}  ({src}.rs; {te} test file(s))')
    elif '--markdown' in argv:
        last = None
        for src, n, t, te, o in rows:
            if src != last:
                print(f'\n`{src}.rs`\n\n| export | tools | tests | OPFOR |\n|---|---|---|---|')
                last = src
            print(f'| `{n}` | {t} | {te} | {o if root else "-"} |')
    else:
        for src, n, t, te, o in rows:
            print(f'{n:34s} {src:24s} tools {t:3d}  tests {te:3d}  OPFOR {o if root else "-"}')
    print(f'\n{len(rows)} exports in {len({r[0] for r in rows})} files; called by a suite tool {sum(1 for r in rows if r[2])}'
          f', by OPFOR {sum(1 for r in rows if r[4]) if root else "-"} ({root or "OPFOR not found"}); '
          f'{len(unused)} by no tool of either (tests only, or nothing)', file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
