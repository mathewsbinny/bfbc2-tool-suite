r"""Look things up in the private decompiled database of BFBC2Game.exe (round DC0; tools/decomp/README.md).

    python tools/decomp/decomp.py <address>          the function containing it: facts, then its pseudo-C
    python tools/decomp/decomp.py <name>             a function by exact name (Session::setState, FUN_00557150)
    python tools/decomp/decomp.py --info <a|name>    facts only (no pseudo-C)
    python tools/decomp/decomp.py --callers <a|name> / --callees <a|name>
    python tools/decomp/decomp.py --name <part>      functions whose name contains <part> (case-insensitive)
    python tools/decomp/decomp.py --string <text>    functions referencing a string containing <text>
    python tools/decomp/decomp.py --rule <RULE-ID>   the addresses and functions a spec rule cites
    python tools/decomp/decomp.py --stats            the database's counts (measure.json)
    python tools/decomp/decomp.py --unconfirmed      the functions nothing confirms (prologue scan, doubtful range)
    options: --all  include unconfirmed functions among callers and callees
    options: --db <folder> (default BFBC2_DECOMP_DB, else battlefield-decomp-work\bfbc2-decomp beside or above the
             suite: bfbc2core.paths.decomp_root)
             --limit N (lists; default 50)

Addresses are hex, with or without 0x. The database is private and EA-derived: read it, cite addresses and rule ids
in the spec, never paste its pseudo-C into a repo.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sqlite3
import sys
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
FIELDS = ('addr', 'end', 'size', 'name', 'namespace', 'is_class', 'source', 'signature', 'convention',
          'signature_source', 'this_type', 'thunk', 'file', 'offset', 'length', 'status', 'found', 'doubtful',
          'confirmed', 'evidence')
# callers / callees: unconfirmed functions (a prologue scan's, or in the doubtful range, with nothing referencing
# them: tools/decomp/README.md) are left out unless --all
TRUSTED = '(f.confirmed = 1)'


def default_db() -> Path:
    # bfbc2core.paths.decomp_root's order (round DOC1: the repository moved from bfbc2-decomp-private)
    env = os.environ.get('BFBC2_DECOMP_DB')
    if env:
        return Path(env)
    rel = Path('battlefield-decomp-work', 'bfbc2-decomp')
    for up in SUITE.parents:
        if (up / rel).is_dir():
            return up / rel
    return SUITE.parent / rel


class Db:
    def __init__(self, root: Path) -> None:
        self.root = root
        path = root / 'index' / 'decomp.sqlite'
        if not path.is_file():
            raise SystemExit(f'no index at {path} (build it: tools/decomp/README.md)')
        self.con = sqlite3.connect(f'file:{path}?mode=ro', uri=True)

    def row(self, r: tuple[object, ...] | None) -> dict[str, object] | None:
        return dict(zip(FIELDS, r, strict=True)) if r else None

    def containing(self, a: int) -> dict[str, object] | None:
        r = self.con.execute(f'SELECT {",".join(map(q, FIELDS))} FROM functions WHERE addr <= ? ORDER BY addr DESC LIMIT 1',
                             (a,)).fetchone()
        f = self.row(r)
        return f if f and int(str(f['end'])) >= a else None

    def by_name(self, n: str) -> list[dict[str, object]]:
        if '::' in n:
            ns, _, short = n.rpartition('::')
            rows = self.con.execute(f'SELECT {",".join(map(q, FIELDS))} FROM functions WHERE name = ? AND namespace = ?',
                                    (short, ns)).fetchall()
        else:
            rows = self.con.execute(f'SELECT {",".join(map(q, FIELDS))} FROM functions WHERE name = ?', (n,)).fetchall()
        return [f for f in (self.row(r) for r in rows) if f]

    def resolve(self, key: str) -> dict[str, object] | None:
        m = re.fullmatch(r'(?:0x)?([0-9A-Fa-f]{5,8})', key)
        if m and not self.by_name(key):
            return self.containing(int(m.group(1), 16))
        fs = self.by_name(key)
        if len(fs) > 1:
            print(f'{len(fs)} functions are named {key}; the first is shown:', ', '.join(f'{int(str(f["addr"])):08X}' for f in fs))
        return fs[0] if fs else None

    def label(self, a: int) -> str:
        r = self.con.execute('SELECT name, namespace, confirmed FROM functions WHERE addr = ?', (a,)).fetchone()
        return f'{a:08X} ' + ((r[1] + '::' if r[1] else '') + r[0] + ('' if r[2] else ' (unconfirmed)') if r else '?')

    def related(self, a: int, callers: bool, everything: bool) -> list[int]:
        if callers:
            sql = 'SELECT DISTINCT c.caller FROM calls c JOIN functions f ON f.addr = c.caller WHERE c.callee = ?'
        else:
            sql = 'SELECT DISTINCT c.callee FROM calls c LEFT JOIN functions f ON f.addr = c.callee WHERE c.caller = ?'
        if not everything:
            sql += f' AND ({TRUSTED} OR f.addr IS NULL)'
        return [r[0] for r in self.con.execute(sql + ' ORDER BY 1', (a,))]

    def pseudo(self, f: dict[str, object]) -> str:
        with open(self.root / 'pseudo' / str(f['file']), 'rb') as fh:
            fh.seek(int(str(f['offset'])))
            return fh.read(int(str(f['length']))).decode('utf-8', errors='replace')


def q(c: str) -> str:
    return f'"{c}"'


def full(f: dict[str, object]) -> str:
    return (str(f['namespace']) + '::' if f['namespace'] else '') + str(f['name'])


def info(db: Db, f: dict[str, object], everything: bool = False) -> None:
    a = int(str(f['addr']))
    print(f'{a:08X} - {int(str(f["end"])):08X}  {full(f)}  ({f["size"]} bytes, {f["convention"]}'
          f'{", class" if f["is_class"] else ""}{", thunk" if f["thunk"] else ""}; decompile: {f["status"]})')
    print(f'  {f["signature"]}')
    if f['found'] != 'analysis' or f['doubtful']:
        print(f'  found by {f["found"]}' + (f'; {"confirmed: " + str(f["evidence"]) if f["confirmed"] else "UNCONFIRMED: nothing references it"}'
                                            if f['doubtful'] else ''))
    if f['this_type']:
        print(f'  this: {f["this_type"]}')
    callers = db.related(a, True, everything)
    callees = db.related(a, False, everything)
    print(f'  callers {len(callers)}: ' + ', '.join(db.label(c) for c in callers[:12]) + (' ...' if len(callers) > 12 else ''))
    print(f'  callees {len(callees)}: ' + ', '.join(db.label(c) for c in callees[:12]) + (' ...' if len(callees) > 12 else ''))
    strs = db.con.execute('SELECT addr, text FROM strings WHERE func = ? ORDER BY addr', (a,)).fetchall()
    for sa, t in strs[:15]:
        print(f'  string {sa:08X} {t!r}')
    if len(strs) > 15:
        print(f'  ... {len(strs) - 15} more strings')
    cites = db.con.execute('SELECT DISTINCT addr, rule, location FROM cites WHERE func = ? OR addr = ? ORDER BY addr',
                           (a, a)).fetchall()
    for ca, rule, loc in cites[:20]:
        print(f'  cited {ca:08X} by {rule or "-"} ({loc})')
    if len(cites) > 20:
        print(f'  ... {len(cites) - 20} more citations')


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0], formatter_class=argparse.RawDescriptionHelpFormatter,
                                 epilog=__doc__)
    ap.add_argument('key', nargs='?')
    ap.add_argument('--db', default='')
    ap.add_argument('--info', action='store_true')
    ap.add_argument('--callers', action='store_true')
    ap.add_argument('--callees', action='store_true')
    ap.add_argument('--name', default='')
    ap.add_argument('--string', default='')
    ap.add_argument('--rule', default='')
    ap.add_argument('--stats', action='store_true')
    ap.add_argument('--unconfirmed', action='store_true')
    ap.add_argument('--all', action='store_true')
    ap.add_argument('--limit', type=int, default=50)
    a = ap.parse_args(argv)
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    db = Db(Path(a.db) if a.db else default_db())
    try:
        return run(db, a, ap)
    finally:
        db.con.close()


def run(db: Db, a: argparse.Namespace, ap: argparse.ArgumentParser) -> int:
    if a.stats:
        for k, v in db.con.execute('SELECT key, value FROM meta ORDER BY key'):
            print(f'{k}: {json.loads(v)}')
        return 0
    if a.unconfirmed:
        for r in db.con.execute('SELECT addr, name, size, found FROM functions WHERE confirmed = 0 ORDER BY addr LIMIT ?',
                                (a.limit,)):
            print(f'{r[0]:08X} {r[1]} ({r[2]} bytes, {r[3]})')
        return 0
    if a.name:
        for r in db.con.execute('SELECT addr, namespace, name, size FROM functions WHERE name LIKE ? OR namespace LIKE ? '
                                'ORDER BY addr LIMIT ?', (f'%{a.name}%', f'%{a.name}%', a.limit)):
            print(f'{r[0]:08X} {(r[1] + "::") if r[1] else ""}{r[2]} ({r[3]} bytes)')
        return 0
    if a.string:
        for r in db.con.execute('SELECT s.func, s.addr, s.text FROM strings s WHERE s.text LIKE ? ORDER BY s.func LIMIT ?',
                                (f'%{a.string}%', a.limit)):
            print(f'{db.label(r[0])}  string {r[1]:08X} {r[2]!r}')
        return 0
    if a.rule:
        for r in db.con.execute('SELECT DISTINCT addr, func, location FROM cites WHERE rule = ? ORDER BY addr', (a.rule,)):
            print(f'{r[0]:08X} in {db.label(r[1]) if r[1] is not None else "(no function)"}  ({r[2]})')
        return 0
    if not a.key:
        ap.print_usage()
        return 2
    f = db.resolve(a.key)
    if not f:
        print(f'no function at or named {a.key}')
        return 1
    fa = int(str(f['addr']))
    if a.callers or a.callees:
        for c in db.related(fa, bool(a.callers), a.all):
            print(db.label(c))
        return 0
    info(db, f, a.all)
    if not a.info:
        print()
        print(db.pseudo(f))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
