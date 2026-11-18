r"""The suite's on-disk caches in %LOCALAPPDATA%\bfbc2-tool-suite\cache: where each is, how big, a size cap with the
least recently used files deleted first, and a command to see and clear them (round E3p, 2026-11-17).

    python -m bfbc2core.caches                    each cache: folder, size, files, cap (also --list)
    python -m bfbc2core.caches --prune [name...]  delete the least recently used files beyond each cap now
    python -m bfbc2core.caches --clear [name...]  delete the cached files (every cache when no name is given)

(from the suite's core\ folder; or python core\bfbc2core\caches.py ...). Names: hlsl, fbrb, collision, effectport,
culltree, bf1943source (round E3r). Every cache is only a cache: a deleted file is read or computed again on its next use, never an error, so
clearing or pruning is safe while tools run.

The helpers the caches use:

    prune(folder, cap, suffixes)  the least recently used entries deleted until the folder's entries fit in cap bytes
    maybe_prune(name, folder)     prune(name's folder) at most once per PRUNE_EVERY seconds (all processes together)
    touch(path)                   marks an entry used now (its modification time: the clock the pruning goes by)
    cap(name)                     the cap in bytes: <ENV>_MB from the environment, else the default

Safe beside readers and writers in other processes. Writers in the suite write a temporary file beside the entry
and rename it over the entry (atomicio, pcbuild, culltree, bc2-formats' atomic::replace): a temporary file
(*.tmp, *.tmp-*) is never deleted unless it is older than STALE_TMP (its writer died). An entry is deleted only
when it still has the size and time the scan saw (one rewritten or touched meanwhile is kept), and a file another
process has open without delete sharing (Windows: a Python reader) is skipped; a reader in the middle of an entry
the Rust core opened keeps reading it. The pruning of a folder runs under a lock between processes
(atomicio.locked, the folder's .prune.lock), so two processes never both delete for the same excess. Only the
cache's own kind of entry (by suffix) is ever deleted: a cache folder pointed at the wrong place by its
environment variable loses nothing else.
"""
from __future__ import annotations

import argparse
import contextlib
import os
import sys
import time
from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field

if __package__ in (None, ''):                       # run as a file: python core\bfbc2core\caches.py
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from bfbc2core import atomicio
else:
    from . import atomicio

MB = 1 << 20
STALE_TMP = 3600.0              # s: a temporary file not written to for this long is a dead writer's
PRUNE_EVERY = 900.0             # s: maybe_prune prunes a folder at most this often
LOCK = '.prune.lock'            # the folder's lock between pruning processes (kept)
STAMP = '.pruned'               # its modification time: the last prune of the folder


@dataclass(frozen=True)
class Spec:
    """One cache: its folder's variable and default name, its entries (by suffix), its cap."""
    name: str
    env: str                    # the folder; '0' (or 'off') turns the cache off
    suffixes: tuple[str, ...]   # the entries: what is counted and pruned
    cap_env: str                # the cap: a number (fractions allowed) of cap_unit bytes
    cap_unit: int
    default_cap: float          # in cap_unit
    what: str
    markers: tuple[str, ...] = ()       # small bookkeeping files: deleted by --clear, by the pruning after MARKER_AGE
    marker_age: float = 30 * 86400.0
    off_without_localappdata: bool = False

    def default_folder(self) -> str | None:
        base = os.environ.get('LOCALAPPDATA')
        if not base:
            if self.off_without_localappdata:
                return None
            base = os.path.join(os.path.expanduser('~'), '.cache') if self.name == 'fbrb' else os.path.expanduser('~')
        return os.path.join(base, 'bfbc2-tool-suite', 'cache', self.name)


# As each owner resolves its folder (pcbuild.cache_dir, fbrb's CacheConfig::from_env, collision._default_cache,
# effectport.cache_dir, culltree.cache_folder, bf1943import.index_dir). The fbrb cache is trimmed by the Rust core itself after each payload
# it writes (same rule, same cap); the others are pruned through maybe_prune by the code that writes them.
CACHES: dict[str, Spec] = {s.name: s for s in (
    Spec('hlsl', 'BFBC2_HLSL_CACHE', ('.bin',), 'BFBC2_HLSL_CACHE_MB', MB, 1024,
         'compiled HLSL (pcbuild.compile_hlsl), one file per source, under a folder per compiler DLL',
         off_without_localappdata=True),
    Spec('fbrb', 'BFBC2_FBRB_CACHE', ('.payload',), 'BFBC2_FBRB_CACHE_GB', 1 << 30, 16,
         "large bundles' decompressed payloads (fbrb.read with the Rust core)", markers=('.seen',)),
    Spec('collision', 'BFBC2_COLLISION_CACHE', ('.bin',), 'BFBC2_COLLISION_CACHE_MB', MB, 256,
         "each data folder's mesh aliases (collision.index, level.load)"),
    Spec('effectport', 'BFBC2_EFFECTPORT_CACHE', ('.pkl',), 'BFBC2_EFFECTPORT_CACHE_MB', MB, 256,
         "BFBC2's emitter documents' instance types per game copy (effectport.EmitterIndex)"),
    Spec('culltree', 'BFBC2_CULLTREE_CACHE', ('.pkl',), 'BFBC2_CULLTREE_CACHE_MB', MB, 2048,
         "reference levels' cull tree rows (culltree ref_rows)"),
    Spec('bf1943source', 'BF1943_SOURCE_INDEX', ('.pkl',), 'BF1943_SOURCE_INDEX_MB', MB, 64,
         "BF1943 source folders' checked bundles and their entries (bf1943import.Source)"),
)}


def folder(name: str) -> str | None:
    """The cache's folder as its owner resolves it from the environment; None when it is turned off."""
    s = CACHES[name]
    v = os.environ.get(s.env, '')
    if v == '0' or v.lower() == 'off':
        return None
    return v or s.default_folder()


def cap(name: str) -> int:
    """The cache's cap in bytes: its cap variable (a non-negative number of MiB, GiB for fbrb), else the default."""
    s = CACHES[name]
    try:
        v = float(os.environ.get(s.cap_env, '').strip())
        if not (v >= 0 and v != float('inf')):
            raise ValueError
    except ValueError:
        v = s.default_cap
    return int(v * s.cap_unit)


def is_temp(file_name: str) -> bool:
    """A writer's temporary file (atomicio: x.tmp-<pid>; pcbuild: x.bin.tmp-<pid>-<thread>; culltree, the Rust
    core: x.<...>.tmp)."""
    return file_name.endswith('.tmp') or '.tmp-' in file_name


def touch(path: str, older_than: float = 60.0) -> None:
    """Marks the entry at path used now, unless it was within older_than seconds (saves a write per hit). Never
    raises: an entry that is gone or cannot be touched stays as it is."""
    with contextlib.suppress(OSError):
        if time.time() - os.stat(path).st_mtime >= older_than:
            os.utime(path)


@dataclass
class _File:
    path: str
    size: int
    mtime_ns: int
    used: float                 # the later of its access and modification times


@dataclass
class _Scan:
    entries: list[_File] = field(default_factory=list)
    temps: list[_File] = field(default_factory=list)
    markers: list[_File] = field(default_factory=list)
    other: int = 0              # files of no known kind (never touched)


def _scan(top: str, suffixes: Sequence[str], markers: Sequence[str] = ()) -> _Scan:
    out = _Scan()
    stack = [top]
    while stack:
        d = stack.pop()
        try:
            it = list(os.scandir(d))
        except OSError:
            continue
        for e in it:
            try:
                if e.is_symlink() or (hasattr(e, 'is_junction') and e.is_junction()):
                    continue                        # never followed: a link may point anywhere
                if e.is_dir(follow_symlinks=False):
                    stack.append(e.path)
                    continue
                if e.name.startswith('.') or not e.is_file(follow_symlinks=False):
                    continue                        # the lock and stamp files
                st = e.stat(follow_symlinks=False)
            except OSError:
                continue                            # gone meanwhile
            f = _File(e.path, st.st_size, st.st_mtime_ns, max(st.st_atime, st.st_mtime))
            if is_temp(e.name):
                out.temps.append(f)
            elif e.name.endswith(tuple(suffixes)):
                out.entries.append(f)
            elif markers and e.name.endswith(tuple(markers)):
                out.markers.append(f)
            else:
                out.other += 1
    return out


@dataclass
class Result:
    """What a prune or clear did: the entries left (bytes, files), those deleted, those kept because another
    process had them open or changed them meanwhile, and the stale temporary files and markers deleted."""
    kept_bytes: int = 0
    kept_files: int = 0
    deleted_bytes: int = 0
    deleted_files: int = 0
    busy: int = 0
    stale: int = 0


def _delete_if_unchanged(f: _File, check: bool = True) -> bool | None:
    """Deletes f if it still has the size and time the scan saw (any time when not check): True deleted, False
    kept (in use, or changed), None gone before (nothing to count)."""
    if check:
        try:
            st = os.stat(f.path)
        except FileNotFoundError:
            return None
        except OSError:
            return False
        if st.st_size != f.size or st.st_mtime_ns != f.mtime_ns:
            return False                            # rewritten or touched since the scan: used, kept
    try:
        os.remove(f.path)
    except FileNotFoundError:
        return None
    except OSError:
        return False                                # open in another process without delete sharing
    return True


def _drop_stale(scan: _Scan, now: float, marker_age: float, r: Result) -> None:
    for f, age in [(t, STALE_TMP) for t in scan.temps] + [(m, marker_age) for m in scan.markers]:
        if now - f.mtime_ns / 1e9 > age and _delete_if_unchanged(f):
            r.stale += 1


def _prune_unlocked(top: str, cap_bytes: int, suffixes: Sequence[str], markers: Sequence[str],
                    marker_age: float, keep: Iterable[str]) -> Result:
    r = Result()
    scan = _scan(top, suffixes, markers)
    _drop_stale(scan, time.time(), marker_age, r)
    keep_set = {os.path.normcase(os.path.abspath(k)) for k in keep}
    total = sum(f.size for f in scan.entries)
    for f in sorted(scan.entries, key=lambda f: (f.used, f.path)):
        if total <= cap_bytes:
            break
        if os.path.normcase(os.path.abspath(f.path)) in keep_set:
            continue
        done = _delete_if_unchanged(f)
        if done is False:
            r.busy += 1
            continue
        total -= f.size
        if done:
            r.deleted_bytes += f.size
            r.deleted_files += 1
    r.kept_bytes = total
    r.kept_files = len(scan.entries) - r.deleted_files
    return r


def prune(top: str, cap_bytes: int, suffixes: Sequence[str], *, markers: Sequence[str] = (),
          marker_age: float = 30 * 86400.0, keep: Iterable[str] = ()) -> Result:
    """Deletes the least recently used entries (files under top ending in one of suffixes; by the later of their
    access and modification times) until the rest fit in cap_bytes, never one named in keep; also temporary files
    older than STALE_TMP and markers older than marker_age. Under the folder's lock; a missing folder is empty."""
    if not os.path.isdir(top):
        return Result()
    with atomicio.locked(os.path.join(top, LOCK)):
        r = _prune_unlocked(top, cap_bytes, suffixes, markers, marker_age, keep)
        with contextlib.suppress(OSError):
            with open(os.path.join(top, STAMP), 'wb'):
                pass
            os.utime(os.path.join(top, STAMP))
    return r


_last: dict[str, float] = {}


def maybe_prune(name: str, top: str | None = None, every: float = PRUNE_EVERY) -> Result | None:
    """prune(the cache's folder (or top), cap(name), ...) unless this folder was pruned in the last `every` seconds
    by this or any other process (the folder's .pruned stamp). Never raises (a cache that cannot be pruned is only
    bigger); None when it did not run."""
    try:
        top = top if top is not None else folder(name)
        if not top or not os.path.isdir(top):
            return None
        key = os.path.normcase(os.path.abspath(top))
        now = time.monotonic()
        if now - _last.get(key, -1e18) < every:
            return None
        _last[key] = now
        stamp = os.path.join(top, STAMP)

        def recent() -> bool:
            try:
                return time.time() - os.stat(stamp).st_mtime < every
            except OSError:
                return False
        if recent():
            return None
        s = CACHES[name]
        with atomicio.locked(os.path.join(top, LOCK)):
            if recent():                            # another process pruned it while this one waited
                return None
            r = _prune_unlocked(top, cap(name), s.suffixes, s.markers, s.marker_age, ())
            with open(stamp, 'wb'):
                pass
            os.utime(stamp)
        return r
    except Exception:                               # noqa: BLE001 - pruning never stops the tool using the cache
        return None


def clear(top: str, suffixes: Sequence[str], markers: Sequence[str] = ()) -> Result:
    """Deletes every entry and marker under top (and stale temporary files; a writer's current one is left), under
    the folder's lock. Files of other kinds are left alone."""
    if not os.path.isdir(top):
        return Result()
    r = Result()
    with atomicio.locked(os.path.join(top, LOCK)):
        scan = _scan(top, suffixes, markers)
        _drop_stale(_Scan(temps=scan.temps), time.time(), 0, r)
        for f in scan.markers:
            _delete_if_unchanged(f, check=False)
        for f in scan.entries:
            done = _delete_if_unchanged(f, check=False)
            if done is False:
                r.busy += 1
                r.kept_bytes += f.size
                r.kept_files += 1
            elif done:
                r.deleted_bytes += f.size
                r.deleted_files += 1
    return r


def usage(top: str | None, suffixes: Sequence[str], markers: Sequence[str] = ()) -> tuple[int, int, int]:
    """(bytes of entries, entries, bytes of everything else the cache keeps: temporary files and markers)."""
    if not top or not os.path.isdir(top):
        return 0, 0, 0
    s = _scan(top, suffixes, markers)
    return (sum(f.size for f in s.entries), len(s.entries),
            sum(f.size for f in s.temps) + sum(f.size for f in s.markers))


def logs_folder() -> str:
    """errors.py's log folder (listed by the command; not a cache: the 20 newest logs per tool are kept)."""
    return os.path.join(os.environ.get('LOCALAPPDATA') or os.path.expanduser('~'), 'bfbc2-tool-suite', 'logs')


def _size(n: int) -> str:
    for unit in ('B', 'KB', 'MB', 'GB'):
        if n < 1024 or unit == 'GB':
            return f'{n:.0f} {unit}' if unit == 'B' else f'{n:.1f} {unit}'
        n /= 1024  # type: ignore[assignment]
    return f'{n} B'


def _names(given: Sequence[str]) -> list[str]:
    bad = [n for n in given if n not in CACHES]
    if bad:
        raise SystemExit(f'unknown cache {", ".join(bad)}; the caches are {", ".join(CACHES)}')
    return list(given) or list(CACHES)


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog='python -m bfbc2core.caches',
                                 description="The suite's on-disk caches: list them, prune them to their caps, "
                                             'or clear them.')
    g = ap.add_mutually_exclusive_group()
    g.add_argument('--list', action='store_true', help='each cache: folder, size, files, cap (the default)')
    g.add_argument('--prune', action='store_true', help='delete the least recently used files beyond each cap')
    g.add_argument('--clear', action='store_true', help='delete the cached files (every cache when no name)')
    ap.add_argument('names', nargs='*', metavar='name', help=f'caches to act on: {", ".join(CACHES)}')
    a = ap.parse_args(argv)
    names = _names(a.names)
    if a.prune or a.clear:
        for n in names:
            s, top = CACHES[n], folder(n)
            if top is None:
                print(f'{n:<11} off ({s.env}=0)')
                continue
            if a.prune:
                r = prune(top, cap(n), s.suffixes, markers=s.markers, marker_age=s.marker_age)
            else:
                r = clear(top, s.suffixes, s.markers)
            print(f'{n:<11} deleted {r.deleted_files} file(s), {_size(r.deleted_bytes)}; left {r.kept_files} file(s), '
                  f'{_size(r.kept_bytes)}' + (f'; {r.busy} in use, kept' if r.busy else '')
                  + (f'; {r.stale} stale temporary file(s) or marker(s) deleted' if r.stale else ''))
        return 0
    total = 0
    for n in names:
        s, top = CACHES[n], folder(n)
        if top is None:
            print(f'{n:<11} off ({s.env}=0)')
            continue
        b, files, extra = usage(top, s.suffixes, s.markers)
        total += b + extra
        unit = 'GB' if s.cap_unit == 1 << 30 else 'MB'
        print(f'{n:<11} {_size(b + extra):>9} in {files:>6} file(s)   cap {_size(cap(n)):>9} ({s.cap_env}, {unit})   '
              f'{top}')
    print(f'{"all":<11} {_size(total):>9}')
    if not a.names:
        lf = logs_folder()
        logs = 0
        with contextlib.suppress(OSError):
            logs = sum(e.stat().st_size for e in os.scandir(lf) if e.is_file())
        print(f'(logs, not a cache: {_size(logs)} in {lf}; the 20 newest per tool are kept)')
    return 0


if __name__ == '__main__':
    sys.exit(main())
