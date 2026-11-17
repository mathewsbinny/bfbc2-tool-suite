"""Whole-file writes that never leave a half-written target: the data goes to a temporary file in the target's own
folder (the same volume, so the rename is atomic), is flushed to disk, and replaces the target with os.replace.
An interrupted write (a crash, a full disk, a kill) leaves the old file as it was and at most a stray *.tmp-<pid>.

Used for every write into the game copy and for the caches and settings the tools read back (suite deep scan
2026-11-15: C1, C2, C3, C6).
"""
from __future__ import annotations

import contextlib
import json
import os
import shutil
import sys
import time
from collections.abc import Iterator
from typing import Any


@contextlib.contextmanager
def locked(path: str, poll: float = 0.2) -> Iterator[None]:
    """Holds a lock between processes on the lock file path (created when missing, kept afterwards) while the block
    runs; a second process waits. The operating system drops the lock when its process ends, so a killed holder
    never leaves it stuck. For work two processes must not do at once - unpacking a level's terrain over the folder
    another reads, building a shared cache twice (run_tests.py runs tests side by side; 2026-11-17)."""
    os.makedirs(os.path.dirname(path) or '.', exist_ok=True)
    fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o666)
    try:
        if sys.platform == 'win32':
            import msvcrt
            while True:
                os.lseek(fd, 0, os.SEEK_SET)
                try:
                    msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
                    break
                except OSError:
                    time.sleep(poll)
            try:
                yield
            finally:
                os.lseek(fd, 0, os.SEEK_SET)
                msvcrt.locking(fd, msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            fcntl.flock(fd, fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(fd, fcntl.LOCK_UN)
    finally:
        os.close(fd)


def _tmp(path: str) -> str:
    return f'{path}.tmp-{os.getpid()}'


def _finish(tmp: str, path: str) -> None:
    try:
        os.replace(tmp, path)
    except BaseException:
        with contextlib.suppress(OSError):
            os.remove(tmp)
        raise


def write_bytes(path: str, data: bytes) -> None:
    tmp = _tmp(path)
    try:
        with open(tmp, 'wb') as f:
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
    except BaseException:
        with contextlib.suppress(OSError):
            os.remove(tmp)
        raise
    _finish(tmp, path)


def write_text(path: str, text: str, encoding: str = 'utf-8', newline: str | None = None) -> None:
    """text as open(path, 'w', encoding=encoding, newline=newline) would write it."""
    if newline is None:
        text = text.replace('\n', os.linesep)
    elif newline:
        text = text.replace('\n', newline)
    write_bytes(path, text.encode(encoding))


def write_json(path: str, obj: Any, **kw: Any) -> None:
    """json.dump(obj, ...) to path, whole or not at all (the serialisation runs before the file is touched)."""
    write_text(path, json.dumps(obj, **kw))


def copy_file(src: str, dst: str) -> None:
    """shutil.copyfile(src, dst), the target replaced only once the whole copy is on disk."""
    tmp = _tmp(dst)
    try:
        shutil.copyfile(src, tmp)
        with open(tmp, 'rb+') as f:
            os.fsync(f.fileno())
    except BaseException:
        with contextlib.suppress(OSError):
            os.remove(tmp)
        raise
    _finish(tmp, dst)


def read_json(path: str, default: Any = None) -> Any:
    """The JSON at path, or default when it is missing or unreadable (a cache: a damaged one is a miss)."""
    try:
        with open(path, encoding='utf-8') as f:
            return json.load(f)
    except (OSError, ValueError):
        return default
