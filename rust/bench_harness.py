r"""Round E4f's timings (rust/BENCHMARKS.md "E4f"): the harness's and the crash-reader tools' work with the Rust core
(rust\bc2-harness) and with the Python reference, on Project OPFOR BFBC2's opfor.log and BFBC2Game.exe (read only).

  python rust\bench_harness.py [--rounds 2]

Times are in process (no interpreter start-up), best of the rounds; every pair's answers are compared too.
"""
import os
import sys
import time

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools', 'navkit', 'core', 'harness', 'crash-reader')
import crashreader  # noqa: E402
import exeref  # noqa: E402
import harnesscore  # noqa: E402
from testharness import ingame  # noqa: E402


def best(fn, rounds):
    out, t = None, float('inf')
    for _ in range(rounds):
        t0 = time.perf_counter()
        out = fn()
        t = min(t, time.perf_counter() - t0)
    return out, t


def main(argv: list[str]) -> int:
    rounds = int(argv[argv.index('--rounds') + 1]) if '--rounds' in argv else 2
    exe = crashreader.find_exe()
    if not exe or harnesscore.native() is None:
        print('needs BFBC2Game.exe of Project OPFOR BFBC2 and the Rust core')
        return 1
    log = os.path.join(os.path.dirname(exe), 'logs', 'opfor.log')
    rows = []

    def pair(name, native, python):
        harnesscore.use_native(True)
        a, ta = best(native, rounds)
        harnesscore.use_native(False)
        b, tb = best(python, rounds)
        harnesscore.use_native(True)
        rows.append((name, tb, ta, a == b))
        print(f'{name:60s} Python {tb:7.2f} s   Rust {ta:7.2f} s   x{tb / ta if ta else 0:5.1f}   same: {a == b}', flush=True)

    if os.path.isfile(log):
        def py_sessions():
            with open(log, encoding='utf-8', errors='replace') as fh:
                return ingame.sessions_python(fh.read())
        pair(f'import-log: sessions of opfor.log ({os.path.getsize(log) / 1e6:.0f} MB, read included)',
             lambda: ingame.sessions_of_file(log), py_sessions)
        m = os.path.getmtime(log)
        pair('crash reader: the log lines before a crash (3 lookups)',
             lambda: [crashreader.log_before_crash(log), crashreader.log_before_crash(log, when=m),
                      crashreader.log_before_crash(log, 40, when=m - 3600)],
             lambda: [crashreader.log_before_crash_python(log), crashreader.log_before_crash_python(log, when=m),
                      crashreader.log_before_crash_python(log, 40, when=m - 3600)])
    syms = crashreader.read_symbols([os.path.join(os.path.dirname(crashreader.__file__), 'symbols_795745.txt')])

    def queries():
        e = exeref.Exe(exe)
        out: list[object] = []
        for text, exact in (('Server.UnrestrictedUnlocks', True), ('presenceRequestOnline', True), ('Kynapse', False)):
            for va, s in e.find_strings(text, exact)[:5]:
                out.append((va, s, [(a, i.mnemonic, i.op_str, k) for a, i, k in e.refs(va, 40)], e.data_refs(va)))
        for va in (0x8527D0, 0x6EFBC0, 0x95B7D3):
            out.append([(a, i.mnemonic, i.op_str, k) for a, i, k in e.refs(va, 40)])
            out.append(exeref.window(e, va, 0x40, 0x40, syms))
        return out
    pair('exeref: load, 3 string searches, their refs, 3 refs and windows', queries, queries)

    def code_view():
        img = crashreader.Image(exe)
        out: list[object] = []
        lo, hi = img.code[0]
        for a in range(lo + 0x123457, hi, 0x7F3A1)[:60]:
            s, ins, at = img.around(a)
            out.append((s, [(x.address, x.mnemonic, x.op_str) for x in ins], at))
            c = img.call_before(a)
            out.append(None if c is None else (c.address, c.op_str))
            out.append(crashreader.name_of(a, syms, img))
        return out
    pair('crash reader: load, code view, call before and name at 60 addresses', code_view, code_view)
    print(f'{sum(r[3] for r in rows)} of {len(rows)} the same')
    return 0 if all(r[3] for r in rows) else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))
