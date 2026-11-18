r"""DbxEdit's command line: xml, edit, import, check, roundtrip and fixslots, as DbxEdit.exe had them, run by the
suite's Rust core (bfbc2core.dbxedit; DbxEdit.exe was retired 2026-11-18). The same files, messages and exit codes.

    python dbxedit.py xml <file.dbx | folder> ...
    python dbxedit.py edit <in.dbx> <out.dbx> <command> ... | @<commands.txt>
    python dbxedit.py import <file.xml> <out.dbx> [--like <original.dbx>] [--types <file | folder>] ...
    python dbxedit.py check <level.dbx> [--slots-from <level.dbx>] [--bundle <folder>] ...
    python dbxedit.py roundtrip <file.dbx | folder>
    python dbxedit.py fixslots <in.dbx> <out.dbx> --from <level.dbx>

Without arguments it prints the usage. Needs the suite's Rust core (python rust\build_native.py --download, or build
it: README.md, "The Rust core"). DbxEditGUI.exe, the window, is built by build.bat.
"""
import os
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbxedit  # noqa: E402

if __name__ == '__main__':
    sys.exit(dbxedit.main(sys.argv[1:]))
