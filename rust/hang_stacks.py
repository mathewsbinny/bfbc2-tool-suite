"""Where each thread of a hung process waits (round HG1): `python rust\\hang_stacks.py <pid> [dump file]`.

The editor test binary's hang guard (`rust/bc2-editor/tests/main.rs`, `BC2_TEST_HANG_STACKS=1`) runs it on itself
before it ends. It writes a full minidump of the process with Windows' dbghelp (out of process: nothing is injected,
so it works when the target holds the loader lock, where a debugger-agent tool such as Frida hangs too), then prints,
per thread, the return addresses found on its stack, named from the modules' PDBs (a test build keeps line tables, so
the Rust functions are named; system DLLs by their exports). It also names the thread holding the loader lock.

A scan of the stack, not an unwind: an address may be left over from an earlier call, but every real frame is there.
The dump is deleted unless a dump file is given.
"""

from __future__ import annotations

import bisect
import ctypes
import ctypes.wintypes as wt
import os
import struct
import sys
import tempfile

PROCESS_ALL = 0x1F0FFF
MINIDUMP_WITH_FULL_MEMORY = 0x2


class SymbolInfo(ctypes.Structure):
    """dbghelp's SYMBOL_INFO with room for the name."""

    _fields_ = [('SizeOfStruct', wt.ULONG), ('TypeIndex', wt.ULONG), ('Reserved', ctypes.c_ulonglong * 2),
                ('Index', wt.ULONG), ('Size', wt.ULONG), ('ModBase', ctypes.c_ulonglong), ('Flags', wt.ULONG),
                ('Value', ctypes.c_ulonglong), ('Address', ctypes.c_ulonglong), ('Register', wt.ULONG),
                ('Scope', wt.ULONG), ('Tag', wt.ULONG), ('NameLen', wt.ULONG), ('MaxNameLen', wt.ULONG),
                ('Name', ctypes.c_char * 1024)]


def write_dump(pid: int, path: str) -> None:
    """A full-memory minidump of `pid` at `path`."""
    k32 = ctypes.WinDLL('kernel32', use_last_error=True)
    k32.OpenProcess.restype = wt.HANDLE
    dbg = ctypes.WinDLL('dbghelp.dll', use_last_error=True)
    dbg.MiniDumpWriteDump.argtypes = [wt.HANDLE, wt.DWORD, wt.HANDLE, wt.DWORD, ctypes.c_void_p, ctypes.c_void_p,
                                      ctypes.c_void_p]
    h = k32.OpenProcess(PROCESS_ALL, False, pid)
    if not h:
        raise OSError(f'cannot open process {pid}: error {ctypes.get_last_error()}')
    try:
        with open(path, 'wb') as f:
            import msvcrt  # noqa: PLC0415 - Windows only, like the tool
            fh = msvcrt.get_osfhandle(f.fileno())
            if not dbg.MiniDumpWriteDump(h, pid, fh, MINIDUMP_WITH_FULL_MEMORY, None, None, None):
                raise OSError(f'MiniDumpWriteDump failed: error {ctypes.get_last_error()}')
    finally:
        k32.CloseHandle(h)


class Dump:
    """A minidump's threads, modules and memory (streams 3, 4 and 9)."""

    def __init__(self, path: str) -> None:
        with open(path, 'rb') as f:
            self.data = data = f.read()
        sig, _, n, rva = struct.unpack_from('<4sIII', data, 0)
        if sig != b'MDMP':
            raise ValueError('not a minidump')
        streams = {}
        for i in range(n):
            t, size, at = struct.unpack_from('<III', data, rva + 12 * i)
            streams[t] = (size, at)
        self.modules: list[tuple[int, int, str]] = []
        at = streams[4][1]
        for i in range(struct.unpack_from('<I', data, at)[0]):
            o = at + 4 + 108 * i
            base, size, _, _, name_rva = struct.unpack_from('<QIIII', data, o)
            ln = struct.unpack_from('<I', data, name_rva)[0]
            self.modules.append((base, size, data[name_rva + 4:name_rva + 4 + ln].decode('utf-16-le')))
        self.ranges: list[tuple[int, int, int]] = []
        at = streams[9][1]
        count, off = struct.unpack_from('<QQ', data, at)
        for i in range(count):
            start, size = struct.unpack_from('<QQ', data, at + 16 + 16 * i)
            self.ranges.append((start, size, off))
            off += size
        self.ranges.sort()
        self.starts = [r[0] for r in self.ranges]
        # (thread id, TEB, stack start, stack size, rsp)
        self.threads: list[tuple[int, int, int, int, int]] = []
        at = streams[3][1]
        for i in range(struct.unpack_from('<I', data, at)[0]):
            o = at + 4 + 48 * i
            tid = struct.unpack_from('<I', data, o)[0]
            teb = struct.unpack_from('<Q', data, o + 16)[0]
            start, size = struct.unpack_from('<QI', data, o + 24)
            ctx = struct.unpack_from('<I', data, o + 44)[0]
            rsp = struct.unpack_from('<Q', data, ctx + 0x98)[0]
            self.threads.append((tid, teb, start, size, rsp))

    def read(self, addr: int, size: int) -> bytes | None:
        k = bisect.bisect_right(self.starts, addr) - 1
        if k < 0:
            return None
        start, length, off = self.ranges[k]
        if addr + size > start + length:
            return None
        return self.data[off + addr - start:off + addr - start + size]

    def loader_lock_owner(self) -> int | None:
        """The thread holding the loader lock (PEB.LoaderLock's OwningThread), if any."""
        if not self.threads:
            return None
        teb = self.threads[0][1]
        peb = self.read(teb + 0x60, 8)
        lock = peb and self.read(struct.unpack('<Q', peb)[0] + 0x110, 8)
        cs = lock and self.read(struct.unpack('<Q', lock)[0], 24)
        if not cs:
            return None
        owner = struct.unpack_from('<Q', cs, 16)[0]
        return owner or None


class Namer:
    """Names addresses with dbghelp from the modules' files and PDBs, loaded at the dump's bases."""

    def __init__(self, modules: list[tuple[int, int, str]]) -> None:
        dbg = ctypes.WinDLL('dbghelp.dll')
        dbg.SymInitializeW.argtypes = [wt.HANDLE, wt.LPCWSTR, wt.BOOL]
        dbg.SymLoadModuleExW.argtypes = [wt.HANDLE, wt.HANDLE, wt.LPCWSTR, wt.LPCWSTR, ctypes.c_ulonglong,
                                         wt.DWORD, ctypes.c_void_p, wt.DWORD]
        dbg.SymLoadModuleExW.restype = ctypes.c_ulonglong
        dbg.SymFromAddr.argtypes = [wt.HANDLE, ctypes.c_ulonglong, ctypes.POINTER(ctypes.c_ulonglong),
                                    ctypes.POINTER(SymbolInfo)]
        self.dbg = dbg
        self.key = wt.HANDLE(0x48_4731)  # dbghelp only uses the handle as a key here
        dbg.SymSetOptions(0x2)  # undecorated names
        dbg.SymInitializeW(self.key, None, False)
        self.modules = modules
        for base, size, path in modules:
            dbg.SymLoadModuleExW(self.key, None, path, None, base, size, None, 0)

    def name(self, a: int) -> str | None:
        mod = next((m for m in self.modules if m[0] <= a < m[0] + m[1]), None)
        if mod is None:
            return None
        short = os.path.basename(mod[2])
        s = SymbolInfo()
        s.SizeOfStruct = ctypes.sizeof(SymbolInfo) - 1024
        s.MaxNameLen = 1024
        off = ctypes.c_ulonglong(0)
        if self.dbg.SymFromAddr(self.key, a, ctypes.byref(off), ctypes.byref(s)):
            return f'{short}!{s.Name.decode(errors="replace")}+{off.value:#x}'
        return f'{short}!{a:#x}'


def report(dump: Dump, most: int = 40) -> None:
    namer = Namer(dump.modules)
    owner = dump.loader_lock_owner()
    print(f'hang_stacks: {len(dump.threads)} threads; the loader lock is held by thread {owner}')
    for tid, _, start, size, rsp in dump.threads:
        print(f'--- thread {tid}' + ('  (holds the loader lock)' if tid == owner else ''))
        last, shown = None, 0
        for a in range(rsp, start + size, 8):
            b = dump.read(a, 8)
            if b is None or shown >= most:
                break
            n = namer.name(struct.unpack('<Q', b)[0])
            if n and n != last and not n.endswith('+0x0'):
                print('   ', n)
                last, shown = n, shown + 1


def main(argv: list[str]) -> int:
    if len(argv) not in (2, 3) or not argv[1].isdigit():
        print(__doc__)
        return 2
    keep = argv[2] if len(argv) == 3 else None
    path = keep or os.path.join(tempfile.gettempdir(), f'hang_stacks_{argv[1]}.dmp')
    write_dump(int(argv[1]), path)
    try:
        report(Dump(path))
    finally:
        if keep is None:
            os.remove(path)
    sys.stdout.flush()
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
