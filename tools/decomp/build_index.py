r"""Build the private database's SQLite index from ExportDb.java's raw output and the names (round DC0;
tools/decomp/README.md).

    python tools/decomp/build_index.py [--db <private folder>]

Reads <db>\index\functions.jsonl (ExportDb.java), <db>\index\names.json (extract_names.py),
<db>\index\names_applied.jsonl (ApplyNames.java) and <db>\index\measure.json, and writes <db>\index\decomp.sqlite:

  functions(addr, end, size, name, namespace, is_class, source, signature, convention, signature_source, this_type,
            thunk, file, offset, length, status)
  calls(caller, callee)              strings(func, addr, text)
  cites(addr, func, rule, source, location, name, context)      the spec rules and notes citing an address
  meta(key, value)                   measure.json's counts and the build's inputs

Everything it writes stays in the private folder (EA-derived: strings and pseudo-C offsets).
"""
from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from decomp import default_db  # noqa: E402

SCHEMA = '''
CREATE TABLE functions(addr INTEGER PRIMARY KEY, "end" INTEGER, size INTEGER, name TEXT, namespace TEXT,
    is_class INTEGER, source TEXT, signature TEXT, convention TEXT, signature_source TEXT, this_type TEXT,
    thunk INTEGER, file TEXT, offset INTEGER, length INTEGER, status TEXT);
CREATE TABLE calls(caller INTEGER, callee INTEGER);
CREATE TABLE strings(func INTEGER, addr INTEGER, text TEXT);
CREATE TABLE cites(addr INTEGER, func INTEGER, rule TEXT, source TEXT, location TEXT, name TEXT, context TEXT);
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT);
'''
INDEXES = '''
CREATE INDEX functions_name ON functions(name);
CREATE INDEX functions_end ON functions("end");
CREATE INDEX calls_caller ON calls(caller);
CREATE INDEX calls_callee ON calls(callee);
CREATE INDEX strings_func ON strings(func);
CREATE INDEX cites_func ON cites(func);
CREATE INDEX cites_rule ON cites(rule);
CREATE INDEX cites_addr ON cites(addr);
'''


def build(db: Path) -> dict[str, int]:
    idx = db / 'index'
    out = idx / 'decomp.sqlite'
    tmp = idx / 'decomp.sqlite.tmp'
    if tmp.exists():
        tmp.unlink()
    con = sqlite3.connect(tmp)
    con.executescript(SCHEMA)
    n = {'functions': 0, 'calls': 0, 'strings': 0, 'cites': 0}
    starts: list[int] = []
    with open(idx / 'functions.jsonl', encoding='utf-8') as f:
        for line in f:
            d = json.loads(line)
            con.execute('INSERT OR REPLACE INTO functions VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)', (
                d['addr'], d['end'], d['size'], d['name'], d['namespace'], int(d['is_class']), d['source'],
                d['signature'], d['convention'], d['signature_source'], d['this_type'], int(d['thunk']), d['file'],
                d['offset'], d['length'], d['status']))
            con.executemany('INSERT INTO calls VALUES (?,?)', [(d['addr'], c) for c in d['callees']])
            con.executemany('INSERT INTO strings VALUES (?,?,?)', [(d['addr'], a, t) for a, t in d['strings']])
            n['functions'] += 1
            n['calls'] += len(d['callees'])
            n['strings'] += len(d['strings'])
            starts.append(d['addr'])
    applied: dict[int, int | None] = {}
    ap = idx / 'names_applied.jsonl'
    if ap.is_file():
        for line in ap.read_text(encoding='utf-8').splitlines():
            d = json.loads(line)
            applied[d['addr']] = d['function']
    names = json.loads((idx / 'names.json').read_text(encoding='utf-8'))
    for e in names['entries']:
        func = applied.get(e['addr'])
        for c in e['cites']:
            con.execute('INSERT INTO cites VALUES (?,?,?,?,?,?,?)',
                        (e['addr'], func, c['rule'], c['source'], c['where'], c['name'], c['context']))
            n['cites'] += 1
    meas = idx / 'measure.json'
    if meas.is_file():
        for k, v in json.loads(meas.read_text(encoding='utf-8')).items():
            con.execute('INSERT OR REPLACE INTO meta VALUES (?,?)', (k, json.dumps(v)))
    con.executescript(INDEXES)
    con.commit()
    con.close()
    os.replace(tmp, out)
    return n


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--db', default='')
    a = ap.parse_args(argv)
    db = Path(a.db) if a.db else default_db()
    n = build(db)
    print(f'{n} -> {db / "index" / "decomp.sqlite"}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
