"""Error handling and logging for the suite's tools: every tool keeps a log, and when something goes wrong the user
is told what happened, the likely cause and what to do, in words rather than a traceback.

    log = errors.setup('navkit')                     # once, at start: log file, warnings, uncaught errors
    with errors.guard('navkit'):                     # around main(): a crash is logged and shown, exit code 1
        ...

Logs: %LOCALAPPDATA%\\bfbc2-tool-suite\\logs\\<tool>-<date>.log. Uncaught errors in any thread, Python warnings and
each tool's own messages go there; errors also show a window (GUI tools) or a short message (command line) naming
the log. A day's log reaching BFBC2_LOG_FILE_MB goes on in <tool>-<date>.1.log, .2.log ...; at each start the folder
is pruned (bfbc2core.caches.prune_logs: each tool's 20 newest days, BFBC2_LOG_TOOL_MB per tool, BFBC2_LOGS_MB in
all; never this session's files).
"""
import contextlib
import logging
import os
import sys
import threading
import traceback
import warnings
from collections.abc import Iterator
from types import TracebackType
from typing import Any, cast

from . import caches, paths

KEEP = caches.LOG_KEEP_DAYS
_state: dict[str, Any] = {'tool': None, 'path': None, 'gui': True}

# (test on the error, the cause in words, what to do)
CAUSES = [
    (lambda e: bool(getattr(e, 'what_to_do', None)),              # a tool's own error that says what to do
     lambda e: str(e),
     lambda e: e.what_to_do),
    (lambda e: isinstance(e, ModuleNotFoundError),
     lambda e: f'A Python library the tool needs is not installed ({e.name}).',
     lambda e: f'Install it: pip install {PIP_NAMES.get(e.name, e.name)}'),
    (lambda e: 'opengl' in str(e).lower() or 'glfw' in str(e).lower() or type(e).__module__.startswith(('moderngl', 'glfw')),
     lambda e: 'The 3D view could not start OpenGL 3.3 (the graphics driver is too old, or Python runs on a GPU without it).',
     lambda e: 'Update the graphics driver; on a laptop with two GPUs set python.exe to "High performance" in Windows '
               'Settings > System > Display > Graphics.'),
    (lambda e: isinstance(e, paths.GameCopyNotFound)                 # by type; by text for the tools' own errors
     or 'no game copy' in str(e).lower() or 'project opfor' in str(e).lower() and 'not found' in str(e).lower(),
     lambda e: 'The tools could not find Project OPFOR BFBC2\'s game copy.',
     lambda e: 'Put the suite folder next to the "project-opfor-bfbc2" folder, or set OPFOR_ROOT to it.'),
    (lambda e: 'not prepared' in str(e).lower() or 'unknown level' in str(e).lower(),
     lambda e: f'The level is not available: {e}',
     lambda e: 'Choose a level from the list, or prepare it first (Levels tab: Prepare level).'),
    (lambda e: isinstance(e, PermissionError),
     lambda e: f'A file could not be written or read: {getattr(e, "filename", "") or e}',
     lambda e: 'Close the program using it (the game, an editor), or check the folder is not read-only.'),
    (lambda e: isinstance(e, FileNotFoundError),
     lambda e: f'A file the tool needs is missing: {getattr(e, "filename", "") or e}',
     lambda e: 'Check the path; if it is a level\'s file, prepare the level again.'),
    (lambda e: isinstance(e, MemoryError),
     lambda e: 'The PC ran out of memory.',
     lambda e: 'Close other programs, or open a smaller level.'),
]
PIP_NAMES: dict[str, str] = {'imgui_bundle': 'imgui-bundle', 'PIL': 'pillow', 'capstone': 'capstone', 'pefile': 'pefile'}


def log_dir() -> str:
    base = os.environ.get('LOCALAPPDATA') or os.path.expanduser('~')
    d = os.path.join(base, 'bfbc2-tool-suite', 'logs')
    os.makedirs(d, exist_ok=True)
    return d


def explain(e: BaseException) -> tuple[str, str]:
    """(cause, what to do) for an exception, in words; a generic pair when it is not a known case."""
    for test, cause, fix in CAUSES:
        try:
            if test(e):
                return cause(e), fix(e)
        except Exception:
            continue
    return f'{type(e).__name__}: {e}', 'If it happens again, send the log file to the tool\'s author.'


class DayLog(logging.FileHandler):
    """A tool's day log: appends to caches.day_log's file and, once it reaches the per-file cap
    (BFBC2_LOG_FILE_MB), goes on in the day's next part - nothing renamed or truncated, so no line of the session is
    lost and a file another process holds is never in the way. files: every part this session wrote (kept by the
    pruning)."""

    def __init__(self, tool: str, folder: str) -> None:
        self.tool, self.folder = tool, folder
        self.limit = caches.log_caps()[2]
        path = caches.day_log(folder, tool, limit=self.limit)
        super().__init__(path, encoding='utf-8')
        self.files = [self.baseFilename]

    def emit(self, record: logging.LogRecord) -> None:
        try:                                        # before the line: a part is made only for a line written to it
            if self.stream is not None and self.stream.tell() >= max(self.limit, 1):
                self.roll()
        except Exception:                           # noqa: BLE001 - a log that cannot roll keeps its file
            pass
        super().emit(record)

    def roll(self) -> None:
        """Goes on in the day's next part (opened at the next line); the folder pruned, this session's parts kept."""
        stream, self.stream = self.stream, None
        if stream is not None:
            stream.close()
        path = os.path.abspath(caches.day_log(self.folder, self.tool, limit=self.limit))
        if path == self.baseFilename:               # removed or emptied meanwhile: room in it again
            return
        if _state.get('path') and os.path.abspath(_state['path']) == self.baseFilename:
            _state['path'] = path
        self.baseFilename = path
        self.files.append(path)
        with contextlib.suppress(Exception):
            caches.prune_logs(self.folder, keep=self.files)


def setup(tool: str, gui: bool = True, level: int = logging.INFO) -> logging.Logger:
    """The tool's log file, warnings into it, and uncaught errors (main thread and others) logged and reported."""
    _state.update(tool=tool, gui=gui)
    log = logging.getLogger('bfbc2')
    log.setLevel(level)
    folder = log_dir()
    mine = [h for h in log.handlers if isinstance(h, DayLog) and h.tool == tool
            and os.path.normcase(h.folder) == os.path.normcase(folder)]
    if mine:
        h: logging.Handler = mine[0]
    else:
        h = DayLog(tool, folder)
        h.setFormatter(logging.Formatter('%(asctime)s %(levelname)-7s %(message)s'))
        log.addHandler(h)
    _state['path'] = cast(DayLog, h).baseFilename
    logging.captureWarnings(True)
    wlog = logging.getLogger('py.warnings')
    for h in log.handlers:
        if h not in wlog.handlers:
            wlog.addHandler(h)
    warnings.simplefilter('default')
    sys.excepthook = lambda t, v, tb: report(v, tb=tb)
    threading.excepthook = lambda a: report(cast(BaseException, a.exc_value), tb=a.exc_traceback,
                                            where=f'thread {a.thread.name if a.thread else "?"}')
    log.info(f'--- {tool} started (Python {sys.version.split()[0]}, {sys.platform}) ---')
    _prune(tool)
    return log


def _prune(tool: str) -> None:
    """The log folder pruned (caches.prune_logs: each tool's KEEP newest days, then the size caps), the files of this
    process's logs kept. Never raises: a log folder that cannot be pruned is only bigger."""
    keep = [f for h in logger().handlers if isinstance(h, DayLog) for f in h.files]
    with contextlib.suppress(Exception):
        caches.prune_logs(log_dir(), keep=keep)


def logger() -> logging.Logger:
    return logging.getLogger('bfbc2')


def report(e: BaseException, tb: TracebackType | None = None, where: str | None = None,
           show: bool = True) -> str | None:
    """Logs an error with its traceback and tells the user (a window for GUI tools, else a short message)."""
    tool = _state['tool'] or 'tool'
    cause, fix = explain(e)
    lines = ''.join(traceback.format_exception(type(e), e, tb or e.__traceback__))
    logger().error(f'{tool} error{" in " + where if where else ""}: {cause}\n{lines}')
    text = f'{cause}\n\nWhat to do: {fix}\n\nDetails were saved to:\n{_state["path"] or "(no log file)"}'
    print(f'error: {cause}\n  what to do: {fix}\n  details: {_state["path"]}', file=sys.stderr)
    if show and _state['gui']:
        _window(f'{tool} - error', text)
    return _state['path']


def warn(message: str, show: bool = False) -> None:
    """A warning: logged, printed, and shown in a window when show is set (GUI tools)."""
    logger().warning(message)
    print(f'warning: {message}', file=sys.stderr)
    if show and _state['gui']:
        _window(f'{_state["tool"] or "tool"} - warning', message, kind='warning')


def _window(title: str, text: str, kind: str = 'error') -> None:
    if threading.current_thread() is not threading.main_thread():
        return                                  # Tk only on the main thread (a worker's error: printed and logged)
    try:
        import tkinter as tk
        from tkinter import messagebox
        root = tk._default_root  # type: ignore[attr-defined]   # private, not in the tkinter stubs
        own = root is None
        if own:
            root = tk.Tk()
            root.withdraw()
        (messagebox.showerror if kind == 'error' else messagebox.showwarning)(title, text, parent=root)
        if own:
            root.destroy()
    except Exception:
        pass                                    # no display: the message was printed and logged


@contextlib.contextmanager
def guard(tool: str | None = None, gui: bool | None = None) -> Iterator[None]:
    """Around a tool's main(): any error is logged and reported, and the tool exits with code 1."""
    if tool and _state['tool'] != tool:
        setup(tool, gui if gui is not None else True)
    try:
        yield
    except SystemExit:
        raise
    except KeyboardInterrupt:
        logger().info('stopped (Ctrl+C)')
        raise SystemExit(130) from None
    except BaseException as e:
        report(e)
        raise SystemExit(1) from e
