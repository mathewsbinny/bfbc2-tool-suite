r"""Round O5's NavKit cases (test_cli_o5.py, group nav): the Python cores round O4 still handed off - detailed.py's
sector choice (flagged objects' collision boxes, the stacked floors of the 0.8 m samples) and navkit.py status
(time.ctime of the samples' and the build's file times).

cases(): synthetic inputs only (no game data): a stand-in OPFOR folder with objects.json, packfiles built in code
(test_converter_units' model group: an hkpExtendedMeshShape placing a convex box), sample files of versions 1, 2
and 3 built in code, prepared NavKit levels with given file times. opfor_calls(): detailed on the prepared levels of
the real work\ (read in place: objects.json, the unpacked bundles' packfiles, the 0.8 m samples when there are any),
navkit status of the real folder's levels (read only).
"""
from __future__ import annotations

import json
import math
import os
import random
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import cli_o2_nav as o2nav  # noqa: E402
import cli_o4_navkit as o4nav  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]
with_variants = o2nav.with_variants
ENV = {'OPFOR_ROOT': '{d}\\opfor'}
HEADER = struct.Struct('<4sI3f2i6fI')


def _packfile(centre: tuple[float, float, float], half: tuple[float, float, float]) -> bytes:
    """test_converter_units.model_group_packfile: one convex box placed by a model group (no game data)."""
    sys.path.insert(0, os.path.join(HERE, '..', '..', 'tests'))
    try:
        import test_converter_units as tcu
    finally:
        sys.path.pop(0)
    return bytes(tcu.model_group_packfile((0.0, 0.0, 0.0, 1.0), centre, half)[0])


def _obj(bp: str, x: float, z: float, flags: Any = (True, False), y: float = 0.0, rot: float = 0.0,
         m: list[Any] | None = None) -> dict[str, Any]:
    c, s = round(math.cos(rot), 6), round(math.sin(rot), 6)
    mm = m if m is not None else [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, x, y, z, 1.0]
    o: dict[str, Any] = {'bp': bp, 'm': mm}
    if flags is not None:
        o['flags'] = list(flags) if isinstance(flags, tuple) else flags
    return o


def _samples(version: int, nx: int, nz: int, cols: dict[int, list[tuple[float, int, int]]],
             pitch: float = 0.8, min_x: float = -400.0, min_z: float = -400.0, cut: int = 0) -> bytes:
    """A NavKit sample file: per column (index: [(y, normal y as a byte, flags)]); versions 1 / 2 (count byte, then
    the records) or 3 (the records, the counts, their offset and 'CNT3')."""
    head = HEADER.pack(b'ONS1', version, pitch, min_x, min_z, nx, nz, 200.0, -50.0, 0.4, 0.28, 1.9, 0.1, 0)
    if version in (1, 2):
        body = bytearray()
        for c in range(nx * nz):
            recs = cols.get(c, [])
            body.append(len(recs))
            for y, ny, f in recs:
                body += struct.pack('<f3bB', y, 0, ny, 0, f)
                if version == 2:
                    body += struct.pack('<3I', 1, 2, 3)
        data = head + bytes(body)
        return data[:len(data) - cut] if cut else data
    recs_b, counts = bytearray(), bytearray()
    for c in range(nx * nz):
        recs = cols.get(c, [])
        counts.append(len(recs))
        for y, ny, f in recs:
            recs_b += struct.pack('<f3bB3I', y, 0, ny, 0, f, 1, 2, 3)
    at = len(head) + len(recs_b)
    return head + bytes(recs_b) + bytes(counts) + struct.pack('<Q', at) + b'CNT3'


def _stacked(nx: int, nz: int, rng: random.Random, n: int, gap: float = 3.0) -> dict[int, list[tuple[float, int, int]]]:
    """n random columns with two standable floors gap apart, the rest one floor (some steep, some flagged)."""
    cols: dict[int, list[tuple[float, int, int]]] = {}
    picked = rng.sample(range(nx * nz), n)
    for c in range(nx * nz):
        floor = round(rng.uniform(0, 20), 3)
        cols[c] = [(floor, 127, 0)]
        if c in picked:
            cols[c].append((floor + gap, rng.choice((127, 120, 97)), 0))
        elif rng.random() < 0.1:
            cols[c].append((floor + 5, rng.choice((90, 96, 97, 127)), rng.choice((0, 1, 2, 4))))
    return cols


def _level(lv: str, objects: Any, extra: dict[str, Any] | None = None, raw: bytes | None = None) -> dict[str, Any]:
    f: dict[str, Any] = {'opfor\\game': None,
                         f'opfor\\work\\levels\\{lv}\\objects.json': raw if raw is not None else json.dumps(objects)}
    f.update(extra or {})
    return f


def detailed_cases() -> list[Case]:
    box = _packfile((0.0, 1.0, 0.0), (3.0, 1.0, 40.0))
    small = _packfile((0.0, 0.0, 0.0), (0.5, 0.5, 0.5))
    pk = 'opfor\\work\\levels\\sp_t\\dist\\objects\\'
    pkg = 'opfor\\work\\levels\\sp_t\\pkg\\objects\\'
    common = 'opfor\\work\\navkit\\common\\mp_common\\objects\\'
    havok = {pk + 'long_Havok.res': box, pk + 'small_Havok.res': small, common + 'shared_Havok.res': small,
             pkg + 'long_Havok.res': small,                        # pkg is searched first: this one wins
             pk + 'door_Havok_Havok.res': box, pk + 'bad_Havok.res': b'not a packfile at all'}
    objs = [_obj('Objects/Long', 395.0, 12.0), _obj('objects/small', -95.2, 4.9), _obj('objects/none', 250.0, 250.0),
            _obj('objects/shared', 0.0, 0.0, flags=(False, True)), _obj('objects/door', -600.0, 120.0, rot=0.7),
            _obj('objects/nothing', 10.0, 10.0, flags=[]), _obj('objects/noflag', 10.0, 10.0, flags=None)]
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('flagged objects, no samples', _level('sp_t', objs, havok), ['sp_t', '--offsets', '-5,-5']),
        ('flagged objects, upper-case level, other offsets', _level('sp_t', objs, havok), ['SP_T', '--offsets', '3,-7']),
        ('the common folders\' packfiles', _level('sp_t', [_obj('objects/shared', 50.0, 50.0)], havok),
         ['sp_t', '--offsets', '0,0']),
        ('no flagged object', _level('sp_t', objs[3:], havok), ['sp_t', '--offsets', '0,0']),
        ('no objects at all', _level('sp_t', []), ['sp_t', '--offsets', '0,0']),
        ('integer matrices', _level('sp_t', [_obj('objects/small', 0, 0, m=[1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 199, 3,
                                                                              -1, 1])], havok), ['sp_t', '--offsets', '1,1']),
        ('an object on a sector line (margin)', _level('sp_t', [_obj('objects/none', 105.0, -5.0)]),
         ['sp_t', '--offsets', '0,0']),
        ('a packfile the reader refuses (handed to the Python)', _level('sp_t', [_obj('objects/bad', 0.0, 0.0)], havok),
         ['sp_t', '--offsets', '0,0']),
        ('flags as a string (handed to the Python)', _level('sp_t', [_obj('objects/none', 0.0, 0.0, flags='x')]),
         ['sp_t', '--offsets', '0,0']),
        ('a short matrix (handed to the Python)', _level('sp_t', [_obj('objects/none', 0.0, 0.0, m=[1, 2, 3])]),
         ['sp_t', '--offsets', '0,0']),
        ('objects.json not JSON (handed to the Python)', _level('sp_t', None, raw=b'{not json'),
         ['sp_t', '--offsets', '0,0']),
        ('--score (handed to the Python)', _level('sp_t', objs, havok), ['sp_t', '--offsets', '0,0', '--score']),
    ]
    rng = random.Random(7)
    for version in (1, 2, 3):
        nx, nz = 300, 260
        cols = _stacked(nx, nz, rng, 900)
        data = _samples(version, nx, nz, cols)
        rows.append((f'samples version {version}, stacked floors', _level('sp_t', objs, dict(havok, **{
            'opfor\\work\\navsample\\s.bin': data})), ['sp_t', '--offsets', '-5,-5', '--samples', '{d}\\opfor\\work\\'
                                                                                                    'navsample\\s.bin']))
    nx, nz = 400, 300
    cols = {c: [(0.0, 127, 0), (2.0, 127, 0)] for c in range(0, nx * nz, 3)}
    rows += [
        ('samples, gaps of exactly 2 m', _level('sp_t', [], {'s.bin': _samples(1, nx, nz, cols)}),
         ['sp_t', '--offsets', '-4,-4', '--samples', '{d}\\s.bin']),
        ('samples, steep and flagged second floors', _level('sp_t', [], {'s.bin': _samples(
            3, nx, nz, {c: [(0.0, 127, 0), (9.0, 96 + c % 3, c % 2)] for c in range(nx * nz)})}),
         ['sp_t', '--offsets', '-4,-4', '--samples', '{d}\\s.bin']),
        ('samples, an interrupted run (fewer columns)', _level('sp_t', [], {'s.bin': _samples(
            1, nx, nz, cols)[:HEADER.size + 40_000]}), ['sp_t', '--offsets', '-4,-4', '--samples', '{d}\\s.bin']),
        ('samples cut inside a record (handed to the Python)', _level('sp_t', [], {'s.bin': _samples(
            1, nx, nz, cols, cut=3)}), ['sp_t', '--offsets', '-4,-4', '--samples', '{d}\\s.bin']),
        ('samples, not a sample file (handed to the Python)', _level('sp_t', [], {'s.bin': b'XXXX' + bytes(80)}),
         ['sp_t', '--offsets', '-4,-4', '--samples', '{d}\\s.bin']),
        ('samples, an empty --samples (none)', _level('sp_t', objs, havok), ['sp_t', '--offsets', '-5,-5', '--samples',
                                                                              '']),
        ('samples missing (handed to the Python)', _level('sp_t', []), ['sp_t', '--offsets', '0,0', '--samples',
                                                                         '{d}\\none.bin']),
    ]
    out: list[Case] = [(f'detailed: {n}', 'detailed', f, a, ENV, False) for n, f, a in rows]
    return with_variants(out, ['detailed: flagged objects, no samples', 'detailed: samples version 3, stacked floors'])


def status_cases() -> list[Case]:
    dice, ours = o4nav._finite_hpd(11), o4nav._finite_hpd(40)
    xml = o4nav._level_xml(dice, 1)
    full = o4nav._opfor(dice=dice, built=ours, xml=xml)
    t1, t2 = 1_700_000_000, 1_699_999_999.75
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('a prepared and built level', full, ['status', 'SP_TEST']),
        ('with both samples', dict(full, **{'opf\\work\\navsample\\sp_test_navkit.bin': (b'x', t1),
                                            'opf\\work\\navsample\\sp_test_navkit_hi.bin': (b'y', t2)}),
         ['status', 'sp_test']),
        ('a time in summer, a single-digit day', dict(full, **{'opf\\work\\navsample\\sp_test_navkit.bin':
                                                               (b'x', 1_688_400_000)}), ['status', 'sp_test']),
        ('times around the turns of the clocks', dict(full, **{
            'opf\\work\\navsample\\sp_test_navkit.bin': (b'x', 1_710_054_000),
            'opf\\work\\navsample\\sp_test_navkit_hi.bin': (b'x', 1_730_613_600)}), ['status', 'sp_test']),
        ('not prepared, not built', o4nav._opfor(), ['status', 'sp_test']),
        ('installed', dict(full, **{'opf\\game\\Output\\win32\\ai\\levels\\sp_test\\infantry\\pathdata.hpd.opfor-orig':
                                    dice}), ['status', 'sp_test']),
        ('navkit.json damaged', dict(full, **{'opf\\work\\navkit\\sp_test\\navkit.json': b'{oops'}),
         ['status', 'sp_test']),
        ('navkit.json with float offsets (handed to the Python)', dict(full, **{
            'opf\\work\\navkit\\sp_test\\navkit.json': json.dumps({'box': [1.5, 2, 3, 4], 'y': [0, 1], 'xoff': 1.0,
                                                                   'zoff': 2})}), ['status', 'sp_test']),
        ('navkit.json, float box values', dict(full, **{
            'opf\\work\\navkit\\sp_test\\navkit.json': json.dumps({'box': [-1.25, 2e-7, 1e17, 4], 'y': [0.5, 150],
                                                                   'xoff': -3, 'zoff': 0})}), ['status', 'sp_test']),
        ('navkit.json without a box (handed to the Python)', dict(full, **{
            'opf\\work\\navkit\\sp_test\\navkit.json': json.dumps({'y': [0, 1], 'xoff': 1, 'zoff': 2})}),
         ['status', 'sp_test']),
        ('no level named', full, ['status']),
        ('an unknown level', full, ['status', 'sp_nope']),
    ]
    out: list[Case] = [(f'navkit status: {n}', 'navkit', f, a + ['--opfor', '{d}\\opf'], {}, False) for n, f, a in rows]
    return with_variants(out, ['navkit status: with both samples', 'navkit status: navkit.json damaged'])


def cases() -> list[Case]:
    return detailed_cases() + status_cases()


# ------------------------------------------------------------------------------------------- the game

def opfor_calls(opfor: str) -> list[Call]:
    """detailed on every prepared level of the real work\\ (read in place), with the real sector offsets of its
    navkit.json and, where NavKit sampled it, its 0.8 m samples; navkit status of the same levels (read only)."""
    calls: list[Call] = []
    nk = os.path.join(opfor, 'work', 'navkit')
    levels = sorted(d for d in os.listdir(nk) if os.path.isfile(os.path.join(nk, d, 'navkit.json'))) \
        if os.path.isdir(nk) else []
    for lv in levels:
        if not os.path.isfile(os.path.join(opfor, 'work', 'levels', lv, 'objects.json')):
            continue
        with open(os.path.join(nk, lv, 'navkit.json'), encoding='utf-8') as f:
            s = json.load(f)
        off = f'{s["xoff"]},{s["zoff"]}'
        calls.append((f'detailed {lv}', 'detailed', [lv, '--offsets', off], {}, {'OPFOR_ROOT': opfor}, True))
        smp = os.path.join(opfor, 'work', 'navsample', f'{lv}_navkit.bin')
        if os.path.isfile(smp):
            calls.append((f'detailed {lv} --samples', 'detailed', [lv, '--offsets', off, '--samples', smp], {},
                          {'OPFOR_ROOT': opfor}, True))
        calls.append((f'navkit status {lv}', 'navkit', ['status', lv, '--opfor', opfor], {}, {}, True))
    return calls
