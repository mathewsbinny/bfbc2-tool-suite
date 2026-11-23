r"""Who calls each function and class the Rust core's Python module exports (bc2_native; audit AU5 ARC-12).

    python rust\native_census.py [--opfor <folder>] [--markdown] [--unused]
    python rust\native_census.py --check [--opfor <folder>]
    python rust\native_census.py --record [--opfor <folder>]

The exports are read from rust\bc2-py\src\lib.rs (its `#[pymodule_export] use super::<file>::{...}` lists). Each
name is looked up, as a whole word, in the suite's tracked Python - tools and tests counted apart - and in Project
OPFOR BFBC2's (its src\ folder: --opfor, else OPFOR_ROOT, else project-opfor-bfbc2 beside the suite; left out when
not found). A name with no caller outside the suite's tests is a twin's retired path the module still carries: what
the census is for is telling which exports a retiring round can drop.

  --markdown  a table per bc2-py source file (name, tool callers, test callers, OPFOR callers)
  --unused    only the names no tool of the suite or OPFOR calls
  --check     the check (round PR1b-2, run_tests.py quick): an export nothing calls - no tool or test of the suite,
              no file of OPFOR's - fails (a class only returned, never named, is listed in RETURNED with why).
              Without OPFOR's checkout the names OPFOR called when the census was last recorded count
              (rust\native_census.json, --record); with it, a name OPFOR called then and calls no more is said.
  --record    rewrites rust\native_census.json: the exports OPFOR's files name (sorted)

Exit code 0 for the reports; --check exits 1 on an export without a caller.
"""
import json
import os
import re
import subprocess
import sys

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIB = os.path.join(SUITE, 'rust', 'bc2-py', 'src', 'lib.rs')
RECORD = os.path.join(SUITE, 'rust', 'native_census.json')
# classes the module returns and Python never names (no caller by name is right for them)
RETURNED = {
    'AptLines': 'apt_decode(records=True) returns it (the dump kept in Rust until a line is asked for)',
    'NavStageGraphs': "navstage_graphs returns it (the stage's graphs, read by index)",
    'Ww2Index': "ww2build_stock_index returns it (OPFOR's wake_install keeps it as its index's .native)",
    'Ww2PackJob': 'ww2build_pack_start returns it (wake_install.finish_packs waits on it)',
}
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


def check(rows: list[tuple[str, str, int, int, int]], root: str | None) -> int:
    recorded: list[str] = []
    if os.path.isfile(RECORD):
        with open(RECORD, encoding='utf-8') as f:
            recorded = json.load(f).get('opfor', [])
    bad = []
    for src, n, t, te, o in rows:
        opfor = o > 0 if root else n in recorded
        if not (t or te or opfor or n in RETURNED):
            bad.append(f'{n} ({src}.rs): no tool, test or OPFOR file calls it - drop the export, or name it in RETURNED')
    if root:
        gone = sorted(set(recorded) - {n for _s, n, _t, _te, o in rows if o})
        if gone:
            print(f'note: OPFOR no longer calls {", ".join(gone)} (python rust\\native_census.py --record)')
    for b in bad:
        print(f'FAIL {b}')
    print(f'{"FAIL" if bad else "ok  "} {len(rows)} exports, {len(bad)} without a caller'
          f' (OPFOR: {root or "the record, " + str(len(recorded)) + " names"})')
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    ex = exports()
    suite = python_files(SUITE, True)
    tools = words([p for p in suite if not is_test(p)])
    tests = words([p for p in suite if is_test(p)])
    root = opfor_root(argv)
    opfor = words(python_files(root, False)) if root else {}
    rows = [(src, n, len(tools.get(n, ())), len(tests.get(n, ())), len(opfor.get(n, ()))) for src, n in ex]
    unused = [r for r in rows if r[2] == 0 and r[4] == 0]
    if '--check' in argv:
        return check(rows, root)
    if '--record' in argv:
        if not root:
            print('FAIL OPFOR not found: nothing recorded')
            return 1
        with open(RECORD, 'w', encoding='utf-8', newline='\n') as f:
            json.dump({'opfor': sorted({n for _s, n, _t, _te, o in rows if o})}, f, indent=1, sort_keys=True)
            f.write('\n')
        print(f'ok   {RECORD}')
        return 0
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
