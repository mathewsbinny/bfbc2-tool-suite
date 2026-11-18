r"""Round E3o's NavKit parity on real data (rust/BENCHMARKS.md "E3o"): the Rust reader of the runtime mesh
(kymesh.parse_mesh) against kymesh.parse_mesh_reference on every sector of the files given, and navdata.load (the
Rust cells of a whole file and union-find) against the code it replaced (main's navdata.py, run on the reference
reader). Read only: the files are only read; navdata runs without a cache.

  python rust\parity_e3o_nav.py mesh <file or folder>...      every .hpd under the folders
  python rust\parity_e3o_nav.py load <file or folder>... [--main]
                                          against navdata's reference paths (E3o's off), or with --main against
                                          main's navdata.py (git show main:core/bfbc2core/navdata.py)

Values are compared bit for bit: every float by its IEEE bytes, every array by dtype, shape and bytes, every
dataclass, list, tuple and dict by type and order (canon).
"""
import dataclasses
import hashlib
import importlib.util
import os
import struct
import subprocess
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


def head_navdata() -> Any:
    """main's navdata.py (git HEAD of this checkout's main), on the reference mesh reader."""
    src = subprocess.run(['git', '-C', SUITE, 'show', 'main:core/bfbc2core/navdata.py'], capture_output=True,
                         check=True).stdout.decode('utf-8')
    src = src.replace('from . import atomicio', 'from bfbc2core import atomicio')
    spec = importlib.util.spec_from_loader('navdata_main', loader=None)
    assert spec is not None
    mod = importlib.util.module_from_spec(spec)
    mod.__file__ = os.path.join(SUITE, 'core', 'bfbc2core', 'navdata.py')
    exec(compile(src, 'navdata_main.py', 'exec'), mod.__dict__)

    class RefMesh:                       # kymesh with the reference reader only
        parse_mesh = staticmethod(kymesh.parse_mesh_reference)
    mod.kymesh = RefMesh  # type: ignore[attr-defined]
    return mod


class _Reference:
    """navdata with round E3o's paths off - the code they replaced: the reference mesh reader, _cells_reference
    (navdiff.area_cells_many, E3h's Rust, per parsed area) and _union_roots_reference."""

    def __init__(self) -> None:
        from bfbc2core import navdata
        self.navdata = navdata

    def load(self, p: str) -> Any:
        nd = self.navdata
        nd._NATIVE[:] = [None]
        kymesh.use_native(False)
        try:
            return nd.load(p)
        finally:
            nd._NATIVE.clear()
            kymesh.use_native(True)


def load(paths: list[str], against_main: bool = False) -> int:
    from bfbc2core import navdata
    assert navdata._native() is not None, 'the Rust core is not built'
    old = head_navdata() if against_main else _Reference()
    bad = n = cells = 0
    t_new = t_old = 0.0
    for p in paths:
        t = time.perf_counter()
        a = navdata.load(p)
        t_new += time.perf_counter() - t
        da = digest(_fields(a))
        del a
        t = time.perf_counter()
        b = old.load(p)
        t_old += time.perf_counter() - t
        db = digest(_fields(b))
        n += 1
        cells += len(b.cell_area)
        same = da == db
        bad += not same
        print(f'  {"same" if same else "DIFFERENT"} {p} ({len(b.areas):,} areas, {len(b.cell_area):,} cells, '
              f'{len(b.node_pos):,} nodes, {len(b.islands)} islands)', flush=True)
        del b
    print(f'load: {n} files, {cells:,} cells: {bad} different (new {t_new:.1f} s, '
          f'{"main" if against_main else "reference"} {t_old:.1f} s)')
    return 1 if bad else 0


def _fields(nav: Any) -> Any:
    d = dict(nav.__dict__)
    d.pop('_adj', None)
    return (type(nav).__name__, d)


def main(argv: list[str]) -> int:
    if len(argv) < 3 or argv[1] not in ('mesh', 'load'):
        print(__doc__)
        return 2
    main_too = '--main' in argv
    paths = files([a for a in argv[2:] if a != '--main'])
    return mesh(paths) if argv[1] == 'mesh' else load(paths, main_too)


if __name__ == '__main__':
    sys.exit(main(sys.argv))
