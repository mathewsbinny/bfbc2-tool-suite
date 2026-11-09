r"""Edge cases of the converters and readers, without game data (quick tier). Each check is a regression test for a
fix of the round-3 audit (QUALITY_AUDIT.md):

- core dbx: a number field of an unusual size (not 2, 4 or 8 bytes) prints its first two bytes as an unsigned 16-bit
  number, as to_xml and DbxEdit do (it printed the whole value as one integer);
- dbxport: a field taking the template's value (BF1943's reference would not resolve) loses the template's own
  instance references (a local guid would dangle in the ported partition);
- console texture check: a format without a block decoder (RGBA16F) is not measured (it raised IndexError).

  python tests\test_converter_edges.py
"""
import os
import struct
import sys

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'bf1943-import', 'console-convert')
import dbxport  # noqa: E402
import texture  # noqa: E402
from bfbc2core import dbx  # noqa: E402


def main(argv: list[str]) -> int:
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    raw = bytes([0x01, 0x02]) + bytes(14)
    got = dbx._number_text(raw, 16, '')
    check(got == '258', 'dbx: a 16-byte number prints its first two bytes (as to_xml / DbxEdit)', got)
    check(dbx._number_text(bytes([0x12, 0x34]), 2, '') == '4660', 'dbx: a 2-byte number unchanged')

    # BF1943's field refers to an instance that will not resolve; the template's value names its own instance
    e = dbx.Element('field', dbx.TEXT, [('name', 'HudData'), ('ref', 'bf1943-guid-0001')])
    t = dbx.Element('field', dbx.TEXT, [('name', 'HudData'), ('ref', 'template-local-guid')])
    report: list[str] = []
    out = dbxport._align(e, t, 'Seat.HudData', report, resolve=lambda target: False)
    check(out.attr('ref') == 'null' and any(r.startswith('nulled') for r in report),
          'dbxport: a substituted template value does not keep the template\'s own instance', f'{out.attr("ref")} {report}')
    t2 = dbx.Element('field', dbx.TEXT, [('name', 'Effect'), ('ref', 'effects/x/y/0000-guid')])
    out2 = dbxport._align(e, t2, 'Weapon.Effect', [], resolve=lambda target: False)
    check(out2.attr('ref') == 'effects/x/y/0000-guid', 'dbxport: a substituted reference into another partition is kept')

    sizes = [16 * 16 * 8, 8 * 8 * 8]                                    # two levels: the mip chain is compared
    hdr = struct.pack('<8I', 0, 0, 13, 0, 16, 16, 1, 2) + struct.pack('<15I', *sizes, *([0] * 13))
    pc = hdr.ljust(texture.HEADER, b'\0') + bytes(sum(sizes))
    try:
        res = texture.check(pc)
    except IndexError as ex:
        res = f'IndexError {ex}'
    check(res == (0.0, None, None, None), 'texture check: RGBA16F is not measured (it raised IndexError)', str(res))
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
