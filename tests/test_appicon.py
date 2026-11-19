r"""The application icon (round I1): the lookup, the hub's window icon and the icon the Rust binaries carry.

  python tests\test_appicon.py [--exe <path> ...] [--ico <path> | --none]
  python tests\test_appicon.py --editor bin\bfbc2-editor.exe [--ico <path> | --none [--ref <path>]]

  - the lookup (bfbc2core.appicon, as rust\app_icon.rs and rust\bc2-editor\src\appicon.rs): BFBC2_APP_ICON, then
    design\icons\local\app.ico, then design\icons\app.ico, then none; a BFBC2_APP_ICON that is not a file is passed over;
  - the hub (BFBC2 Tool Suite.pyw) gives its Tk window the icon found, on a window never shown; with none nothing
    changes;
  - the binaries in bin\ (bfbc2.exe, bfbc2-editor.exe, mapexport.exe): the icon group in each .exe holds exactly the
    found .ico's pictures (size, colour depth and bytes), or there is no icon group when there is no icon. An .exe
    built before the icon last changed is only a note (rebuild with rust\build_native.py).

--exe checks the given .exe files instead, against --ico (or the lookup), or --none: they must have no icon group;
a mismatch is then always a failure. --editor starts the map editor for a few seconds (a window opens) and checks its
running window's icon against the .ico's largest picture (Pillow). Needs pefile for the binary checks (skipped
without it). No game data.
"""
import os
import shutil
import struct
import sys
import tempfile
import zlib

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import appicon, native  # noqa: E402

RT_ICON, RT_GROUP_ICON = 3, 14
BINARIES = ('bfbc2', 'bfbc2-editor', 'mapexport')
MATCH = 6.0     # the editor's window icon against the .ico scaled to its size: mean RGBA difference, in levels
failed = 0


def check(ok: bool, what: str) -> None:
    global failed
    failed += not ok
    print(f'{"ok  " if ok else "FAIL"} {what}')


def png_of(size: int, rgba: tuple[int, int, int, int]) -> bytes:
    """A size x size PNG of one colour."""
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    rows = b''.join(b'\x00' + bytes(rgba) * size for _ in range(size))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def ico_of(pngs: list[tuple[int, bytes]]) -> bytes:
    """An .ico of PNG pictures."""
    head = struct.pack('<HHH', 0, 1, len(pngs))
    offset, dirs, body = 6 + 16 * len(pngs), b'', b''
    for size, png in pngs:
        side = 0 if size >= 256 else size
        dirs += struct.pack('<BBBBHHII', side, side, 0, 0, 1, 32, len(png), offset)
        offset += len(png)
        body += png
    return head + dirs + body


def ico_images(data: bytes) -> list[tuple[int, int, int, bytes]]:
    """(width, height, bits per pixel, bytes) of each picture of an .ico."""
    _, _, count = struct.unpack_from('<HHH', data, 0)
    out = []
    for i in range(count):
        w, h, _, _, _, bpp, size, off = struct.unpack_from('<BBBBHHII', data, 6 + 16 * i)
        out.append((w or 256, h or 256, bpp, data[off:off + size]))
    return out


def exe_icons(path: str) -> list[list[tuple[int, int, int, bytes]]]:
    """Each icon group of an .exe's resources as the pictures it names (as ico_images)."""
    import pefile
    pe = pefile.PE(path, fast_load=True)
    pe.parse_data_directories(directories=[pefile.DIRECTORY_ENTRY['IMAGE_DIRECTORY_ENTRY_RESOURCE']])
    root = getattr(pe, 'DIRECTORY_ENTRY_RESOURCE', None)
    if root is None:
        return []

    def leaves(kind: int) -> dict[int, bytes]:
        out = {}
        for t in root.entries:
            if t.id != kind:
                continue
            for name in t.directory.entries:
                for lang in name.directory.entries:
                    d = lang.data.struct
                    out[name.id] = pe.get_data(d.OffsetToData, d.Size)
        return out

    icons, groups = leaves(RT_ICON), leaves(RT_GROUP_ICON)
    out = []
    for _, g in sorted(groups.items()):
        _, _, count = struct.unpack_from('<HHH', g, 0)
        pics = []
        for i in range(count):
            w, h, _, _, _, bpp, _, nid = struct.unpack_from('<BBBBHHIH', g, 6 + 14 * i)
            pics.append((w or 256, h or 256, bpp, icons.get(nid, b'')))
        out.append(pics)
    pe.close()
    return out


def window_icon(window: object) -> tuple[int, int, set[tuple[int, int, int, int]]] | None:
    """The Win32 big icon a Tk window has (WM_GETICON on its frame, never shown): its width, height and the RGBA
    colours of its pixels; None when it has none."""
    window.update_idletasks()  # type: ignore[attr-defined]
    got = hwnd_icon(int(window.wm_frame(), 16), big=True)  # type: ignore[attr-defined]
    if got is None:
        return None
    w, h, raw = got
    return w, h, {(raw[i], raw[i + 1], raw[i + 2], raw[i + 3]) for i in range(0, len(raw), 4)}


def hwnd_icon(hwnd: int, big: bool) -> tuple[int, int, bytes] | None:
    """A window's icon (WM_GETICON, ICON_BIG or ICON_SMALL): width, height and its pixels as RGBA rows top down;
    None when it has none."""
    import ctypes
    from ctypes import wintypes
    u, g = ctypes.windll.user32, ctypes.windll.gdi32
    u.SendMessageW.restype = ctypes.c_void_p
    u.SendMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    hicon = u.SendMessageW(hwnd, 0x7F, 1 if big else 0, 0)  # WM_GETICON
    if not hicon:
        return None

    class ICONINFO(ctypes.Structure):
        _fields_ = [('fIcon', wintypes.BOOL), ('x', wintypes.DWORD), ('y', wintypes.DWORD),
                    ('mask', wintypes.HBITMAP), ('color', wintypes.HBITMAP)]

    class BITMAP(ctypes.Structure):
        _fields_ = [('type', ctypes.c_long), ('w', ctypes.c_long), ('h', ctypes.c_long), ('wb', ctypes.c_long),
                    ('planes', wintypes.WORD), ('bpp', wintypes.WORD), ('bits', ctypes.c_void_p)]

    info = ICONINFO()
    u.GetIconInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(ICONINFO)]
    if not u.GetIconInfo(ctypes.c_void_p(hicon), ctypes.byref(info)):
        return None
    try:
        bm = BITMAP()
        g.GetObjectW.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p]
        g.GetObjectW(info.color, ctypes.sizeof(bm), ctypes.byref(bm))
        w, h = bm.w, bm.h
        header = struct.pack('<IiiHHIIiiII', 40, w, -h, 1, 32, 0, 0, 0, 0, 0, 0)
        bmi = ctypes.create_string_buffer(header + b'\x00' * 16)
        pixels = ctypes.create_string_buffer(w * h * 4)
        u.GetDC.restype = ctypes.c_void_p
        dc = u.GetDC(None)
        g.GetDIBits.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint, ctypes.c_uint, ctypes.c_void_p,
                                ctypes.c_void_p, ctypes.c_uint]
        g.GetDIBits(dc, info.color, 0, h, pixels, bmi, 0)
        u.ReleaseDC.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        u.ReleaseDC(None, dc)
        raw = bytearray(pixels.raw)
        raw[0::4], raw[2::4] = raw[2::4], raw[0::4]       # BGRA -> RGBA
        return w, h, bytes(raw)
    finally:
        g.DeleteObject.argtypes = [ctypes.c_void_p]
        g.DeleteObject(info.mask)
        g.DeleteObject(info.color)


def lookup() -> None:
    saved = (appicon.LOCAL, appicon.COMMITTED, os.environ.get(appicon.ENV))
    tmp = tempfile.mkdtemp(prefix='appicon_')
    try:
        local, committed, given = (os.path.join(tmp, n) for n in ('local.ico', 'committed.ico', 'given.ico'))
        appicon.LOCAL, appicon.COMMITTED = local, committed
        os.environ.pop(appicon.ENV, None)
        check(appicon.find() is None, 'lookup: no icon anywhere is none')
        for p in (committed, local, given):
            with open(p, 'wb') as f:
                f.write(ico_of([(16, png_of(16, (1, 2, 3, 255)))]))
        check(appicon.find() == local, 'lookup: the local icon before the committed one')
        os.remove(local)
        check(appicon.find() == committed, 'lookup: the committed icon when there is no local one')
        os.environ[appicon.ENV] = given
        check(appicon.find() == given, 'lookup: BFBC2_APP_ICON first')
        os.environ[appicon.ENV] = os.path.join(tmp, 'missing.ico')
        check(appicon.find() == committed, 'lookup: a BFBC2_APP_ICON that is not a file is passed over')
        real = saved[0].replace(os.sep, '/')
        check(real.endswith('/design/icons/local/app.ico') and saved[1].replace(os.sep, '/').endswith('/design/icons/app.ico'),
              'lookup: the places are design\\icons\\local\\app.ico and design\\icons\\app.ico')
    finally:
        appicon.LOCAL, appicon.COMMITTED = saved[0], saved[1]
        if saved[2] is None:
            os.environ.pop(appicon.ENV, None)
        else:
            os.environ[appicon.ENV] = saved[2]
        shutil.rmtree(tmp, ignore_errors=True)


def hub() -> None:
    import importlib.util
    import tkinter as tk
    spec = importlib.util.spec_from_file_location('suite_hub', os.path.join(SUITE, 'BFBC2 Tool Suite.pyw'))
    assert spec is not None and spec.loader is not None
    hubmod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(hubmod)
    saved = (appicon.LOCAL, appicon.COMMITTED, os.environ.get(appicon.ENV))
    tmp = tempfile.mkdtemp(prefix='appicon_hub_')
    try:
        icon = os.path.join(tmp, 'app.ico')
        with open(icon, 'wb') as f:
            f.write(ico_of([(16, png_of(16, (200, 40, 40, 255))), (32, png_of(32, (200, 40, 40, 255)))]))
        appicon.LOCAL, appicon.COMMITTED = icon, os.path.join(tmp, 'none.ico')
        os.environ.pop(appicon.ENV, None)
        # the window is never shown: withdrawn before anything is drawn; the Hub itself (its tabs, the level list)
        # is not built, only what it does to the window's icon at its start (Hub.__init__ calls appicon.apply)
        root = tk.Tk()
        root.withdraw()
        try:
            check(hubmod.appicon is appicon, "hub: the window icon comes from bfbc2core.appicon's lookup")
            before = window_icon(root)
            check(appicon.apply(root) == icon, 'hub: apply() finds the icon')
            after = window_icon(root)
            # Windows rounds a few pixels by one level (premultiplied alpha)
            near = after is not None and all(max(abs(a - b) for a, b in zip(c, (200, 40, 40, 255))) <= 1
                                             for c in after[2])
            check(before is None and after is not None and after[:2] in ((16, 16), (32, 32)) and near,
                  f"hub: the Tk window (never shown) has the .ico's picture as its Win32 icon (WM_GETICON: "
                  f"{after[:2] if after else None}, every pixel the icon's colour within one level)")
        finally:
            root.destroy()
        appicon.LOCAL = os.path.join(tmp, 'gone.ico')
        root = tk.Tk()
        root.withdraw()
        try:
            check(appicon.apply(root) is None and window_icon(root) is None,
                  'hub: no icon found, the window keeps Tk\'s own (nothing set, no error)')
        finally:
            root.destroy()
        src = open(os.path.join(SUITE, 'BFBC2 Tool Suite.pyw'), encoding='utf-8').read()
        check('appicon.apply(root)' in src, 'hub: Hub.__init__ applies the icon to its window')
    finally:
        appicon.LOCAL, appicon.COMMITTED = saved[0], saved[1]
        if saved[2] is not None:
            os.environ[appicon.ENV] = saved[2]
        shutil.rmtree(tmp, ignore_errors=True)


def binary(path: str, ico: str | None, strict: bool) -> None:
    name = os.path.basename(path)
    groups = exe_icons(path)
    if ico is None:
        ok = not groups
        if ok or strict:
            check(ok, f'{name}: no icon found, no icon group in the .exe')
        else:
            print(f'note {name}: has an icon group but no icon is found now (built before it was removed?); '
                  f'rebuild with rust\\build_native.py')
        return
    want = ico_images(open(ico, 'rb').read())
    ok = len(groups) == 1 and groups[0] == want
    if not ok and not strict and os.path.getmtime(path) < os.path.getmtime(ico):
        print(f'note {name}: built before {ico} last changed; rebuild with rust\\build_native.py')
        return
    sizes = ', '.join(f'{w}x{h}/{b}' for w, h, b, _ in want)
    check(ok, f'{name}: one icon group, its pictures the .ico\'s byte for byte ({sizes}; '
              f'{sum(len(d) for *_, d in want):,} bytes)')


def top_window(pid: int, title: str) -> int | None:
    """The visible top-level window of process pid whose title starts with title."""
    import ctypes
    from ctypes import wintypes
    u = ctypes.windll.user32
    found: list[int] = []
    proc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def each(hwnd: int, _: int) -> bool:
        owner = wintypes.DWORD()
        u.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and u.IsWindowVisible(hwnd):
            buf = ctypes.create_unicode_buffer(256)
            u.GetWindowTextW(hwnd, buf, 256)
            if buf.value.startswith(title):
                found.append(hwnd)
        return True

    u.EnumWindows(proc(each), 0)
    return found[0] if found else None


def editor(exe: str, ico: str | None, ref: str | None) -> None:
    r"""Starts the map editor for a few seconds and reads its window's icon (WM_GETICON): with an icon (passed as
    BFBC2_APP_ICON), the .ico's largest picture scaled to the window's icon size; with --none (BFBC2_APP_ICON unset:
    run it with no icon in design\icons), not --ref's picture."""
    import io
    import subprocess
    import time

    from PIL import Image
    env = dict(os.environ)
    env.pop(appicon.ENV, None)
    if ico:
        env[appicon.ENV] = ico
    p = subprocess.Popen([exe], env=env)
    try:
        hwnd, end = None, time.monotonic() + 90
        while hwnd is None and time.monotonic() < end and p.poll() is None:
            time.sleep(0.5)
            hwnd = top_window(p.pid, 'BFBC2 Map Editor')
        if hwnd is None:
            check(False, f'{os.path.basename(exe)}: its window opened (exit code {p.poll()})')
            return
        time.sleep(1.0)
        icons = {k: hwnd_icon(hwnd, big=k == 'big') for k in ('small', 'big')}
        print('      window icons: ' + ', '.join(f'{k} {v[0]}x{v[1]}' if v else f'{k} none' for k, v in icons.items()))
        if ico is None and ref is None:
            check(True, f'{os.path.basename(exe)}: no icon found, the window opened (no error)')
            return
        largest = max(ico_images(open(ico or ref or '', 'rb').read()), key=lambda i: i[0])
        want = Image.open(io.BytesIO(largest[3])).convert('RGBA')
        matched = []
        for k, v in icons.items():
            if v is None:
                continue
            w, h, raw = v
            scaled = want if want.size == (w, h) else want.resize((w, h), Image.Resampling.LANCZOS)
            exp = scaled.tobytes()
            mean = sum(abs(a - b) for a, b in zip(raw, exp)) / len(exp)
            print(f'      {k}: {w}x{h}, mean difference from the .ico\'s {largest[0]}x{largest[1]} picture '
                  f'scaled to it: {mean:.2f} levels')
            if mean <= MATCH:
                matched.append(k)
        if ico is None:
            check(not matched, f"{os.path.basename(exe)}: no icon found - the running window's icon is not {ref}'s "
                               f"(no error)")
            return
        check(bool(matched), f"{os.path.basename(exe)}: the running window's icon ({', '.join(matched) or 'none'}) is "
                             f"the .ico's {largest[0]}x{largest[1]} picture (Windows scales it to the window's DPI; "
                             f"mean difference at most {MATCH} levels)")
    finally:
        p.kill()
        p.wait()


def main(argv: list[str]) -> int:
    if '--editor' in argv:
        ico = None if '--none' in argv else (argv[argv.index('--ico') + 1] if '--ico' in argv else appicon.find())
        ref = argv[argv.index('--ref') + 1] if '--ref' in argv else None
        editor(argv[argv.index('--editor') + 1], ico, ref)
        print('all passed' if not failed else f'{failed} check(s) failed')
        return 1 if failed else 0
    exes = [argv[i + 1] for i, a in enumerate(argv) if a == '--exe' and i + 1 < len(argv)]
    lookup()
    try:
        hub()
    except ImportError as err:
        print(f'skip the hub: {err}')
    try:
        import pefile  # noqa: F401
    except ImportError:
        print('skip the binaries: pefile is not installed (pip install pefile)')
        return 1 if failed else 0
    if exes:
        ico = None if '--none' in argv else (argv[argv.index('--ico') + 1] if '--ico' in argv else appicon.find())
        for exe in exes:
            binary(exe, ico, strict=True)
    else:
        found = [p for p in (native.bin_path(n) for n in BINARIES) if os.path.isfile(p)]
        if not found:
            print('skip the binaries: none built in bin\\ (rust\\build_native.py, --editor)')
        for exe in found:
            binary(exe, appicon.find(), strict=False)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
