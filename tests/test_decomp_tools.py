r"""The decompiled database's tools (tools/decomp) on synthetic input only: the name extraction from spec-style text,
the type plan from typeinfo-style records, and the lookup tool on a small index built from made-up functions. No
game, no Ghidra, no private database.

  python tests\test_decomp_tools.py
"""
from __future__ import annotations

import io
import json
import os
import sys
import tempfile
from contextlib import redirect_stdout
from pathlib import Path

SUITE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SUITE / 'tools' / 'decomp'))

import build_index  # noqa: E402
import decomp  # noqa: E402
import extract_names as en  # noqa: E402
import gen_types  # noqa: E402

RANGES = [(0x401000, 0x1D12C00)]


def names_from(text: str) -> dict[int, dict[str, object]]:
    with tempfile.TemporaryDirectory() as t:
        p = Path(t) / 'x.md'
        p.write_text(text, encoding='utf-8')
        entries: dict[int, en.Entry] = {}
        en.scan_markdown(entries, p, 'x.md', 'spec', RANGES, True)
        return {a: en.resolve(e) for a, e in entries.items()}


def test_names() -> None:
    r = names_from('### ABC-DEF-1: A rule\n\n[exe: 0x8527D0 Unlocks::applyLicenses; the AI init 0x5B33C0; '
                   'calls 0x5B4B20; hash 0xD2199312; 0x56F4F0 - 0x56F500]\n\n## Other\n\n'
                   '| Main loop | 0x5703E0 |\n| `searchFriend`, `x` | 0x7D6BF0 |\n')
    assert r[0x8527D0]['name'] == 'Unlocks::applyLicenses' and r[0x8527D0]['rules'] == ['ABC-DEF-1'], r[0x8527D0]
    assert r[0x5B33C0]['name'] == 'AI_init', r[0x5B33C0]
    assert r[0x5B4B20]['name'] is None, r[0x5B4B20]            # "calls" names nothing
    assert 0xD2199312 not in r                                 # outside the image
    assert r[0x56F500]['name'] is None                         # a range's end
    assert r[0x5703E0]['name'] == 'Main_loop' and r[0x5703E0]['rules'] == [], r[0x5703E0]
    assert r[0x7D6BF0]['name'] is None, r[0x7D6BF0]            # a cell listing several things


def test_precedence() -> None:
    e = en.Entry(0x401090, [en.Cand('spec', 'sector_nodes', 'a:1', '', 'R-1', 1),
                            en.Cand('symbols', 'purecall_handler', 'b:1', '', None, 2),
                            en.Cand('research', 'pure_virtual', 'c:1', '', None, 2)])
    r = en.resolve(e)
    assert r['name'] == 'purecall_handler' and r['name_source'] == 'symbols', r   # declared beats picked words
    e.cands.append(en.Cand('spec', 'Foo::bar', 'a:2', '', 'R-2', 3))
    r = en.resolve(e)
    assert r['name'] == 'Foo::bar' and 'purecall_handler' in r['conflict'], r      # the spec wins among declared


def test_types() -> None:
    recs = {
        'Float32': {'kind': 'primitive', 'size': 4, 'module': 'Core'},
        'DataContainer': {'kind': 'class', 'size': 8, 'module': 'Core'},
        'V': {'kind': 'value type', 'size': 8, 'module': 'M', 'fields': [['X', 'Float32', 0], ['Y', 'Float32', 4]]},
        'E': {'kind': 'enumeration', 'size': 4, 'module': 'M', 'values': [['E_a', 0], ['E_b', 1]]},
        'D': {'kind': 'class', 'size': 60, 'module': 'M', 'parent': 'DataContainer',
              'fields': [['Pos', 'V', 8], ['Mode', 'E', 16], ['Other', 'DataContainer', 20], ['List', 'V[]', 24],
                         ['Refs', 'DataContainer[]', 40]]},
    }
    plan = gen_types.build(recs)
    order = [t['name'] for t in plan['types']]
    assert order.index('V') < order.index('D') and order.index('DataContainer') < order.index('D'), order
    d = next(t for t in plan['types'] if t['name'] == 'D')
    m = {x['name']: x for x in d['members']}
    assert m['super_DataContainer']['offset'] == 0
    assert m['Other']['type'] == {'kind': 'pointer', 'name': 'DataContainer'}
    assert m['List']['type']['name'] == 'Array<V>' and m['Refs']['type']['name'] == 'RefArray<DataContainer>'
    sizes = {t['name']: t['size'] for t in plan['types']}
    assert sizes['Array<V>'] == 16 and sizes['RefArray<DataContainer>'] == 20


def test_lookup() -> None:
    with tempfile.TemporaryDirectory() as t:
        db = Path(t)
        (db / 'index').mkdir()
        (db / 'pseudo').mkdir()
        body_a, body_b = '// ==== 00401000 a\nint a(void) { return b(); }\n\n', '// ==== 00401020 b\nint b(void);\n\n'
        (db / 'pseudo' / '0040.c').write_bytes((body_a + body_b).encode())
        fs = [
            {'addr': 0x401000, 'end': 0x40101F, 'size': 32, 'name': 'a', 'namespace': 'Foo', 'is_class': True,
             'source': 'USER_DEFINED', 'signature': 'int a(void)', 'convention': '__thiscall', 'signature_source': 'DEFAULT',
             'this_type': None, 'thunk': False, 'callers': [], 'callees': [0x401020], 'strings': [[0x1405000, 'hello']],
             'file': '0040.c', 'offset': 0, 'length': len(body_a), 'status': 'ok'},
            {'addr': 0x401020, 'end': 0x40102F, 'size': 16, 'name': 'b', 'namespace': None, 'is_class': False,
             'source': 'DEFAULT', 'signature': 'int b(void)', 'convention': 'unknown', 'signature_source': 'DEFAULT',
             'this_type': None, 'thunk': False, 'callers': [0x401000], 'callees': [], 'strings': [],
             'file': '0040.c', 'offset': len(body_a), 'length': len(body_b), 'status': 'ok'},
        ]
        (db / 'index' / 'functions.jsonl').write_text(''.join(json.dumps(f) + '\n' for f in fs), encoding='utf-8')
        names = {'entries': [{'addr': 0x401008, 'cites': [{'rule': 'ABC-DEF-1', 'source': 'spec', 'where': 'x.md:3',
                                                          'name': None, 'context': 'ctx'}]}]}
        (db / 'index' / 'names.json').write_text(json.dumps(names), encoding='utf-8')
        (db / 'index' / 'names_applied.jsonl').write_text(json.dumps({'addr': 0x401008, 'function': 0x401000}) + '\n',
                                                          encoding='utf-8')
        n = build_index.build(db)
        assert n == {'functions': 2, 'calls': 1, 'strings': 1, 'cites': 1}, n

        def run(*args: str) -> str:
            buf = io.StringIO()
            with redirect_stdout(buf):
                decomp.main(['--db', str(db), *args])
            return buf.getvalue()

        out = run('401010')
        assert 'Foo::a' in out and 'return b();' in out and 'ABC-DEF-1' in out and "'hello'" in out, out
        assert 'int b(void);' in run('b') and 'return' not in run('b')
        assert run('--callers', 'b').strip() == '00401000 Foo::a'
        assert '00401000' in run('--string', 'hell') and '00401008' in run('--rule', 'ABC-DEF-1')
        assert run('--name', 'Foo').count('\n') == 1
        db_con = decomp.Db(db)
        assert db_con.containing(0x401030) is None              # past b's end
        db_con.con.close()
        # a prologue-scan function nothing references: out of callers / callees unless --all
        (db / 'index' / 'provenance.jsonl').write_text(
            json.dumps({'addr': 0x401000, 'found': 'prologue', 'doubtful': True, 'confirmed': False, 'evidence': []})
            + '\n', encoding='utf-8')
        build_index.build(db)
        assert run('--callers', 'b').strip() == '', run('--callers', 'b')
        assert run('--callers', 'b', '--all').strip() == '00401000 Foo::a (unconfirmed)'
        assert 'UNCONFIRMED' in run('--info', '401000') and '00401000' in run('--unconfirmed')
        assert run('--callees', 'a').strip() == '00401020 b'    # its own callees stay listed


def main() -> int:
    tests = [test_names, test_precedence, test_types, test_lookup]
    failed = 0
    for t in tests:
        try:
            t()
            print(f'ok   {t.__name__}')
        except AssertionError as e:
            failed += 1
            print(f'FAIL {t.__name__}: {e}')
    return 1 if failed else 0


if __name__ == '__main__':
    os.environ.pop('BFBC2_DECOMP_DB', None)
    sys.exit(main())
