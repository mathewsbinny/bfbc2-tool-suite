r"""Round E3o's NavKit parity on real data (rust/BENCHMARKS.md "E3o"): the Rust reader of the runtime mesh
(kymesh.parse_mesh) against kymesh.parse_mesh_reference (the format's Python reader, kept for good) on every sector of
the files given. Read only. (navdata.load's own comparison, "load", went with navdata's Python twins on 2026-11-18,
round R1: tests/test_e3o_native.py --game checks every path-data file against the answers recorded from them.)

  python rust\parity_e3o_nav.py mesh <file or folder>...      every .hpd under the folders

Values are compared bit for bit: every float by its IEEE bytes, every array by dtype, shape and bytes, every
dataclass, list, tuple and dict by type and order (canon).
"""
import dataclasses
import hashlib
import os
import struct
import sys
import time
from typing import Any

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
import hpdtool  # noqa: E402
import kymesh  # noqa: E402
import numpy as np  # noqa: E402


def canon(x: Any, h: Any) -> None:
    """Feeds x's exact value (types, order, float bits) into the hash h."""
    if isinstance(x, np.ndarray):
        h.update(b'A' + x.dtype.str.encode() + repr(x.shape).encode())
        h.update(np.ascontiguousarray(x).tobytes())
    elif isinstance(x, np.generic):
        h.update(b'G' + type(x).__name__.encode() + x.tobytes())
    elif isinstance(x, bool) or x is None:
        h.update(repr(x).encode())
    elif isinstance(x, float):
        h.update(b'F' + struct.pack('<d', x))
    elif isinstance(x, int):
        h.update(b'I' + repr(x).encode() + b';')
    elif isinstance(x, (str, bytes)):
        h.update(type(x).__name__.encode() + repr(len(x)).encode() + b':' + (x if isinstance(x, bytes) else x.encode()))
    elif isinstance(x, (list, tuple)):
        h.update(type(x).__name__.encode() + b'[' + repr(len(x)).encode())
        for v in x:
            canon(v, h)
        h.update(b']')
    elif isinstance(x, dict):
        h.update(b'D' + repr(len(x)).encode())
        for k, v in x.items():
            canon(k, h)
            canon(v, h)
    elif dataclasses.is_dataclass(x):
        h.update(b'C' + type(x).__name__.encode())
        canon(x.__dict__, h)
    else:
        raise TypeError(f'canon: {type(x).__name__}')


def digest(x: Any) -> str:
    h = hashlib.sha1()
    canon(x, h)
    return h.hexdigest()


def files(args: list[str]) -> list[str]:
    out = []
    for a in args:
        if os.path.isdir(a):
            for dp, _, fn in os.walk(a):
                out += [os.path.join(dp, f) for f in sorted(fn) if f.lower().endswith('.hpd')]
        else:
            out.append(a)
    return out


def mesh(paths: list[str]) -> int:
    assert kymesh._native() is not None, 'the Rust core is not built'
    n = sectors = areas = bad = 0
    t_rust = t_ref = 0.0
    for p in paths:
        try:
            f = hpdtool.parse(open(p, 'rb').read())
        except Exception as e:                                  # noqa: BLE001 - not a path data file: noted
            print(f'  skipped {p}: {type(e).__name__}: {e}')
            continue
        n += 1
        for s in f.sectors:
            t = time.perf_counter()
            try:
                a = ('ok', digest(kymesh.parse_mesh(s.mesh)))
            except Exception as e:                              # noqa: BLE001 - the error is compared
                a = ('error', f'{type(e).__name__}: {e}')
            t_rust += time.perf_counter() - t
            t = time.perf_counter()
            try:
                m = kymesh.parse_mesh_reference(s.mesh)
                b = ('ok', digest(m))
                areas += len(m.areas)
            except Exception as e:                              # noqa: BLE001
                b = ('error', f'{type(e).__name__}: {e}')
            t_ref += time.perf_counter() - t
            sectors += 1
            if a != b:
                bad += 1
                print(f'  DIFFERENT {p} sector {s.id:#x}: {a} / {b}')
    print(f'mesh: {n} files, {sectors:,} sectors, {areas:,} areas: {bad} different '
          f'(Rust + digest {t_rust:.1f} s, reference + digest {t_ref:.1f} s)')
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    if len(argv) < 3 or argv[1] != 'mesh':
        print(__doc__)
        return 2
    return mesh(files(argv[2:]))


if __name__ == '__main__':
    sys.exit(main(sys.argv))
