r"""Build the private database's SQLite index from ExportDb.java's raw output and the names (round DC0;
tools/decomp/README.md).

    python tools/decomp/build_index.py [--db <private folder>]

Reads <db>\index\functions.jsonl (ExportDb.java; a range export's functions_<FIRST>.jsonl replaces its part),
<db>\index\names.json (extract_names.py), <db>\index\names_applied.jsonl (ApplyNames.java) and <db>\index\measure.json, and writes <db>\index\decomp.sqlite:

  functions(addr, end, size, name, namespace, is_class, source, signature, convention, signature_source, this_type,
            thunk, file, offset, length, status, found, doubtful, confirmed, evidence)
            found: analysis / orphan / prologue; doubtful: a prologue-scan function or one in the --skip range;
            confirmed: not doubtful, or referenced (ConfirmFunctions.java, <db>\index\provenance.jsonl)
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
from typing import Any

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from decomp import default_db  # noqa: E402

SCHEMA = '''
CREATE TABLE functions(addr INTEGER PRIMARY KEY, "end" INTEGER, size INTEGER, name TEXT, namespace TEXT,
    is_class INTEGER, source TEXT, signature TEXT, convention TEXT, signature_source TEXT, this_type TEXT,
    thunk INTEGER, file TEXT, offset INTEGER, length INTEGER, status TEXT, found TEXT, doubtful INTEGER,
    confirmed INTEGER, evidence TEXT);
CREATE TABLE calls(caller INTEGER, callee INTEGER);
CREATE TABLE strings(func INTEGER, addr INTEGER, text TEXT);
CREATE TABLE cites(addr INTEGER, func INTEGER, rule TEXT, source TEXT, location TEXT, name TEXT, context TEXT);
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT);
'''
INDEXES = '''
CREATE INDEX functions_name ON functions(name);
CREATE INDEX functions_end ON functions("end");
CREATE INDEX functions_confirmed ON functions(confirmed);
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
    # functions.jsonl, then each range export (functions_<FIRST>.jsonl, in address order): a range export's
    # functions replace every earlier entry whose address lies at or past its FIRST.
    rows: dict[int, dict[str, Any]] = {}
    for path in [idx / 'functions.jsonl', *sorted(idx.glob('functions_*.jsonl'))]:
        if not path.is_file():
            continue
        first = int(path.stem.split('_')[1], 16) if '_' in path.stem else 0
        for a in [a for a in rows if a >= first]:
            del rows[a]
        with open(path, encoding='utf-8') as f:
            for line in f:
                try:
                    d = json.loads(line)
                except json.JSONDecodeError:
                    continue                   # the last line of an export that was stopped
                if int(d['addr']) >= first:
                    rows[int(d['addr'])] = d
    prov: dict[int, dict[str, Any]] = {}
    pp = idx / 'provenance.jsonl'
    if pp.is_file():
        for line in pp.read_text(encoding='utf-8').splitlines():
            d = json.loads(line)
            prov[int(d['addr'])] = d
    statuses: dict[str, int] = {}
    founds: dict[str, int] = {}
    for _, d in sorted(rows.items()):
        pv = prov.get(int(d['addr']), {'found': 'analysis', 'doubtful': False, 'confirmed': True, 'evidence': []})
        con.execute('INSERT OR REPLACE INTO functions VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)', (
            d['addr'], d['end'], d['size'], d['name'], d['namespace'], int(d['is_class']), d['source'],
            d['signature'], d['convention'], d['signature_source'], d['this_type'], int(d['thunk']), d['file'],
            d['offset'], d['length'], d['status'], pv['found'], int(pv['doubtful']), int(pv['confirmed']),
            ', '.join(pv['evidence'])))
        key = f"functions_{pv['found']}" + ('' if not pv['doubtful'] else '_confirmed' if pv['confirmed'] else '_unconfirmed')
        founds[key] = founds.get(key, 0) + 1
        con.executemany('INSERT INTO calls VALUES (?,?)', [(d['addr'], c) for c in d['callees']])
        con.executemany('INSERT INTO strings VALUES (?,?,?)', [(d['addr'], a, t) for a, t in d['strings']])
        n['functions'] += 1
        n['calls'] += len(d['callees'])
        n['strings'] += len(d['strings'])
        st = str(d['status']).split(':')[0]
        statuses[st] = statuses.get(st, 0) + 1
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
    for k, v in founds.items():
        con.execute('INSERT OR REPLACE INTO meta VALUES (?,?)', (k, json.dumps(v, sort_keys=True)))
    for k, v in statuses.items():
        con.execute('INSERT OR REPLACE INTO meta VALUES (?,?)', ('decompile_' + k, json.dumps(v, sort_keys=True)))
    meas = idx / 'measure.json'
    if meas.is_file():
        for k, v in json.loads(meas.read_text(encoding='utf-8')).items():
            if not k.startswith('decompile'):        # the statuses above count the merged exports
                con.execute('INSERT OR REPLACE INTO meta VALUES (?,?)', (k, json.dumps(v, sort_keys=True)))
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
