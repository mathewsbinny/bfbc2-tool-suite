r"""FbrbTool's command line: unpack, pack, list and big, as FbrbTool.exe had them, run by the suite's Rust core
(bfbc2core.fbrbtool; FbrbTool.exe was retired 2026-11-18). The same files, messages and exit codes.

    python fbrbtool.py unpack <bundle.fbrb> <folder>
    python fbrbtool.py pack <folder> <bundle.fbrb>
    python fbrbtool.py list <bundle.fbrb>
    python fbrbtool.py big <archive.big> <folder>

Needs the suite's Rust core (python rust\build_native.py --download, or build it: README.md, "The Rust core").
"""
import os
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import fbrbtool  # noqa: E402

if __name__ == '__main__':
    sys.exit(fbrbtool.main(sys.argv[1:]))
