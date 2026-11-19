r"""The suite's application icon (round I1), found as the Rust builds find it (rust\app_icon.rs, the .exe's own icon)
and the map editor finds it for its window (rust\bc2-editor\src\appicon.rs):

    1. BFBC2_APP_ICON              a path to an .ico
    2. design\icons\local\app.ico  a personal override (gitignored, never committed)
    3. design\icons\app.ico        the committed icon (the author's own M67 render: design\icons\README.md)
    4. none                        no icon, no error

apply(window) gives a tkinter window (the hub) that icon; with none found nothing changes.
"""
import os
from typing import Any

SUITE = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ENV = 'BFBC2_APP_ICON'
LOCAL = os.path.join(SUITE, 'design', 'icons', 'local', 'app.ico')
COMMITTED = os.path.join(SUITE, 'design', 'icons', 'app.ico')


def candidates() -> list[str]:
    """Every path tried, in order: BFBC2_APP_ICON (when set), the local icon, the committed one."""
    env = os.environ.get(ENV)
    return ([env] if env else []) + [LOCAL, COMMITTED]


def find() -> str | None:
    """The first of candidates() that is a file, or None."""
    return next((p for p in candidates() if os.path.isfile(p)), None)


def apply(window: Any) -> str | None:
    """Sets the icon of a tkinter window (wm iconbitmap) and of the windows it opens later (iconbitmap's default=,
    Windows); returns the icon's path, or None when there is none or Tk cannot read it (the window keeps Tk's own
    icon)."""
    import tkinter as tk
    path = find()
    if path is None:
        return None
    try:
        window.iconbitmap(path)
        window.iconbitmap(default=path)
    except tk.TclError:
        return None
    return path
