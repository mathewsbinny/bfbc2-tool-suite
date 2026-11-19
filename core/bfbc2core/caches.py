r"""The suite's on-disk caches in %LOCALAPPDATA%\bfbc2-tool-suite\cache: where each is, how big, a size cap with the
least recently used files deleted first, and a command to see and clear them (round E3p, 2026-11-17).

    python -m bfbc2core.caches                    each cache: folder, size, files, cap (also --list)
    python -m bfbc2core.caches --prune [name...]  delete the least recently used files beyond each cap now
    python -m bfbc2core.caches --clear [name...]  delete the cached files (every cache when no name is given)

(from the suite's core\ folder; or python core\bfbc2core\caches.py ...). Names: hlsl, fbrb, collision, effectport,
culltree, bf1943source (round E3r), shadercover (round S1); the project caches navstudio, harness, navwater, editor
(round G3); logs. Every cache is only a cache: a deleted file is read
or computed again on its next use, never an error, so clearing or pruning is safe while tools run.

Project caches (round E3q): caches the suite's tools keep inside Project OPFOR BFBC2's work\ folder by design -
navdata.load's path-data caches of Navigation Studio (work\navstudio\cache), the test harness (work\harness\cache)
and navwater_mesh (work\navwater\cache). The project folder is found as the tools find it (paths.find_opfor: --opfor,
OPFOR_ROOT, else walking up from the suite); navdata.load prunes the folder it wrote to through prune_project_cache.
--clear without names leaves them (and the logs) alone: name them to clear them.

Logs (round E3q): errors.setup's %LOCALAPPDATA%\bfbc2-tool-suite\logs\<tool>-<date>.log, rolled over within the day
to <tool>-<date>.1.log, .2.log ... once a file reaches BFBC2_LOG_FILE_MB (default 20; nothing renamed, so a file
another process holds is never in the way); prune_logs keeps each tool's 20 newest days, then deletes the oldest files
beyond BFBC2_LOG_TOOL_MB per tool (default 100) and BFBC2_LOGS_MB overall (default 200) - never the current session's
files (keep) nor, for the size caps, a file written in the last LOG_RECENT seconds (a running tool's). The
files a prune removes are zipped into logs-archive\ first (the user's ruling, 2026-11-17; BFBC2_LOG_ARCHIVE_MB).

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
import datetime
import os
import re
import sys
import time
from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field

if __package__ in (None, ''):                       # run as a file: python core\bfbc2core\caches.py
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from bfbc2core import atomicio, paths
else:
    from . import atomicio, paths

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
    project: tuple[str, ...] = ()       # a project cache: its folder under Project OPFOR BFBC2's folder (env unused)

    def default_folder(self) -> str | None:
        base = os.environ.get('LOCALAPPDATA')
        if not base:
            if self.off_without_localappdata:
                return None
            base = os.path.join(os.path.expanduser('~'), '.cache') if self.name == 'fbrb' else os.path.expanduser('~')
        return os.path.join(base, 'bfbc2-tool-suite', 'cache', self.name)


# As each owner resolves its folder (pcbuild.cache_dir, fbrb's CacheConfig::from_env, collision._default_cache,
# effectport.cache_dir, culltree.cache_folder, bf1943import.index_dir, shaderdb.cover_cache_dir). The fbrb cache is trimmed by the Rust core itself after each payload
# it writes (same rule, same cap); the others are pruned through maybe_prune by the code that writes them.
CACHES: dict[str, Spec] = {s.name: s for s in (
    Spec('hlsl', 'BFBC2_HLSL_CACHE', ('.bin',), 'BFBC2_HLSL_CACHE_MB', MB, 1024,
         'compiled HLSL (pcbuild.compile_hlsl), one file per source, under a folder per compiler DLL',
         off_without_localappdata=True),
    Spec('fbrb', 'BFBC2_FBRB_CACHE', ('.payload',), 'BFBC2_FBRB_CACHE_GB', 1 << 30, 16,
         "large bundles' decompressed payloads (fbrb.read)", markers=('.seen',)),
    Spec('collision', 'BFBC2_COLLISION_CACHE', ('.bin',), 'BFBC2_COLLISION_CACHE_MB', MB, 256,
         "each data folder's mesh aliases (collision.index, level.load)"),
    Spec('effectport', 'BFBC2_EFFECTPORT_CACHE', ('.pkl',), 'BFBC2_EFFECTPORT_CACHE_MB', MB, 256,
         "BFBC2's emitter documents' instance types per game copy (effectport.EmitterIndex)"),
    Spec('culltree', 'BFBC2_CULLTREE_CACHE', ('.pkl',), 'BFBC2_CULLTREE_CACHE_MB', MB, 2048,
         "reference levels' cull tree rows (culltree ref_rows)"),
    Spec('bf1943source', 'BF1943_SOURCE_INDEX', ('.pkl',), 'BF1943_SOURCE_INDEX_MB', MB, 64,
         "BF1943 source folders' checked bundles and their entries (bf1943import.Source)"),
    Spec('shadercover', 'BFBC2_SHADERCOVER_CACHE', ('.pkl',), 'BFBC2_SHADERCOVER_CACHE_MB', MB, 64,
         "level shader databases' entry-name prefixes (shaderdb.level_coverage / donor_levels)"),
)}

# navdata.load's entries: <key>.npz (the cells) and <key>.navdata.pkl (the whole NavData); temporary <entry>.<pid>.tmp
NAVDATA = ('.navdata.pkl', '.npz')
# the map editor's Package level copies: partitions, resources, their XML and each level's record
EDITOR = ('.dbx', '.res', '.xml', '.json')

# Caches in Project OPFOR BFBC2's work\ folder (studio.py, harness.py cache_dir, navwater_mesh.py): kept there, listed
# and pruned here. A harness run reads every level's entries in turn, so its cap holds a whole run (about 2 GB, 2026-11).
PROJECT_CACHES: dict[str, Spec] = {s.name: s for s in (
    Spec('navstudio', '', NAVDATA, 'BFBC2_NAVSTUDIO_CACHE_MB', MB, 2048,
         "Navigation Studio's path-data loads (navdata.load)", project=('work', 'navstudio', 'cache')),
    Spec('harness', '', NAVDATA, 'BFBC2_HARNESS_CACHE_MB', MB, 3072,
         "the test harness's path-data loads (navdata.load; its bundles\\ entries are not pruned)",
         project=('work', 'harness', 'cache')),
    Spec('navwater', '', NAVDATA, 'BFBC2_NAVWATER_CACHE_MB', MB, 1024,
         "navwater_mesh's path-data loads (navdata.load)", project=('work', 'navwater', 'cache')),
    # the map editor's copies of the levels that live only in game\Package (round G3, rust/bc2-editor/src/package.rs):
    # each level's folder holds what the editor opens from its bundles and editor-level.json, the bundles' sizes and
    # times and every file; a copy with a file missing or a bundle changed is made again on its next opening. The editor
    # keeps the folder under the same cap itself (least recently opened levels first).
    Spec('editor', '', EDITOR, 'BFBC2_EDITOR_CACHE_MB', MB, 2048,
         "the map editor's copies of game\\Package levels (Vietnam, Pacific War ...)",
         project=('work', 'editor', 'levels')),
)}


def _spec(name: str) -> Spec:
    return CACHES[name] if name in CACHES else PROJECT_CACHES[name]


def project_root(given: str | None = None) -> str | None:
    """Project OPFOR BFBC2's folder as the tools find it (paths.find_opfor: given, OPFOR_ROOT, walking up from the
    suite), or None."""
    try:
        return paths.find_opfor(given)
    except paths.PathsError:
        return None


def folder(name: str, opfor: str | None = None) -> str | None:
    """The cache's folder as its owner resolves it from the environment; None when it is turned off (or, for a
    project cache, when there is no project folder; opfor: the project folder given)."""
    s = _spec(name)
    if s.project:
        root = project_root(opfor)
        return os.path.join(root, *s.project) if root else None
    v = os.environ.get(s.env, '')
    if v == '0' or v.lower() == 'off':
        return None
    return v or s.default_folder()


def _number(env: str, default: float) -> float:
    try:
        v = float(os.environ.get(env, '').strip())
        if not (v >= 0 and v != float('inf')):
            raise ValueError
    except ValueError:
        v = default
    return v


def cap(name: str) -> int:
    """The cache's cap in bytes: its cap variable (a non-negative number of MiB, GiB for fbrb), else the default."""
    s = _spec(name)
    return int(_number(s.cap_env, s.default_cap) * s.cap_unit)


def project_cache_of(path: str) -> tuple[str, str] | None:
    """(name, folder) of the project cache path is (or lies in): the folder whose last parts are the cache's
    work\\<tool>\\cache, wherever the project is; None for any other path."""
    full = os.path.abspath(path)
    parts = full.split(os.sep)
    low = [os.path.normcase(p) for p in parts]
    for k in range(len(parts), 0, -1):
        for name, s in PROJECT_CACHES.items():
            n = len(s.project)
            if k >= n + 1 and low[k - n:k] == [os.path.normcase(p) for p in s.project]:
                return name, os.sep.join(parts[:k]) or os.sep
    return None


def prune_project_cache(path: str) -> Result | None:
    """maybe_prune of the project cache path is in (navdata.load, after it writes an entry); None when path is in
    none (a temporary folder, a test's) or it did not run. Never raises."""
    try:
        hit = project_cache_of(path)
    except Exception:                               # noqa: BLE001 - pruning never stops the tool using the cache
        return None
    return maybe_prune(hit[0], hit[1]) if hit else None


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
        s = _spec(name)
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
    """errors.py's log folder (errors.log_dir, not made here)."""
    return os.path.join(os.environ.get('LOCALAPPDATA') or os.path.expanduser('~'), 'bfbc2-tool-suite', 'logs')


# ---- logs (round E3q)
LOG_KEEP_DAYS = 20              # each tool's newest days kept (errors.py's old rule: its 20 newest files)
LOG_RECENT = 600.0              # s: a log written this recently may be a running tool's: never deleted to fit a cap
LOGS_ENV, LOGS_MB = 'BFBC2_LOGS_MB', 200.0                  # all logs together
LOG_TOOL_ENV, LOG_TOOL_MB = 'BFBC2_LOG_TOOL_MB', 100.0      # one tool's logs
LOG_FILE_ENV, LOG_FILE_MB = 'BFBC2_LOG_FILE_MB', 20.0       # one file, before the day goes on in the next part
LOG_NAME = re.compile(r'^(?P<tool>.+)-(?P<date>\d{4}-\d{2}-\d{2})(?:\.(?P<part>\d+))?\.log$', re.IGNORECASE)


def log_caps() -> tuple[int, int, int]:
    """(all logs, one tool's, one file's) caps in bytes: BFBC2_LOGS_MB, BFBC2_LOG_TOOL_MB, BFBC2_LOG_FILE_MB (MiB,
    fractions allowed), else 200, 100, 20 MiB."""
    return (int(_number(LOGS_ENV, LOGS_MB) * MB), int(_number(LOG_TOOL_ENV, LOG_TOOL_MB) * MB),
            int(_number(LOG_FILE_ENV, LOG_FILE_MB) * MB))


LOG_ARCHIVE_ENV, LOG_ARCHIVE_MB = 'BFBC2_LOG_ARCHIVE_MB', 1024.0   # the archives of pruned logs, all together


def log_archive_folder(top: str) -> str:
    """Where pruned logs are kept, zipped: <logs>-archive beside the log folder (the user's ruling, 2026-11-17: logs
    the caps remove are archived first, not lost)."""
    return os.path.normpath(top) + '-archive'


def _archive_logs(top: str, files: list[_File]) -> str | None:
    """files zipped into a new archive in log_archive_folder(top) (logs-<time>.zip), written beside its name and
    moved in only when complete (atomicio): a crash or an unreadable file leaves no half archive. Returns its path,
    None when there was nothing to keep. The archives over BFBC2_LOG_ARCHIVE_MB (default 1024 MiB) are deleted,
    the oldest first - never the new one."""
    import zipfile
    files = [f for f in files if os.path.isfile(f.path)]
    if not files:
        return None
    folder = log_archive_folder(top)
    os.makedirs(folder, exist_ok=True)
    stamp = datetime.datetime.now().strftime('%Y-%m-%d_%H%M%S')
    path = os.path.join(folder, f'logs-{stamp}.zip')
    n = 1
    while os.path.exists(path):
        n += 1
        path = os.path.join(folder, f'logs-{stamp}-{n}.zip')
    tmp = f'{path}.tmp-{os.getpid()}'
    try:
        with zipfile.ZipFile(tmp, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as z:
            for f in files:
                z.write(f.path, os.path.basename(f.path))
        with open(tmp, 'rb+') as fh:
            os.fsync(fh.fileno())
        atomicio.replace(tmp, path)
    finally:
        with contextlib.suppress(OSError):
            os.remove(tmp)
    cap = int(_number(LOG_ARCHIVE_ENV, LOG_ARCHIVE_MB) * MB)
    olds = []
    for e in os.scandir(folder):
        if e.name.lower().endswith('.zip') and e.is_file(follow_symlinks=False) and e.path != path:
            st = e.stat(follow_symlinks=False)
            olds.append((st.st_mtime_ns, e.path, st.st_size))
    size = os.path.getsize(path) + sum(s for _t, _p, s in olds)
    for _t, p, s in sorted(olds):
        if size <= cap:
            break
        with contextlib.suppress(OSError):
            os.remove(p)
            size -= s
    return path


def log_part(top: str, tool: str, date: str, part: int) -> str:
    """<top>\\<tool>-<date>.log (part 0), else <tool>-<date>.<part>.log."""
    return os.path.join(top, f'{tool}-{date}.log' if part == 0 else f'{tool}-{date}.{part}.log')


def day_log(top: str, tool: str, date: str | None = None, limit: int | None = None) -> str:
    """The file a tool appends its day's lines to: the day's last part while it is under limit bytes (the per-file
    cap by default), else the next part. Never renames or deletes: another process may hold any of them."""
    date = date or datetime.date.today().isoformat()
    limit = log_caps()[2] if limit is None else limit
    last = 0
    with contextlib.suppress(OSError):
        for n in os.listdir(top):
            m = LOG_NAME.match(n)
            if m and m['tool'] == tool and m['date'] == date:
                last = max(last, int(m['part'] or 0))
    p = log_part(top, tool, date, last)
    try:
        full = os.path.getsize(p) >= max(limit, 1)
    except OSError:
        full = False                                # not there yet: this is the one
    return log_part(top, tool, date, last + 1) if full else p


def _log_files(top: str) -> list[tuple[_File, str, str]]:
    """(file, tool, date) of every log in top (not below it; links and other files left out)."""
    out = []
    with contextlib.suppress(OSError):
        for e in os.scandir(top):
            m = LOG_NAME.match(e.name)
            if not m:
                continue
            try:
                if e.is_symlink() or not e.is_file(follow_symlinks=False):
                    continue
                st = e.stat(follow_symlinks=False)
            except OSError:
                continue
            out.append((_File(e.path, st.st_size, st.st_mtime_ns, st.st_mtime), m['tool'], m['date']))
    return out


def prune_logs(top: str | None = None, keep: Iterable[str] = (), *, caps: tuple[int, int, int] | None = None,
               keep_days: int = LOG_KEEP_DAYS, recent: float = LOG_RECENT) -> Result:
    """The log folder (logs_folder() by default) to its caps: each tool's files of other than its keep_days newest
    days deleted, then the oldest files (by modification time) beyond the per-tool cap, then beyond the overall cap.
    Never a file named in keep (the current session's), never for the caps a file written within `recent` seconds,
    never one another process has open without delete sharing (busy). Under the folder's lock.
    The files chosen are first zipped into a new archive (_archive_logs: log_archive_folder, its own cap) and only
    then deleted; when the archive cannot be written, nothing is deleted (OSError raised)."""
    top = top or logs_folder()
    r = Result()
    if not os.path.isdir(top):
        return r
    total_cap, tool_cap, _ = caps if caps is not None else log_caps()
    keep_set = {os.path.normcase(os.path.abspath(k)) for k in keep}
    now = time.time()
    with atomicio.locked(os.path.join(top, LOCK)):
        files = _log_files(top)
        kept = {os.path.normcase(f.path) for f, _t, _d in files if os.path.normcase(os.path.abspath(f.path)) in keep_set}
        gone: set[str] = set()
        chosen: list[_File] = []

        def drop(f: _File, held: bool) -> bool:
            """Chooses f for deletion unless it is kept (held: also when recent); True when it no longer counts."""
            if os.path.normcase(f.path) in kept or held and now - f.mtime_ns / 1e9 < recent:
                return False
            gone.add(f.path)
            chosen.append(f)
            return True

        by_tool: dict[str, list[tuple[_File, str]]] = {}
        for f, tool, date in files:
            by_tool.setdefault(tool, []).append((f, date))
        for items in by_tool.values():
            days = sorted({d for _f, d in items}, reverse=True)[:keep_days]
            for f, d in items:
                if d not in days:
                    drop(f, held=False)
            left = sorted((f for f, _d in items if f.path not in gone), key=lambda f: (f.mtime_ns, f.path))
            size = sum(f.size for f in left)
            for f in left:
                if size <= tool_cap:
                    break
                if drop(f, held=True):
                    size -= f.size
        left = sorted((f for f, _t, _d in files if f.path not in gone), key=lambda f: (f.mtime_ns, f.path))
        size = sum(f.size for f in left)
        for f in left:
            if size <= total_cap:
                break
            if drop(f, held=True):
                size -= f.size
        # archive first, then delete: a log the caps remove is kept in the archive (never lost)
        if chosen:
            _archive_logs(top, chosen)
        busy = 0
        for f in chosen:
            done = _delete_if_unchanged(f)
            if done is False:                       # held open or written since the scan: kept (and archived)
                busy += 1
                size += f.size
            elif done:
                r.deleted_bytes += f.size
                r.deleted_files += 1
        r.busy = busy
        r.kept_bytes = size
        r.kept_files = len(left) - sum(1 for f in left if f.path in gone) + busy
    return r


def clear_logs(top: str | None = None, keep: Iterable[str] = ()) -> Result:
    """Every log deleted but those written within LOG_RECENT seconds (a running tool's) and those in keep."""
    return prune_logs(top, keep, caps=(0, 0, 0))


def logs_usage(top: str | None = None) -> tuple[int, int]:
    """(bytes, files) of the logs."""
    files = _log_files(top or logs_folder())
    return sum(f.size for f, _t, _d in files), len(files)


def _size(n: int) -> str:
    for unit in ('B', 'KB', 'MB', 'GB'):
        if n < 1024 or unit == 'GB':
            return f'{n:.0f} {unit}' if unit == 'B' else f'{n:.1f} {unit}'
        n /= 1024  # type: ignore[assignment]
    return f'{n} B'


LOGS = 'logs'


def _names(given: Sequence[str]) -> list[str]:
    known = [*CACHES, *PROJECT_CACHES, LOGS]
    bad = [n for n in given if n not in known]
    if bad:
        raise SystemExit(f'unknown cache {", ".join(bad)}; the caches are {", ".join(known)}')
    return list(given) or known


def _done(n: str, r: Result) -> str:
    return (f'{n:<11} deleted {r.deleted_files} file(s), {_size(r.deleted_bytes)}; left {r.kept_files} file(s), '
            f'{_size(r.kept_bytes)}' + (f'; {r.busy} in use, kept' if r.busy else '')
            + (f'; {r.stale} stale temporary file(s) or marker(s) deleted' if r.stale else ''))


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog='python -m bfbc2core.caches',
                                 description="The suite's on-disk caches, the project caches in Project OPFOR "
                                             "BFBC2's work\\ folder and the logs: list them, prune them to their caps, "
                                             'or clear them.')
    g = ap.add_mutually_exclusive_group()
    g.add_argument('--list', action='store_true', help='each cache: folder, size, files, cap (the default)')
    g.add_argument('--prune', action='store_true', help='delete the least recently used files beyond each cap '
                                                        '(every cache, project cache and the logs when no name)')
    g.add_argument('--clear', action='store_true', help="delete the cached files (every suite cache when no name; "
                                                        'project caches and logs only when named)')
    ap.add_argument('--opfor', metavar='DIR', help='Project OPFOR BFBC2\'s folder (default: as the tools find it)')
    ap.add_argument('names', nargs='*', metavar='name',
                    help=f'caches to act on: {", ".join([*CACHES, *PROJECT_CACHES, LOGS])}')
    a = ap.parse_args(argv)
    names = _names(a.names)
    if a.opfor and not os.path.isdir(os.path.join(a.opfor, paths.GAME_DIR)):
        raise SystemExit(f'--opfor {a.opfor}: no game\\ folder there (not Project OPFOR BFBC2\'s folder)')
    if a.clear and not a.names:
        names = list(CACHES)
    if a.prune or a.clear:
        for n in names:
            if n == LOGS:
                print(_done(n, prune_logs() if a.prune else clear_logs()))
                continue
            s, top = _spec(n), folder(n, a.opfor)
            if top is None:
                print(f'{n:<11} ' + ('no project folder (--opfor, OPFOR_ROOT)' if s.project else f'off ({s.env}=0)'))
                continue
            if a.prune:
                r = prune(top, cap(n), s.suffixes, markers=s.markers, marker_age=s.marker_age)
            else:
                r = clear(top, s.suffixes, s.markers)
            print(_done(n, r))
        if a.clear and not a.names:
            print(f'(project caches and logs are cleared only by name: {", ".join([*PROJECT_CACHES, LOGS])})')
        return 0
    total = 0
    for n in names:
        if n == LOGS:
            b, files = logs_usage()
            total += b
            all_cap, tool_cap, file_cap = log_caps()
            print(f'{n:<11} {_size(b):>9} in {files:>6} file(s)   cap {_size(all_cap):>9} ({LOGS_ENV}, MB; per tool '
                  f'{_size(tool_cap)} {LOG_TOOL_ENV}, per file {_size(file_cap)} {LOG_FILE_ENV})   {logs_folder()}')
            continue
        s, top = _spec(n), folder(n, a.opfor)
        if top is None:
            print(f'{n:<11} ' + ('no project folder (--opfor, OPFOR_ROOT)' if s.project else f'off ({s.env}=0)'))
            continue
        b, files, extra = usage(top, s.suffixes, s.markers)
        total += b + extra
        unit = 'GB' if s.cap_unit == 1 << 30 else 'MB'
        print(f'{n:<11} {_size(b + extra):>9} in {files:>6} file(s)   cap {_size(cap(n)):>9} ({s.cap_env}, {unit})   '
              f'{top}' + ('   (project cache)' if s.project else ''))
    print(f'{"all":<11} {_size(total):>9}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
