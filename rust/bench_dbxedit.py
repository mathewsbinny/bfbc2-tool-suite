r"""DbxEdit in process (bfbc2core.dbxedit, the Rust core) against DbxEdit.exe: the time of each command, and a replay
of a day's real invocations from DbxEdit's log with both tools' outputs compared (rust\BENCHMARKS.md, "E3b").

  python rust\bench_dbxedit.py commands <small.dbx> <large.dbx> [--runs N]
        each command (xml, edit, import --like, check, roundtrip) on each file, N times both ways (process launch
        included for the exe): the best and the median
  python rust\bench_dbxedit.py replay <dbxedit-YYYY-MM-DD.log> [--before HH:MM:SS] [--limit N]
        every invocation the log started (DbxEdit.exe's own; not the in-process ones) whose inputs still exist, run
        both ways into a temp folder - outputs redirected there, inputs only read - and compared: exit code, stdout,
        stderr and every file written. Then the whole replay timed: the exe one by one, in process one by one, and
        in process as one batch on all cores (dbxedit.run_many).

Inputs are never written: an `xml` invocation's file is copied into the temp folder first (its XML is written beside
it), an edit's / import's / fixslots' output path is replaced by one in the temp folder.
"""
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbxedit  # noqa: E402

EXE = dbxedit.EXE


def exe_run(args: list[str]) -> tuple[int, str, str]:
    r = subprocess.run([EXE, *args], capture_output=True)
    return r.returncode, r.stdout.decode('utf-8', 'replace').replace('\r\n', '\n'), \
        r.stderr.decode('utf-8', 'replace').replace('\r\n', '\n')


def native_run(args: list[str]) -> tuple[int, str, str]:
    os.environ.pop(dbxedit.EXE_ENV, None)
    r = dbxedit.run(args)
    return r.returncode, r.stdout, r.stderr


# ------------------------------------------------------------------------------------------- commands

def commands(small: str, large: str, runs: int) -> None:
    tmp = tempfile.mkdtemp(prefix='bench_dbxedit_')
    try:
        print('| command | file | DbxEdit.exe (best / median) | in process (best / median) | faster |')
        print('|---|---|---:|---:|---:|')
        for label, src in (('small', small), ('large', large)):
            work = os.path.join(tmp, label)
            os.makedirs(work)
            f = os.path.join(work, 'f.dbx')
            shutil.copyfile(src, f)
            exe_run(['xml', f])
            xml = os.path.join(work, 'f.xml')
            guid = None
            from bfbc2core import dbx
            tree = dbx.parse(open(f, 'rb').read())
            for i in tree.instances:
                c = next((c for c in (i.children or []) if c.kind == 7 and len(c.raw) == 6 and c.attr('name')), None)
                if c is not None:
                    guid = f'setfield={i.attr("guid")}|{c.attr("name")}|42'
                    break
            jobs = [('xml', ['xml', f]),
                    ('edit (setfield)' if guid else 'edit (dropfield)',
                     ['edit', f, os.path.join(work, 'o.dbx'), guid or 'dropfield=Nothing']),
                    ('import --like', ['import', xml, os.path.join(work, 'i.dbx'), '--like', f]),
                    ('check', ['check', f]),
                    ('roundtrip', ['roundtrip', f])]
            for name, args in jobs:
                times = {}
                for way, fn in (('exe', exe_run), ('rust', native_run)):
                    ts = []
                    for _ in range(runs):
                        t = time.perf_counter()
                        fn(args)
                        ts.append(time.perf_counter() - t)
                    times[way] = (min(ts), statistics.median(ts))
                e, r = times['exe'], times['rust']
                print(f'| {name} | {label} ({os.path.getsize(src) / 1e3:.0f} KB) | {e[0] * 1e3:.1f} / {e[1] * 1e3:.1f} ms '
                      f'| {r[0] * 1e3:.2f} / {r[1] * 1e3:.2f} ms | {e[1] / r[1]:.0f}x |', flush=True)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# ------------------------------------------------------------------------------------------- replay

LINE = re.compile(r'^(\d{4}-\d\d-\d\d) (\d\d:\d\d:\d\d),\d+ INFO +--- dbxedit started: (.*) ---$')


def invocations(log: str, before: str | None) -> list[list[str]]:
    out = []
    for line in open(log, encoding='utf-8', errors='replace'):
        m = LINE.match(line.rstrip('\r\n'))
        if not m or (before and m.group(2) >= before):
            continue
        # the log joins the arguments with spaces; none of the day's paths has one
        out.append(m.group(3).split(' '))
    return out


def plan(args: list[str], d: str) -> list[str] | None:
    """The invocation rewritten to write only into d (inputs copied there where DbxEdit writes beside them), or
    None when an input is gone."""
    if not args:
        return None
    cmd = args[0]

    def exists(p: str) -> bool:
        return os.path.exists(p)
    if cmd == 'xml':
        paths = args[1:]
        if not paths or not all(os.path.isfile(p) for p in paths):
            return None                      # gone, or a folder (the unpack of a whole bundle: not copied)
        new = []
        for i, p in enumerate(paths):
            q = os.path.join(d, f'x{i}', os.path.basename(p))
            os.makedirs(os.path.dirname(q), exist_ok=True)
            shutil.copyfile(p, q)
            new.append(q)
        return ['xml', *new]
    if cmd in ('roundtrip', 'check'):
        files = [a for a in args[1:] if not a.startswith('--')]
        return list(args) if all(exists(p) for p in files) else None
    if cmd == 'edit' and len(args) >= 3:
        if not os.path.isfile(args[1]):
            return None
        for c in args[3:]:
            if c.startswith('@') and not os.path.isfile(c[1:]):
                return None
        return ['edit', args[1], os.path.join(d, 'out.dbx'), *args[3:]]
    if cmd == 'import' and len(args) >= 3:
        if not os.path.isfile(args[1]):
            return None
        rest = args[3:]
        for i, a in enumerate(rest[:-1]):
            if a in ('--like', '--types') and not exists(rest[i + 1]):
                return None
        return ['import', args[1], os.path.join(d, 'out.dbx'), *rest]
    if cmd == 'fixslots' and len(args) >= 5:
        if not (os.path.isfile(args[1]) and os.path.isfile(args[4])):
            return None
        return ['fixslots', args[1], os.path.join(d, 'out.dbx'), *args[3:]]
    return list(args)                        # a usage error: nothing read or written


def files_in(d: str) -> dict[str, bytes]:
    out = {}
    for top, _, fs in os.walk(d):
        for f in fs:
            p = os.path.join(top, f)
            out[os.path.relpath(p, d)] = open(p, 'rb').read()
    return out


def replay(log: str, before: str | None, limit: int | None) -> None:
    runs = invocations(log, before)
    tmp = tempfile.mkdtemp(prefix='replay_dbxedit_')
    kinds: dict[str, list[int]] = {}
    planned: list[tuple[str, list[str], list[str], list[str]]] = []
    skipped = 0
    try:
        for n, args in enumerate(runs[:limit] if limit else runs):
            base = os.path.join(tmp, f'{n:05d}')
            ways = {}
            for way in ('exe', 'rust'):
                d = os.path.join(base, way)
                os.makedirs(d)
                ways[way] = plan(args, d)
            a_exe, a_rust = ways['exe'], ways['rust']
            if a_exe is None or a_rust is None:
                skipped += 1
                shutil.rmtree(base, ignore_errors=True)
                continue
            planned.append((base, args, a_exe, a_rust))
        same = differ = 0
        examples: list[str] = []
        t_exe = t_rust = 0.0
        for base, _, a_exe, a_rust in planned:
            cmd = a_exe[0] if a_exe else '?'
            t = time.perf_counter()
            e = exe_run(a_exe)
            t_exe += time.perf_counter() - t
            t = time.perf_counter()
            r = native_run(a_rust)
            t_rust += time.perf_counter() - t
            de, dr = os.path.join(base, 'exe'), os.path.join(base, 'rust')
            fe, fr = files_in(de), files_in(dr)
            norm = (lambda s, d: re.sub(r'\.tmp-\d+(-\d+)?', '.tmp-N', s.replace(d, '<dir>')))
            ok = (e[0] == r[0] and norm(e[1], de) == norm(r[1], dr) and norm(e[2], de) == norm(r[2], dr) and fe == fr)
            k = kinds.setdefault(cmd, [0, 0])
            k[0] += 1
            if ok:
                same += 1
            else:
                differ += 1
                k[1] += 1
                if len(examples) < 10:
                    examples.append(f'{" ".join(a_exe)[:200]}: exit {e[0]}/{r[0]}, stdout {e[1][:80]!r} / {r[1][:80]!r}, '
                                    f'files {sorted(fe) == sorted(fr)}')
        print(f'invocations in the log: {len(runs)}; replayed: {len(planned)} (inputs gone: {skipped})')
        for cmd, (n, bad) in sorted(kinds.items()):
            print(f'  {cmd:10} {n:5} replayed, {bad} different')
        print(f'identical (exit code, stdout, stderr, files): {same}; different: {differ}')
        for x in examples:
            print('  ' + x)
        # the whole replay again, in process as one batch on all cores (fresh output folders)
        batch = []
        for base, args, _, _ in planned:
            d = os.path.join(base, 'batch')
            os.makedirs(d)
            batch.append(plan(args, d) or [])
        t = time.perf_counter()
        results = dbxedit.run_many(batch)
        t_batch = time.perf_counter() - t
        agree = sum(1 for (base, _, _, _), res in zip(planned, results)
                    if files_in(os.path.join(base, 'batch')) == files_in(os.path.join(base, 'rust')))
        print(f'time for the whole replay: DbxEdit.exe {t_exe:.1f} s, in process one by one {t_rust:.1f} s '
              f'({t_exe / max(t_rust, 1e-9):.0f}x), in process as one batch {t_batch:.2f} s '
              f'({t_exe / max(t_batch, 1e-9):.0f}x); the batch\'s files the one-by-one run\'s: {agree}/{len(planned)}')
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main(argv: list[str]) -> int:
    if not os.path.isfile(EXE):
        print(f'DbxEdit.exe is not built ({EXE}): bfbc2-dbx-tools\\build.bat')
        return 1
    if dbxedit.use_exe():
        print('the Rust core is not available (python rust\\build_native.py), or BFBC2_DBXEDIT_EXE is set')
        return 1
    if len(argv) >= 4 and argv[1] == 'commands':
        runs = int(argv[argv.index('--runs') + 1]) if '--runs' in argv else 20
        commands(argv[2], argv[3], runs)
        return 0
    if len(argv) >= 3 and argv[1] == 'replay':
        before = argv[argv.index('--before') + 1] if '--before' in argv else None
        limit = int(argv[argv.index('--limit') + 1]) if '--limit' in argv else None
        replay(argv[2], before, limit)
        return 0
    print(__doc__)
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv))
