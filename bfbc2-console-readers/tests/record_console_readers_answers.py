r"""Records the answers of the console readers' Python twins - stfs.py (Package._open_reference / read_reference,
main) and xisolib.py (find_partition_reference / walk_reference) - in console_readers_answers.json, for
test_console_readers.py and test_console_readers_native.py, which hold the Rust core against them and import neither
twin (so the twins can be deleted). Run it while the twins are there; the inputs are the tests' own, built from fixed
seeds (no game content is written: the user's Xbox files, with --game, are recorded as digests only).

  python record_console_readers_answers.py           the built and damaged packages and discs, the entry damaged to
                                                     millions of blocks, `stfs.py`'s printed lines (the --game
                                                     answers already in the file are kept)
  python record_console_readers_answers.py --game [--xbox <folder>]
                                                     + the user's disc image and Update packages (read in Python: a
                                                     few minutes)

A package whose folders never reach the root makes stfs.py's path loop run for ever: the recorder finds those without
running that loop (the twin's own header and file-table reads, then the parent chains walked with a guard) and records
them as 'cycle'; where the Rust core is there, its stfs_refusal must say 'cycle' for exactly those.
"""
import contextlib
import hashlib
import io
import json
import os
import shutil
import struct
import sys
import tempfile
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests'), HERE):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('core', 'xiso', 'stfs')
import stfs  # noqa: E402
import test_console_readers as tcr  # noqa: E402
import test_console_readers_native as tn  # noqa: E402
import xisolib as xiso  # noqa: E402
from bfbc2core import native  # noqa: E402
from f0_parity import stdout_utf8  # noqa: E402

SECTOR = tcr.SECTOR


def loops_for_ever(path: str) -> bool:
    """Whether stfs.Package's path loop never ends for this package: its own header and file-table reads (the twin's
    code, with the table's parse left out), then each entry's parent chain walked with a guard."""
    p = stfs.Package.__new__(stfs.Package)
    p.f = open(path, 'rb')
    p._reader = None
    setattr(p, '_file_table', list)          # noqa: B010 - the header only: an empty table
    try:
        p._open_reference()
        raw = b''
        for b in p._blocks(p.ft_start, p.ft_blocks, False):
            p.f.seek(p._addr(b))
            raw += p.f.read(stfs.BLOCK)
        parents = []
        for i in range(0, len(raw), 0x40):
            e = raw[i:i + 0x40]
            if not e[0x28] & 0x3F:
                break
            parents.append(struct.unpack_from('>H', e, 0x32)[0])
            struct.unpack_from('>I', e, 0x34)          # the twin's last read of the entry (it raises where this does)
    except Exception:          # noqa: BLE001 - the twin raises before its path loop
        return False
    finally:
        p.f.close()
    for first in parents:
        seen: set[int] = set()
        q = first
        while q != 0xFFFF and q < len(parents):
            if q in seen:
                return True
            seen.add(q)
            q = parents[q]
    return False


def reference_package(path: str) -> list[Any]:
    """['ok', digest], ['error', type, message] or ['cycle'] (stfs.py never finishes)."""
    cycle = loops_for_ever(path)
    nat = native.load()
    if nat is not None and hasattr(nat, 'stfs_refusal') and (nat.stfs_refusal(path) == 'cycle') != cycle:
        raise SystemExit(f'the recorder and stfs_refusal disagree on a cycle: {path}')
    if cycle:
        return ['cycle']
    stfs.use_native(False)
    p = None
    try:
        p = stfs.Package(path)
        values = [getattr(p, k) for k in tcr.PKG_FIELDS]
        return ['ok', tn.package_digest(values, p.read_reference)]
    except Exception as ex:          # noqa: BLE001 - the error is what is recorded
        return ['error', type(ex).__name__, str(ex)]
    finally:
        if p is not None:
            p.f.close()


def reference_disc(f: Any) -> list[Any]:
    """['ok', digest] or ['error', step ('find', 'root': the test's descriptor read, 'walk'), type, message]."""
    xiso.use_native(False)
    try:
        base = xiso.find_partition(f)
    except (Exception, SystemExit) as ex:          # noqa: BLE001
        return ['error', 'find', type(ex).__name__, str(ex)]
    after_find = f.tell()
    try:
        f.seek(base + 32 * SECTOR + 20)
        sec, size = struct.unpack('<II', f.read(8))
    except struct.error as ex:
        return ['error', 'root', type(ex).__name__, str(ex)]
    try:
        files = list(xiso.walk(f, base, sec, size))
    except (Exception, SystemExit) as ex:          # noqa: BLE001
        return ['error', 'walk', type(ex).__name__, str(ex)]
    return ['ok', tn.disc_digest(base, after_find, files, f.tell())]


def disc_both(path: str) -> list[Any]:
    """reference_disc of the image read from the file and from memory (io.BytesIO): they must agree."""
    with open(path, 'rb') as f:
        a = reference_disc(f)
    with io.BytesIO(open(path, 'rb').read()) as m:
        b = reference_disc(m)
    if a != b:
        raise SystemExit(f'xisolib.py reads {path} differently from the file and from memory: {a} / {b}')
    return a


def big_entry(path: str) -> dict[str, list[Any]]:
    """Content/b.bin's entry damaged to 0xFFFFFF blocks, chained and consecutive: what stfs.py's own loop reads up to
    the entry's size (its length and SHA-256)."""
    stfs.use_native(False)
    pk = stfs.Package(path)
    try:
        e: Any = dict(next(x for x in pk.entries if x['path'] == 'Content/b.bin'))
        out = {}
        for consecutive in (False, True):
            want, b = bytearray(), e['start']
            for _ in range(tn.BIG_COUNT):
                if len(want) >= e['size']:
                    break
                pk.f.seek(pk._addr(b))
                want += pk.f.read(stfs.BLOCK)
                b = b + 1 if consecutive else pk._next(b)
                if consecutive and pk._addr(b) > os.path.getsize(path):
                    break
            got = bytes(want[:e['size']])
            out['consecutive' if consecutive else 'chained'] = [len(got), hashlib.sha256(got).hexdigest()]
        return out
    finally:
        pk.f.close()


def stfs_cli_answers(work: str) -> dict[str, list[Any]]:
    """stfs.main's exit code and printed lines (the output folder written as tcr.OUT) for each of
    tcr.stfs_cli_cases on both kinds of package."""
    stfs.use_native(False)
    out: dict[str, list[Any]] = {}
    for read_only in (True, False):
        label = 'read-only package' if read_only else 'package with two hash tables'
        path = os.path.join(work, f'cli_pkg_{int(read_only)}')
        tcr.build_stfs(path, read_only, seed=0xF040 + int(read_only))
        for case, (args, folder) in tcr.stfs_cli_cases(path, os.path.join(work, f'cli_{int(read_only)}')).items():
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                code = stfs.main(['stfs.py', *args])
            out[f'{label}: {case}'] = [code, tcr.printed(buf.getvalue(), folder)]
    return out


def game_answers(xbox: str) -> dict[str, list[Any]]:
    out: dict[str, list[Any]] = {}
    iso = os.path.join(xbox, 'Battlefield Bad Company 2 Limited Edition (XBOX 360).iso')
    if os.path.isfile(iso):
        with open(iso, 'rb') as f:
            out[tn.game_key(iso)] = reference_disc(f)
        print(f'disc image: {out[tn.game_key(iso)]}', flush=True)
    folder = os.path.join(xbox, 'Battlefield Bad Company 2 (3 DLCs Pack prototype)', '454108A8', '00000002')
    if os.path.isdir(folder):
        for name in sorted(os.listdir(folder)):
            p = os.path.join(folder, name)
            out[tn.game_key(p)] = reference_package(p)
            print(f'{name}: {out[tn.game_key(p)]}', flush=True)
    return out


def write(doc: dict[str, Any]) -> None:
    """The answers with sorted keys, one answer per line."""
    lines = ['{']
    keys = sorted(doc)
    for n, k in enumerate(keys):
        v = doc[k]
        end = ',' if n + 1 < len(keys) else ''
        if isinstance(v, dict):
            inner = [f'  {json.dumps(kk)}: {json.dumps(v[kk], sort_keys=True)}' for kk in sorted(v)]
            lines.append(f' {json.dumps(k)}: {{\n' + ',\n'.join(inner) + '\n }' + end)
        elif isinstance(v, list):
            lines.append(f' {json.dumps(k)}: [\n' + ',\n'.join(f'  {json.dumps(x)}' for x in v) + '\n ]' + end)
        else:
            lines.append(f' {json.dumps(k)}: {json.dumps(v)}{end}')
    lines.append('}')
    with open(tcr.ANSWERS, 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(lines) + '\n')


def main(argv: list[str]) -> int:
    stdout_utf8()
    t0 = time.perf_counter()
    old = tcr.answers() if os.path.isfile(tcr.ANSWERS) else {}
    work = tempfile.mkdtemp(prefix='console_answers_')
    try:
        doc: dict[str, Any] = {
            'about': 'Answers of stfs.py and xisolib.py (the Python twins of bc2_formats::stfs / xiso), recorded by '
                     'record_console_readers_answers.py on the inputs test_console_readers(_native).py build from '
                     'fixed seeds; game entries are digests of the user\'s own Xbox files, keyed by name and size.'}
        pkgs = tn.built_packages(work)
        doc['packages'] = {label: reference_package(p) for label, p in pkgs}
        dst = os.path.join(work, 'damaged')
        doc['damaged_packages'] = [reference_package(p) for p in tn.damaged_packages(pkgs, dst, tn.GAME_PACKAGES)]
        discs = tn.built_discs(work)
        doc['discs'] = {label: disc_both(p) for label, p, _ in discs}
        doc['damaged_discs'] = [disc_both(p) for p in tn.damaged_discs(discs, dst, tn.GAME_DISCS)]
        doc['big_entry'] = big_entry(pkgs[1][1])
        doc['stfs_cli'] = stfs_cli_answers(work)
        if '--game' in argv:
            xbox = argv[argv.index('--xbox') + 1] if '--xbox' in argv else tcr.XBOX_DEFAULT
            doc['game'] = game_answers(xbox)
        elif 'game' in old:
            doc['game'] = old['game']
        write(doc)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    kinds: dict[str, int] = {}
    for a in doc['damaged_packages'] + doc['damaged_discs']:
        kinds[a[0]] = kinds.get(a[0], 0) + 1
    print(f'recorded {tcr.ANSWERS} ({os.path.getsize(tcr.ANSWERS):,} bytes; damaged inputs {kinds}; '
          f'{time.perf_counter() - t0:.1f} s)')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
