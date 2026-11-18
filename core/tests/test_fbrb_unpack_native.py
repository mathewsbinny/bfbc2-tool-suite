r"""The in-process unpack (rust/bc2-formats fbrb::unpack through the bc2_native module, bfbc2core.fbrb.unpack)
against what FbrbTool's unpack wrote before FbrbTool.exe was retired (round R4, 2026-11-18: csharp_golden.py): the
same files (names with their case, bytes) and the same fbrb.manifest; for a bundle FbrbTool refused, a refusal with
the same files left behind.

  python core\tests\test_fbrb_unpack_native.py          quick: built bundles (zipped, stored, every padded layout
                                                         FbrbTool's manifest names, markers, folders, an empty one),
                                                         odd bundles (odd names, two entries on one file, a file
                                                         where a folder is needed, extra gzip bytes) and damaged ones
                                                         (every cut of a small bundle): FbrbTool's files or its
                                                         failure (fbrbtool_golden.json), in process and through
                                                         bfbc2core.fbrb.unpack; 8 small game bundles when the game
                                                         copy is there
  python core\tests\test_fbrb_unpack_native.py --game   + every bundle of the game copy whose payload is at most
                                                         BFBC2_UNPACK_TEST_MAX_MB (default 300; memory), against
                                                         fbrbtool_golden_game.json by the bundle's SHA-1
                                                         (BFBC2_UNPACK_TEST_MIN_MB: only those from that size)

Skipped (exit 0) when the module is not built.
"""
import os
import random
import shutil
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
from bfbc2core import fbrb, native, paths  # noqa: E402

if HERE not in sys.path:
    sys.path.insert(0, HERE)
import csharp_golden as cg  # noqa: E402

NATIVE: Any = native.load()
SMALL = 8
LINE0 = '# fbrb.manifest - entry order matters. name|type|flags(hex)|size2|size  zipped='


def compare(bundle: str, root: str, want: list[Any] | None) -> str:
    """'identical' (unpacked to the files FbrbTool.exe wrote), 'refused' (FbrbTool refused it and so does Rust,
    leaving the same files behind; bfbc2core.fbrb.unpack raises), or what differs. want: the recording
    [input digest, FbrbTool's exit code, the folder's digest]; the input digest is not compared (fbrb.build's gzip
    headers carry the time they were made: the bytes differ from run to run, never the entries)."""
    if want is None:
        return 'not recorded'
    b, c = (os.path.join(root, x) for x in ('rust', 'api'))
    for x in (b, c):
        shutil.rmtree(x, ignore_errors=True)
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
        if want[1] == 0:
            return f'Rust failed where FbrbTool unpacked it: {rust_err}'
        if cg.tree_digest(b) != want[2]:
            return 'Rust and FbrbTool fail, leaving different files'
        return 'refused' if api_err is not None else f'FbrbTool failed, the API did not ({api_err})'
    if want[1] != 0:
        return 'Rust unpacked a bundle FbrbTool refused'
    if cg.tree_digest(b) != want[2]:
        return 'differs from FbrbTool\'s files'
    if api_err is not None or cg.tree_digest(c) != want[2]:
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
    want = cg.load('fbrbtool_golden.json').get('unpack_built', {})
    try:
        built = built_bundles(root, rng)
        outcomes = {label: compare(p, os.path.join(root, 'cmp'), want.get(label)) for label, p in built}
        bad = {k: v for k, v in outcomes.items() if v != 'identical'}
        check(not bad, f'{len(built)} built bundles (zipped, stored, every padded layout and token): unpacked in '
              'process to FbrbTool\'s files and manifest (fbrbtool_golden.json)', str(bad)[:600])
        odd = odd_bundles(root, rng)
        outcomes = {label: compare(p, os.path.join(root, 'cmp'), want.get(label)) for label, p in odd}
        bad = {k: v for k, v in outcomes.items() if v not in ('refused', 'identical')}
        refused = sum(v == 'refused' for v in outcomes.values())
        check(not bad, f'{len(odd)} odd and damaged bundles: {len(odd) - refused} unpacked in process as FbrbTool did, '
              f'{refused} refused as FbrbTool refused them (the same files left behind)', str(bad)[:600])
        check(outcomes['a byte after the payload stream'] == 'identical' and outcomes['two entries on one file (case)']
              == 'identical', 'extra bytes after the payload and two entries on one file: unpacked in process as '
              'FbrbTool unpacked them (round E3k)')
        b = built[0][1]
        got = fbrb.unpack(b, os.path.join(root, 'api'))
        check(got == 'rust' and cg.tree_digest(os.path.join(root, 'api')) == want[built[0][0]][2],
              'bfbc2core.fbrb.unpack runs in process (FbrbTool.exe, the fallback before, is retired): FbrbTool\'s files',
              got)
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
    t_rust = 0.0
    recorded = cg.load('fbrbtool_golden_game.json').get('bundles', {})
    try:
        for b in bundles:
            if not floor <= payload_size(b) <= cap:
                skipped += 1
                continue
            want = recorded.get(cg.sha1_file(b), {}).get('unpack')
            if want is None:
                outcomes['not recorded'] += 1
                examples.setdefault('not recorded', b)
                continue
            r = os.path.join(root, 'rust')
            shutil.rmtree(r, ignore_errors=True)
            t1 = time.perf_counter()
            try:
                NATIVE.fbrb_unpack(b, r)
                err = None
            except (ValueError, OSError) as e:
                err = e
            t_rust += time.perf_counter() - t1
            if err is not None:
                outcome = 'refused (FbrbTool unpacked it)' if want[0] == 0 else 'refused (FbrbTool refused too)'
            elif want[0] != 0:
                outcome = 'Rust unpacked a bundle FbrbTool refused'
            else:
                outcome = 'identical' if cg.tree_digest(r) == want[3] else 'DIFFERENT'
            outcomes[outcome] += 1
            examples.setdefault(outcome, b)
    finally:
        shutil.rmtree(root, ignore_errors=True)
    for k, n in outcomes.items():
        print(f'       {n} x {k}' + ('' if k == 'identical' else f', e.g. {examples[k]}'))
    bad = (outcomes['DIFFERENT'] + outcomes['Rust unpacked a bundle FbrbTool refused']
           + outcomes['refused (FbrbTool unpacked it)'])
    run = sum(outcomes.values())
    check(not bad and outcomes['not recorded'] <= run // 2,
          f'{label}: {run - outcomes["not recorded"]} of {run} bundles recorded, unpacked in Rust: '
          f'{outcomes["identical"]} identical to FbrbTool.exe\'s files (fbrbtool_golden_game.json), {bad} different; '
          f'{skipped} with payloads outside {floor >> 20} - {cap >> 20} MiB not run '
          f'(unpack time Rust {t_rust:.0f} s; {time.time() - t:.0f} s wall)',
          f'{bad} different' if bad else 'more than half of the bundles were not recorded')


def main(argv: list[str]) -> int:
    native.require('the bundle unpack', 'fbrb_unpack')
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
