r"""FbrbTool's commands: unpack, pack, list and big, run in process by the suite's Rust core
(rust\bc2-formats\src\fbrb\tool.rs, a port of FbrbTool.cs), without starting .NET for each call. The same files are
written, byte for byte, with the same messages and exit codes FbrbTool.exe gave (rust\BENCHMARKS.md, "E3k"; the exe was
retired 2026-11-18, round R4: core\tests\test_fbrbtool.py checks against its recorded answers).

    from bfbc2core import fbrbtool
    r = fbrbtool.run(['unpack', bundle, folder])               # a CompletedProcess: returncode, stdout, stderr
    r = fbrbtool.unpack(bundle, folder)                         # the same, by command
    r = fbrbtool.pack(folder, bundle);  fbrbtool.list_entries(bundle);  fbrbtool.big(big_file, folder)
    rs = fbrbtool.run_many([[...], [...]])                      # many command lines on all cores
    r = fbrbtool.run_cmd(['FbrbTool', 'unpack', b, d], check=True, capture_output=True)   # drop-in for subprocess.run

    python bfbc2-fbrb-tool\fbrbtool.py <command> ...           # the command line FbrbTool.exe had (main())

run_cmd takes a command line whose program is FbrbTool ('FbrbTool', or FbrbTool.exe by any path: no file needs to be
there, as callers built them for subprocess.run) and the keyword arguments those callers pass (capture_output, text,
check, stdout=..., ...). Output is text with '\n' line ends (as subprocess.run(..., text=True) gave FbrbTool's), or bytes
with '\r\n' when text is false. With cwd=, the command's relative paths are taken from that folder.

The Rust core is required (bfbc2core.native.require: NativeMissing says how to get it); BFBC2_FBRBTOOL_EXE, the switch
that ran FbrbTool.exe instead, is gone with the exe.

Each run is logged as FbrbTool logged itself (%LOCALAPPDATA%\bfbc2-tool-suite\logs\fbrbtool-<date>.log), its start
line marked "(in process)".
"""
from __future__ import annotations

import datetime
import os
import subprocess
import sys
from collections.abc import Iterable, Sequence
from typing import Any

from . import caches, native

NATIVE_ATTR = 'fbrbtool_run'
WHAT = 'FbrbTool\'s commands'

Result = subprocess.CompletedProcess


def log_path() -> str | None:
    """FbrbTool's log file for today (as FbrbTool's ToolLog names it; once it reaches BFBC2_LOG_FILE_MB, the day's
    next part: caches.day_log, as dbxedit.log_path), or None when the folder cannot be made."""
    try:
        base = os.environ.get('LOCALAPPDATA') or os.path.expanduser('~')
        d = os.path.join(base, 'bfbc2-tool-suite', 'logs')
        os.makedirs(d, exist_ok=True)
        return caches.day_log(d, 'fbrbtool', datetime.date.today().isoformat())
    except OSError:
        return None


def _log(path: str | None, args: Sequence[str], lines: Iterable[tuple[str, str]]) -> None:
    if path is None:
        return
    now = datetime.datetime.now()
    stamp = now.strftime('%Y-%m-%d %H:%M:%S,') + f'{now.microsecond // 1000:03d}'
    text = f'{stamp} INFO    --- fbrbtool started (in process): {" ".join(args)} ---\r\n'
    for level, message in lines:
        text += f'{stamp} {level:<7} {message}\r\n'
    try:
        with open(path, 'a', encoding='utf-8', newline='') as f:
            f.write(text)
    except OSError:
        pass                                  # no log: the command still ran


def _result(args: Sequence[str], code: int, out: str, err: str, text: bool, cmd: Sequence[str] | None = None) -> Result:
    if text:
        return subprocess.CompletedProcess(list(cmd or args), code, out, err)
    return subprocess.CompletedProcess(list(cmd or args), code, out.replace('\n', '\r\n').encode('utf-8'),
                                       err.replace('\n', '\r\n').encode('utf-8'))


def _native_run(args: Sequence[str]) -> tuple[int, str, str]:
    path = log_path()
    code, out, err, lines = native.require(WHAT, NATIVE_ATTR).fbrbtool_run(list(args), path)
    _log(path, args, lines)
    return code, out, err


def run(args: Sequence[str], *, text: bool = True, check: bool = False) -> Result:
    """One FbrbTool command line (without the program name), in process. check: raise subprocess.CalledProcessError
    on a non-zero exit code, as subprocess.run(check=True)."""
    args = [os.fspath(a) for a in args]
    r = _result(args, *_native_run(args), text)
    if check:
        r.check_returncode()
    return r


def run_many(runs: Sequence[Sequence[str]], *, text: bool = True) -> list[Result]:
    """Many command lines at once, in process on all cores (none may write a file or folder another reads or
    writes). Results in order."""
    runs = [[os.fspath(a) for a in r] for r in runs]
    path = log_path()
    outs = native.require(WHAT, NATIVE_ATTR).fbrbtool_run_many(runs, path)
    results = []
    for args, (code, out, err, lines) in zip(runs, outs):
        _log(path, args, lines)
        results.append(_result(args, code, out, err, text))
    return results


def is_fbrbtool(program: str) -> bool:
    """Whether a command line's program is FbrbTool (FbrbTool.exe by any path, or 'FbrbTool')."""
    name = os.path.basename(os.fspath(program)).lower()
    return name in ('fbrbtool.exe', 'fbrbtool')


def run_cmd(cmd: Sequence[Any], *, capture_output: bool = False, text: bool = False, check: bool = False,
            encoding: str | None = None, errors: str | None = None, universal_newlines: bool | None = None,
            **kwargs: Any) -> Result:
    """subprocess.run(cmd, ...) for a command line whose program is FbrbTool: the command runs in process (with cwd=,
    its relative paths taken from that folder). Other programs run with subprocess.run as given. capture_output
    defaults to False, as subprocess.run's does; stdout= / stderr= (PIPE, DEVNULL, STDOUT, a file) are honoured as
    subprocess.run honours them."""
    cmd = [os.fspath(c) for c in cmd]
    as_text = bool(text or universal_newlines or encoding or errors)
    if not cmd or not is_fbrbtool(cmd[0]):
        return subprocess.run(cmd, capture_output=capture_output, text=bool(text or universal_newlines), check=check,
                              encoding=encoding, errors=errors, **kwargs)
    cwd = kwargs.get('cwd')
    if cwd is not None:
        cmd = cmd[:2] + [a if not a or os.path.isabs(a) else os.path.join(os.fspath(cwd), a) for a in cmd[2:]]
    code, out, err = _native_run(cmd[1:])
    r = _result(cmd[1:], code, out, err, as_text, cmd)
    if not capture_output:
        # not captured: printed where the exe's output would go, unless the caller sent it elsewhere
        for stream, sink in (('stdout', sys.stdout), ('stderr', sys.stderr)):
            text_out = out if stream == 'stdout' else err
            target = kwargs.get(stream)
            if target is subprocess.PIPE:
                continue                                          # captured: kept on the result
            if target is subprocess.STDOUT and stream == 'stderr':
                if kwargs.get('stdout') is subprocess.PIPE:
                    joined = out + err
                    r.stdout = joined if as_text else joined.replace('\n', '\r\n').encode('utf-8')
                elif text_out and sys.stdout is not None:
                    sys.stdout.write(text_out)
            elif text_out and target is None and sink is not None:  # sys.stdout is None under pythonw
                sink.write(text_out)
            elif text_out and target not in (None, subprocess.DEVNULL, subprocess.STDOUT):
                writer = getattr(target, 'write', None)
                if writer is not None:
                    writer(text_out if 'b' not in getattr(target, 'mode', '') else
                           text_out.replace('\n', '\r\n').encode('utf-8'))
                elif isinstance(target, int):
                    os.write(target, text_out.replace('\n', '\r\n').encode('utf-8'))
            setattr(r, stream, None)
    if check:
        r.check_returncode()
    return r


# ------------------------------------------------------------------------------------------- the commands by name

def unpack(bundle: str, folder: str, **kw: Any) -> Result:
    """FbrbTool unpack: every entry of the bundle under folder, and fbrb.manifest (*deleted* markers listed only)."""
    return run(['unpack', bundle, folder], **kw)


def pack(folder: str, bundle: str, **kw: Any) -> Result:
    """FbrbTool pack: the bundle of an unpacked folder (its files and fbrb.manifest), written atomically."""
    return run(['pack', folder, bundle], **kw)


def list_entries(bundle: str, **kw: Any) -> Result:
    """FbrbTool list: size, type and name of every entry, then the count, payload length and zipped flag."""
    return run(['list', bundle], **kw)


def big(path: str, folder: str, **kw: Any) -> Result:
    """FbrbTool big: every file of a BIGF / BIG4 archive (a SwfMovie resource) under folder."""
    return run(['big', path, folder], **kw)


def main(argv: Sequence[str]) -> int:
    """The command line FbrbTool.exe had (bfbc2-fbrb-tool\\fbrbtool.py): its output printed, its exit code returned.
    Without arguments, the usage."""
    try:
        r = run(list(argv))
    except native.NativeMissing as e:
        print(f'error: {e}', file=sys.stderr)
        return 1
    if r.stdout:
        sys.stdout.write(r.stdout)
    if r.stderr:
        sys.stderr.write(r.stderr)
    return int(r.returncode)
