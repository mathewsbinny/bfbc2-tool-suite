"""A small level picker for the suite's 3D tools, shown when they are started without a level (a double-click,
or `python studio.py` alone): the levels of Project OPFOR BFBC2's game copy, double-click or OK to open one.

    lv = pick_level('BFBC2 Navigation Studio')      # the level name, or None when cancelled
"""
import os
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import contextlib

import suite_paths  # noqa: E402


def levels() -> list[tuple[str, str]]:
    suite_paths.use('navkit')
    import navkit
    pr = navkit.Project()
    out = []
    for lv in pr.level_dirs():
        prepared = os.path.isdir(os.path.join(pr.work, 'levels', lv, 'dist'))
        out.append((lv, ('prepared' if prepared else 'not prepared yet - about 20 s the first time') +
                    (', DICE path data' if pr.dice_pathdata(lv) else '')))
    return out


def pick_level(title: str) -> str | None:
    import tkinter as tk
    from tkinter import messagebox, ttk
    try:
        import ctypes
        ctypes.windll.shcore.SetProcessDpiAwareness(1)      # sharp on scaled displays
    except (AttributeError, OSError):
        pass
    root = tk.Tk()
    root.title(title)
    root.minsize(460, 420)
    with contextlib.suppress(tk.TclError):
        ttk.Style().theme_use('vista')
    try:
        items = levels()
    except Exception as e:
        messagebox.showerror(title, f'No game copy found: {e}\nPut the suite next to the "project-opfor-bfbc2" folder, '
                                    f'or set OPFOR_ROOT.')
        root.destroy()
        return None
    chosen: list[str] = []
    ttk.Label(root, text='Choose a level to open:', padding=(10, 10, 10, 4)).pack(anchor='w')
    tree = ttk.Treeview(root, columns=('state',), selectmode='browse', height=16)
    tree.heading('#0', text='Level')
    tree.heading('state', text='')
    tree.column('#0', width=140)
    tree.column('state', width=300)
    for lv, state in items:
        tree.insert('', 'end', iid=lv, text=lv, values=(state,))
    tree.pack(fill='both', expand=True, padx=10)

    def ok(*_: object) -> None:
        sel = tree.selection()
        if sel:
            chosen.append(sel[0])
            root.destroy()
    tree.bind('<Double-1>', ok)
    tree.bind('<Return>', ok)
    b = ttk.Frame(root, padding=10)
    b.pack(fill='x')
    ttk.Button(b, text='Open', command=ok).pack(side='right')
    ttk.Button(b, text='Cancel', command=root.destroy).pack(side='right', padx=6)
    root.mainloop()
    return chosen[0] if chosen else None
