r"""Round O2's nav group: the command lines test_cli_o2.py replays for hpdtool, hkpack, convert.py and aimanim.

cases(): the quick tier's (synthetic inputs made in the case folder; no game data). opfor_calls(opfor): the --game
tier's (Project OPFOR's real inputs, copied to the case folder).
"""
from __future__ import annotations

import math
import struct
from typing import Any

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, str], dict[str, str], bool]

# Python's stdio switches the encoding / buffering cases run under too
VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    """rows, each again with stderr on stdout's pipe, and the picked ones (keys) under Python's stdio switches
    (alone and merged)."""
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


# ------------------------------------------------------------------------------------------- aimanim

def _aim_write(a: dict[str, Any]) -> bytes:
    """aimanim.write_reference's bytes (the Python writer, copied, so the inputs do not depend on the port)."""
    nb, n = len(a['bones']), a['poses']
    o = 0x1c + 2 * nb
    head = bytearray(o + (16 - (o - 0x0c) % 16) % 16)
    struct.pack_into('<5I', head, 0, 1, a['addr'], 0, (a['addr'] + 0x10) & 0xffffffff, nb)
    head[0x14] = n
    for i, (bone, kind, _) in enumerate(a['bones']):
        head[0x1c + 2 * i], head[0x1d + 2 * i] = bone, kind
    body = bytearray()
    v = lambda q: struct.pack('<4f', *q)                                               # noqa: E731
    for _bone, kind, block in a['bones']:
        if kind == 0:
            body += v(block[0]) + v(block[1])
        elif kind == 1:
            body += v(block[0]) + b''.join(v(q) for q in block[1])
        else:
            body += b''.join(v(p) + v(q) for p, q in block)
    out = bytes(head) + bytes(body)
    return out[:8] + struct.pack('<I', len(out) - 12) + out[12:]


def _aim_big(b: bytes) -> bytes:
    nb = struct.unpack_from('<I', b, 0x10)[0]
    out = bytearray(b)
    for o in range(0, len(b), 4):
        if 0x14 <= o < 0x1c + 2 * nb:
            continue
        out[o:o + 4] = b[o:o + 4][::-1]
    return bytes(out)


def _aim_sample(n: int = 25) -> dict[str, Any]:
    q = lambda k: (0.0, math.sin(k / 10), 0.0, math.cos(k / 10))                       # noqa: E731
    return {'addr': 0x08A255B0, 'poses': n, 'bones': [
        (0, 0, ((0.0, 0.0, 0.0, 1.0), (0.0, 0.0, 0.0, 1.0))),
        (2, 1, ((0.1, 0.2, 0.3, 0.0), [q(k) for k in range(n)])),
        (17, 2, [((k * 0.01, 1.4, 0.2, 1.0), q(k)) for k in range(n)]),
        (255, 0, ((1e-7, -2.5e10, 3.4e38, -0.0), (1.0, 0.5, 0.25, 1e-40))),
    ]}


def _aim_many() -> bytes:
    bones: list[tuple[int, int, Any]] = []
    for i in range(300):
        if i % 3 == 0:
            bones.append((i % 256, 0, ((i * 0.1, 0.0, 0.0, 1.0), (0.0, 0.0, 0.0, 1.0))))
        elif i % 3 == 1:
            bones.append((i % 256, 1, ((0.0, 0.0, 0.0, 0.0), [(0.0, 0.0, 0.0, 1.0)])))
        else:
            bones.append((i % 256, 2, [((0.5, 0.5, 0.5, 1.0), (0.0, 0.0, 0.0, 1.0))]))
    return _aim_write({'addr': 0, 'poses': 1, 'bones': bones})


def aimanim_cases() -> list[Case]:
    good = _aim_write(_aim_sample())
    nan = bytearray(good)
    struct.pack_into('<f', nan, 0x30, float('nan'))
    no_poses = _aim_write({'addr': 16, 'poses': 0, 'bones': [(1, 0, ((1.0, 2.0, 3.0, 1.0), (0.0, 0.0, 0.0, 1.0))),
                                                               (2, 1, ((0.0, 0.0, 0.0, 0.0), []))]})
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('the usage', {}, []),
        ('a PC file', {'a.res': good}, ['{d}\\a.res']),
        ('a PC file read as a Xbox one (--big after it)', {'a.res': good}, ['{d}\\a.res', '--big']),
        ('a Xbox file --big', {'a.res': _aim_big(good)}, ['{d}\\a.res', '--big']),
        ('a Xbox file without --big', {'a.res': _aim_big(good)}, ['{d}\\a.res']),
        ('--big first (the file is named --big)', {'a.res': good}, ['--big', '{d}\\a.res']),
        ('a missing file', {}, ['{d}\\missing.res']),
        ('an empty argument', {}, ['']),
        ('a folder', {'f': None}, ['{d}\\f']),
        ('too short', {'a.res': b'\x01\x00\x00\x00'}, ['{d}\\a.res']),
        ('not the layout', {'a.res': b'\x02' + bytes(0x2f)}, ['{d}\\a.res']),
        ('a NaN', {'a.res': bytes(nan)}, ['{d}\\a.res']),
        ('no poses (IndexError)', {'a.res': no_poses}, ['{d}\\a.res']),
        ('300 bones (stdout over 8,192 bytes)', {'a.res': _aim_many()}, ['{d}\\a.res']),
        ('a path in cp1252', {'café.res': good}, ['{d}\\café.res']),
        ('a missing path outside cp1252', {}, ['{d}\\Ж.res']),
        ('extra arguments', {'a.res': good}, ['{d}\\a.res', 'x', 'y']),
    ]
    out: list[Case] = [(f'aimanim: {name}', 'aimanim', files, args, {}, False) for name, files, args in rows]
    return with_variants(out, ['aimanim: a missing path outside cp1252', 'aimanim: 300 bones (stdout over 8,192 bytes)',
                               'aimanim: a path in cp1252', 'aimanim: no poses (IndexError)'])


# ------------------------------------------------------------------------------------------- the group

def cases() -> list[Case]:
    return aimanim_cases()


def opfor_calls(opfor: str) -> list[Call]:
    return []
