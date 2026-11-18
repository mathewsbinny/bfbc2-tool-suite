r"""FbrbTool's commands in process: bfbc2-fbrb-tool\FbrbTool.exe's unpack, pack, list and big run by the suite's Rust
core (rust\bc2-formats\src\fbrb\tool.rs, a port of FbrbTool.cs), without starting .NET for each call. The same files
are written, byte for byte, with the same messages and exit codes (rust\BENCHMARKS.md, "E3k").

    from bfbc2core import fbrbtool
    r = fbrbtool.run(['unpack', bundle, folder])               # a CompletedProcess: returncode, stdout, stderr
    r = fbrbtool.unpack(bundle, folder)                         # the same, by command
    r = fbrbtool.pack(folder, bundle);  fbrbtool.list_entries(bundle);  fbrbtool.big(big_file, folder)
    rs = fbrbtool.run_many([[...], [...]])                      # many command lines on all cores
    r = fbrbtool.run_cmd([FBRBTOOL_EXE, 'unpack', b, d], check=True, capture_output=True)   # drop-in for subprocess.run

run_cmd takes a command line that starts with FbrbTool.exe's path (as callers build them for subprocess.run) and the
keyword arguments those callers pass (capture_output, text, check, stdout=..., ...): switching a caller is one line.
Output is text with '\n' line ends (as subprocess.run(..., text=True) gives FbrbTool's), or bytes with '\r\n' when text
is false. A call with cwd= runs the exe (its relative paths are the exe's to resolve).

BFBC2_FBRBTOOL_EXE=1 runs FbrbTool.exe instead (for comparison; FbrbTool.exe must be built: bfbc2-fbrb-tool\build.bat).
Without the Rust core (bfbc2core.native: not built, or built from other sources) the exe is used as well, with a
warning once - the exe stays the reference this round.

Each in-process run is logged as FbrbTool logs itself (%LOCALAPPDATA%\bfbc2-tool-suite\logs\fbrbtool-<date>.log), its
start line marked "(in process)".
"""
from __future__ import annotations

import datetime
import os
import subprocess
import sys
import warnings
from collections.abc import Iterable, Sequence
from typing import Any

from . import native

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
EXE = os.path.join(SUITE, 'bfbc2-fbrb-tool', 'FbrbTool.exe')
EXE_ENV = 'BFBC2_FBRBTOOL_EXE'
NATIVE_ATTR = 'fbrbtool_run'

Result = subprocess.CompletedProcess
_state: dict[str, Any] = {}


def use_exe() -> bool:
    """Whether FbrbTool.exe runs instead of the Rust core: BFBC2_FBRBTOOL_EXE=1, or no usable Rust core."""
    if os.environ.get(EXE_ENV, '') not in ('', '0'):
        return True
    m = native.load()
    if m is None or not hasattr(m, NATIVE_ATTR):
        if not _state.get('warned'):
            _state['warned'] = True
            warnings.warn('bfbc2core.fbrbtool: the Rust core is not available, FbrbTool.exe runs instead',
                          RuntimeWarning, stacklevel=3)
        return True
    return False


def log_path() -> str | None:
    """FbrbTool's log file for today (as FbrbTool's ToolLog names it), or None when the folder cannot be made."""
    try:
        base = os.environ.get('LOCALAPPDATA') or os.path.expanduser('~')
        d = os.path.join(base, 'bfbc2-tool-suite', 'logs')
        os.makedirs(d, exist_ok=True)
        return os.path.join(d, f'fbrbtool-{datetime.date.today().isoformat()}.log')
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


def _exe(args: Sequence[str], exe: str | None, text: bool) -> Result:
    return subprocess.run([exe or EXE, *args], capture_output=True, text=text)


def _native_run(args: Sequence[str]) -> tuple[int, str, str]:
    path = log_path()
    code, out, err, lines = native.require('FbrbTool in process', NATIVE_ATTR).fbrbtool_run(list(args), path)
    _log(path, args, lines)
    return code, out, err


def run(args: Sequence[str], *, text: bool = True, check: bool = False, exe: str | None = None) -> Result:
    """One FbrbTool command line (without the program name), in process unless use_exe(). check: raise
    subprocess.CalledProcessError on a non-zero exit code, as subprocess.run(check=True)."""
    args = [os.fspath(a) for a in args]
    r = _exe(args, exe, text) if use_exe() else _result(args, *_native_run(args), text)
    if check:
        r.check_returncode()
    return r


def run_many(runs: Sequence[Sequence[str]], *, text: bool = True) -> list[Result]:
    """Many command lines at once: in process on all cores (none may write a file or folder another reads or
    writes), else FbrbTool.exe one after another. Results in order."""
    runs = [[os.fspath(a) for a in r] for r in runs]
    if use_exe():
        return [_exe(r, None, text) for r in runs]
    path = log_path()
    outs = native.require('FbrbTool in process', NATIVE_ATTR).fbrbtool_run_many(runs, path)
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
    """subprocess.run(cmd, ...) for a command line starting with FbrbTool.exe's path: the command runs in process
    (unless use_exe() or cwd= is given: then that exe). Other programs run with subprocess.run as given.
    capture_output defaults to False, as subprocess.run's does; stdout= / stderr= (PIPE, DEVNULL, STDOUT, a file) are
    honoured as subprocess.run honours them."""
    cmd = [os.fspath(c) for c in cmd]
    as_text = bool(text or universal_newlines or encoding or errors)
    if not cmd or not is_fbrbtool(cmd[0]) or use_exe() or kwargs.get('cwd') is not None:
        return subprocess.run(cmd, capture_output=capture_output, text=bool(text or universal_newlines), check=check,
                              encoding=encoding, errors=errors, **kwargs)
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
