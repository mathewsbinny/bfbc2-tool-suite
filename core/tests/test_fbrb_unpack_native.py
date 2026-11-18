r"""The in-process unpack (rust/bc2-formats fbrb::unpack through the bc2_native module, bfbc2core.fbrb.unpack)
against FbrbTool's unpack, which it replaces: the same files (names with their case, bytes) and the same
fbrb.manifest; for a bundle FbrbTool refuses, its failure with the same files left behind.

  python core\tests\test_fbrb_unpack_native.py          quick: built bundles (zipped, stored, every padded layout
                                                         FbrbTool's manifest names, markers, folders, an empty one),
                                                         odd bundles (odd names, two entries on one file, a file
                                                         where a folder is needed, extra gzip bytes) and damaged ones
                                                         (every cut of a small bundle): FbrbTool's files or its
                                                         failure, in process and through bfbc2core.fbrb.unpack; 8
                                                         small game bundles when the game copy is there
  python core\tests\test_fbrb_unpack_native.py --game   + every bundle of the game copy whose payload is at most
                                                         BFBC2_UNPACK_TEST_MAX_MB (default 300; memory), both ways
                                                         (BFBC2_UNPACK_TEST_MIN_MB: only those from that size)

Skipped (exit 0) when the module or FbrbTool.exe is not built.
"""
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time
from collections import Counter
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx, fbrb, paths  # noqa: E402

NATIVE: Any = dbx._import_native()
NATIVE = NATIVE if NATIVE is not None and hasattr(NATIVE, 'fbrb_unpack') else None
TOOL = os.path.join(SUITE, 'bfbc2-fbrb-tool', 'FbrbTool.exe')
SMALL = 8
LINE0 = '# fbrb.manifest - entry order matters. name|type|flags(hex)|size2|size  zipped='


def tool(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([TOOL, *args], capture_output=True, text=True, errors='replace')


def tree(d: str) -> dict[str, tuple[int, bytes]]:
    """{relative path as stored (case kept): (size, SHA-256)} of every file under d (hashes, not the bytes: a large
    bundle's files would hold its whole payload in memory twice)."""
    import hashlib
    out = {}
    for dp, _ds, fs in os.walk(d):
        for f in fs:
            p = os.path.join(dp, f)
            with open(p, 'rb') as fh:
                data = fh.read()
            out[os.path.relpath(p, d)] = (len(data), hashlib.sha256(data).digest())
    return out


def compare(bundle: str, root: str) -> str:
    """'identical' (both unpacked the same), 'refused' (FbrbTool fails on it and so does Rust, leaving the same files
    behind; bfbc2core.fbrb.unpack raises), or what differs. Since round E3k the in-process unpack is FbrbTool's
    command line's engine: it takes every bundle FbrbTool takes."""
    a, b, c = (os.path.join(root, x) for x in ('tool', 'rust', 'api'))
    for x in (a, b, c):
        shutil.rmtree(x, ignore_errors=True)
    r = tool('unpack', bundle, a)
    try:
        NATIVE.fbrb_unpack(bundle, b)
        rust_err = None
    except (ValueError, OSError) as e:
        rust_err = e
    try:
        fbrb.unpack(bundle, c)
        api_err = None
    except (ValueError, OSError) as e:
        api_err = e
    if rust_err is not None:
        if r.returncode == 0:
            return f'Rust failed where FbrbTool unpacks it: {rust_err}'
        left_a = tree(a) if os.path.isdir(a) else {}
        left_b = tree(b) if os.path.isdir(b) else {}
        if left_a != left_b:
            return f'Rust and FbrbTool fail, leaving different files: {sorted(set(left_a) ^ set(left_b))[:3]}'
        return 'refused' if api_err is not None else f'FbrbTool failed, the API did not ({api_err})'
    if r.returncode != 0:
        return f'Rust unpacked a bundle FbrbTool refuses: {r.stderr.strip()[-200:]}'
    ta, tb, tc = tree(a), tree(b), tree(c)
    if ta != tb:
        only = sorted(set(ta) ^ set(tb))[:3]
        diff = [k for k in ta if k in tb and ta[k] != tb[k]][:3]
        return f'differs: names {only}, bytes {diff}'
    if api_err is not None or tc != ta:
        return f'the API differs ({api_err})'
    return 'identical'


def build(items: list[tuple[str, str, bytes]], zipped: bool) -> bytes:
    return fbrb.build(items, zipped)


def built_bundles(root: str, rng: random.Random) -> list[tuple[str, str]]:
    """(label, bundle) of hand-made bundles: the layouts FbrbTool unpacks (padded ones packed by the proven packer
    from manifests naming them)."""
    out: list[tuple[str, str]] = []

    def put(label: str, data: bytes) -> None:
        p = os.path.join(root, f'b{len(out)}.fbrb')
        open(p, 'wb').write(data)
        out.append((label, p))

    blob = lambda n: bytes(rng.randrange(256) for _ in range(n))  # noqa: E731
    common = [('Levels/A.dbx', '<non-resource>', blob(5000)), ('Gone', '*deleted*', b''),
              ('Levels/sub/B.res', 'RigidMeshSet', blob(3000)), ('levels/C.res', 'RigidMeshSet', blob(10)),
              ('x\\y.dbx', 'T', b'z'), ('empty.bin', 'T', b'')]
    for z in (True, False):
        put(f'{"zipped" if z else "stored"}, folders, markers, an empty entry, a backslash', build(common, z))
        put(f'{"zipped" if z else "stored"}, markers only', build([('g', '*deleted*', b'')], z))
        put(f'{"zipped" if z else "stored"}, no entries', build([], z))
    for lay in ('align4096', 'gap64'):
        for hdr in (4096, 4, 1):
            for extra in ('', ' plen=size2', ' size2=minus64', ' plen=size2 size2=minus64'):
                d = os.path.join(root, f'f{len(out)}')
                os.makedirs(d)
                rows = [('a.res', blob(3000)), ('b.res', blob(5000)), ('c.res', blob(4096))]
                lines = [LINE0 + f'0 layout={lay} hdr={hdr}{extra}']
                for i, (n, data) in enumerate(rows):
                    open(os.path.join(d, n), 'wb').write(data)
                    s2 = len(data) - 64 if 'minus64' in extra else len(data)
                    lines.append(f'{n}|T|00010000|{s2}|{len(data)}')
                    if i == 0:
                        lines.append('gone|*deleted*|00000000|0|0')
                open(os.path.join(d, 'fbrb.manifest'), 'w').write('\n'.join(lines) + '\n')
                p = os.path.join(root, f'b{len(out)}.fbrb')
                NATIVE.fbrb_pack(d, p)
                out.append((f'{lay} hdr={hdr}{extra}', p))
    return out


def odd_bundles(root: str, rng: random.Random) -> list[tuple[str, str]]:
    """(label, bundle) of odd and damaged bundles (FbrbTool unpacks some, refuses others; so does the port)."""
    out: list[tuple[str, str]] = []

    def put(label: str, data: bytes) -> None:
        p = os.path.join(root, f'o{len(out)}.fbrb')
        open(p, 'wb').write(data)
        out.append((label, p))

    blob = lambda n: bytes(rng.randrange(256) for _ in range(n))  # noqa: E731
    for name in ('a/./b.dbx', 'a/../b.dbx', '../escape.dbx', 'a/b. ', 'trailing.', ' lead', 'con.dbx', 'a:b',
                 'a//b', '/rooted', 'q?.dbx', 'x' * 300):
        put(f'name {name[:20]!r}', build([(name, 'T', b'data')], True))
    put('two entries on one file (case)', build([('A.dbx', 'T', b'1'), ('a.DBX', 'T', b'22')], True))
    put('a file where a folder is needed', build([('a', 'T', b'1'), ('a/b', 'T', b'2')], True))
    good = build([('a.dbx', 'T', blob(2000))], True)
    put('a byte after the payload stream', good + b'\0')
    put('a second gzip member', good + good[-30:])
    stored = build([('a.dbx', 'T', blob(200))], False)
    for k in sorted({len(good) * i // 23 for i in range(23)} | {len(good) - 1, len(good) - 4, 9, 12}):
        put(f'zipped cut at {k}', good[:k])
    for k in (len(stored) - 1, len(stored) - 100, 20):
        put(f'stored cut at {k}', stored[:k])
    bad = bytearray(good)
    bad[-6] ^= 0xFF
    put('a wrong payload checksum', bytes(bad))
    put('not a bundle', b'NotFbRB....')
    return out


def check_built(check: Any) -> None:
    rng = random.Random(11)
    root = tempfile.mkdtemp(prefix='fbrb_unpack_')
    try:
        built = built_bundles(root, rng)
        outcomes = {label: compare(p, os.path.join(root, 'cmp')) for label, p in built}
        bad = {k: v for k, v in outcomes.items() if v != 'identical'}
        check(not bad, f'{len(built)} built bundles (zipped, stored, every padded layout and token): unpacked in '
              'process to FbrbTool\'s files and manifest', str(bad)[:600])
        odd = odd_bundles(root, rng)
        outcomes = {label: compare(p, os.path.join(root, 'cmp')) for label, p in odd}
        bad = {k: v for k, v in outcomes.items() if v not in ('refused', 'identical')}
        refused = sum(v == 'refused' for v in outcomes.values())
        check(not bad, f'{len(odd)} odd and damaged bundles: {len(odd) - refused} unpacked in process as FbrbTool does, '
              f'{refused} refused as FbrbTool refuses them (the same files left behind)', str(bad)[:600])
        check(outcomes['a byte after the payload stream'] == 'identical' and outcomes['two entries on one file (case)']
              == 'identical', 'extra bytes after the payload and two entries on one file: unpacked in process as '
              'FbrbTool unpacks them (round E3k)')
        # without the Rust core: FbrbTool.exe
        b = built[0][1]
        fbrb.use_native(False)
        try:
            got = fbrb.unpack(b, os.path.join(root, 'py'))
        finally:
            fbrb.use_native(True)
        a = os.path.join(root, 'rs')
        NATIVE.fbrb_unpack(b, a)
        check(got != 'rust' and tree(os.path.join(root, 'py')) == tree(a),
              'use_native(False): bfbc2core.fbrb.unpack runs FbrbTool.exe, the same files', got)
    finally:
        shutil.rmtree(root, ignore_errors=True)


def payload_size(bundle: str) -> int:
    try:
        return max((e.offset + e.size for e in fbrb.entries(bundle)), default=0)
    except (ValueError, OSError):
        return 0


def game_bundles(root: str) -> list[str]:
    out = []
    for d, _, names in os.walk(os.path.join(root, 'game')):
        out += [os.path.join(d, n) for n in names if n.lower().endswith('.fbrb')]
    return sorted(out)


def run_bundles(bundles: list[str], check: Any, label: str) -> None:
    cap = int(os.environ.get('BFBC2_UNPACK_TEST_MAX_MB', '300')) << 20
    floor = int(os.environ.get('BFBC2_UNPACK_TEST_MIN_MB', '0')) << 20     # a second run for the large ones
    t = time.time()
    root = tempfile.mkdtemp(prefix='fbrb_unpack_g_')
    outcomes: Counter[str] = Counter()
    examples: dict[str, str] = {}
    skipped = 0
    files = 0
    t_tool = t_rust = 0.0
    try:
        for b in bundles:
            if not floor <= payload_size(b) <= cap:
                skipped += 1
                continue
            a, r = os.path.join(root, 'tool'), os.path.join(root, 'rust')
            shutil.rmtree(a, ignore_errors=True)
            shutil.rmtree(r, ignore_errors=True)
            t0 = time.perf_counter()
            p = tool('unpack', b, a)
            t1 = time.perf_counter()
            try:
                NATIVE.fbrb_unpack(b, r)
                err = None
            except (ValueError, OSError) as e:
                err = e
            t2 = time.perf_counter()
            t_tool += t1 - t0
            t_rust += t2 - t1
            if err is not None:
                outcome = 'refused (FbrbTool unpacks it)' if p.returncode == 0 else 'refused (FbrbTool refuses too)'
            elif p.returncode != 0:
                outcome = 'Rust unpacked a bundle FbrbTool refuses'
            else:
                ta, tr = tree(a), tree(r)
                files += len(ta)
                outcome = 'identical' if ta == tr else 'DIFFERENT'
            outcomes[outcome] += 1
            examples.setdefault(outcome, b)
    finally:
        shutil.rmtree(root, ignore_errors=True)
    for k, n in outcomes.items():
        print(f'       {n} x {k}' + ('' if k == 'identical' else f', e.g. {examples[k]}'))
    bad = (outcomes['DIFFERENT'] + outcomes['Rust unpacked a bundle FbrbTool refuses']
           + outcomes['refused (FbrbTool unpacks it)'])
    check(not bad, f'{label}: {sum(outcomes.values())} bundles unpacked both ways ({files} files): '
          f'{outcomes["identical"]} identical, {bad} different; {skipped} with payloads outside {floor >> 20} - '
          f'{cap >> 20} MiB not run '
          f'(unpack time FbrbTool {t_tool:.0f} s, Rust {t_rust:.0f} s; {time.time() - t:.0f} s wall)')


def main(argv: list[str]) -> int:
    if NATIVE is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py); bundles are unpacked by FbrbTool')
        return 0
    if not os.path.isfile(TOOL):
        print('skipped: FbrbTool.exe is not built (bfbc2-fbrb-tool\\build.bat): nothing to compare the unpack with')
        return 0
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    check_built(check)
    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the game bundles (no Project OPFOR BFBC2 game copy)')
    else:
        bundles = game_bundles(root)
        if '--game' in argv:
            run_bundles(bundles, check, 'game copy')
        else:
            small = sorted(bundles, key=os.path.getsize)
            pick = random.Random(3).sample(small[:len(small) // 2], min(SMALL, len(small)))
            run_bundles(pick, check, f'{len(pick)} small game bundles')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
