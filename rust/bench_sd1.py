r"""Round SD1 timings: how shaderdb.py finds and opens a database (ShaderDb.parse's dispatch, pick_database,
dx_from_name, load / load_levels), on the game copy (read only). Best of 5 per figure (load_levels: best of 3), each
checkout in a process of its own; peak working set at the end.

  python rust\bench_sd1.py [--suite <checkout>] [--python]

--suite: the checkout to time (default this one; the round's "before" is a frozen copy of its starting commit);
--python: BFBC2_PURE_PYTHON=1 (shaderdb.py's Python reader and dispatch).
"""
import ctypes
import os
import sys
import time
from typing import Any

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if '--suite' in sys.argv:
    SUITE = os.path.abspath(sys.argv[sys.argv.index('--suite') + 1])
if '--python' in sys.argv:
    os.environ['BFBC2_PURE_PYTHON'] = '1'
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'core')
import shaderdb as S  # noqa: E402


def best(fn: Any, n: int = 5) -> float:
    out = []
    for _ in range(n):
        t = time.perf_counter()
        fn()
        out.append(time.perf_counter() - t)
    return min(out)


def peak_mb() -> float:
    class PMC(ctypes.Structure):
        _fields_ = [('cb', ctypes.c_ulong), ('PageFaultCount', ctypes.c_ulong), ('PeakWorkingSetSize', ctypes.c_size_t),
                    ('WorkingSetSize', ctypes.c_size_t), ('QuotaPeakPagedPoolUsage', ctypes.c_size_t),
                    ('QuotaPagedPoolUsage', ctypes.c_size_t), ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t),
                    ('QuotaNonPagedPoolUsage', ctypes.c_size_t), ('PagefileUsage', ctypes.c_size_t),
                    ('PeakPagefileUsage', ctypes.c_size_t)]
    from ctypes import wintypes
    c = PMC()
    c.cb = ctypes.sizeof(c)
    k32 = ctypes.WinDLL('kernel32')
    k32.GetCurrentProcess.restype = wintypes.HANDLE
    k = ctypes.WinDLL('psapi')
    k.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(PMC), wintypes.DWORD]
    k.GetProcessMemoryInfo(k32.GetCurrentProcess(), ctypes.byref(c), c.cb)
    return c.PeakWorkingSetSize / 2 ** 20


def main() -> int:
    from bfbc2core import fbrb
    root = S.find_opfor(None)
    bundles = S.level_bundles(root)
    levels = sorted(bundles)
    b = bundles['mp_005']
    names = {dx: S.pick_database(b, dx) for dx in (9, 10, 11)}
    data = fbrb.read(b, sorted(set(names.values())))
    d9, d10 = data[names[9]], data[names[10]]
    cut = d10[:len(d10) // 2]
    rows = [
        ('parse mp_005 Dx9, dx=9', lambda: S.ShaderDb.parse(d9, 9, 'x')),
        ('parse mp_005 Dx10, no dx given (Dx9 tried first)', lambda: S.ShaderDb.parse(d10, None, 'x')),
        ('parse mp_005 Dx10 cut in half, no dx (both layouts fail)', lambda: _err(lambda: S.ShaderDb.parse(cut, None, 'x'))),
        ('pick_database mp_005 Dx10 (x100)', lambda: [S.pick_database(b, 10) for _ in range(100)]),
        ('dx_from_name (x10,000)', lambda: [S.dx_from_name('levels/mp_005/shaders/Dx11_Single.res')
                                            for _ in range(10_000)]),
        ('load level:mp_005 Dx11', lambda: S.load('level:mp_005', 11)),
    ]
    print(f'checkout {SUITE}; backend {S.backend()}')
    for label, fn in rows:
        print(f'{label}: {best(fn) * 1000:.2f} ms', flush=True)
    t = best(lambda: [None for _ in S.load_levels(levels, (9, 10, 11))], 3)
    print(f'load_levels, {len(levels)} levels x Dx9 / 10 / 11: {t:.2f} s', flush=True)
    print(f'peak working set: {peak_mb():.0f} MB')
    return 0


def _err(fn: Any) -> None:
    try:
        fn()
    except S.ShaderDbError:
        return
    raise AssertionError('expected a ShaderDbError')


if __name__ == '__main__':
    sys.exit(main())
