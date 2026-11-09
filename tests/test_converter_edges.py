r"""Edge cases of the converters and readers, without game data (quick tier). Each check is a regression test for a
fix of the round-3 audit (QUALITY_AUDIT.md):

- core dbx: a number field of an unusual size (not 2, 4 or 8 bytes) prints its first two bytes as an unsigned 16-bit
  number, as to_xml and DbxEdit do (it printed the whole value as one integer);
- dbxport: a reference that will not resolve is null (the template's value is never substituted for it: the Stage 1
  rewrite); taken from the template through a stand-in (STAND_INS: TEMPLATE), the template's own instance references
  are left out (a local guid would dangle in the ported partition) and its references into other partitions kept;
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
    def one(fields: list[dbx.Element], guid: str) -> bytes:
        inst = dbx.Element('instance', dbx.CONTAINER, [('guid', guid), ('type', 'X.Seat')], '', b'', fields)
        part = dbx.Element('partition', dbx.CONTAINER, [('guid', guid), ('primaryInstance', guid)], '', b'', [inst])
        return dbx.write(dbx.DbxFile([], [part]))
    g, tg = '00000000-0000-0000-0000-00000000000a', '00000000-0000-0000-0000-00000000000b'
    other = '00000000-0000-0000-0000-00000000000c'
    bf = one([dbx.Element('field', dbx.TEXT, [('ref', other), ('name', 'HudData')]),
              dbx.Element('field', dbx.TEXT, [('ref', f'effects/bf/{other}'), ('name', 'Effect')])], g)
    T = dbxport.Templates([('tpl', one([dbx.Element('field', dbx.TEXT, [('ref', tg), ('name', 'HudData')]),
                                        dbx.Element('field', dbx.TEXT, [('ref', f'effects/x/{tg}'), ('name', 'Effect')])],
                                       tg))])
    out, report = dbxport.port(bf, T, lambda p: p == 'effects/x')
    inst = dbx.parse(out).instances[0]
    hud, fx = inst.child('HudData'), inst.child('Effect')
    check(hud is not None and fx is not None and hud.attr('ref') == 'null' and fx.attr('ref') == 'null' and
          not report.of('substituted'), 'dbxport: an unresolved reference is null, no template value substituted',
          str(report))
    out, report = dbxport.port(bf, T, lambda p: p == 'effects/x',
                               stand_ins={('HudData', '*'): dbxport.TEMPLATE, ('Effect', '*'): dbxport.TEMPLATE})
    inst = dbx.parse(out).instances[0]
    hud, fx = inst.child('HudData'), inst.child('Effect')
    check(hud is not None and hud.attr('ref') == 'null' and report.of('nulled'),
          'dbxport: a stand-in template value does not keep the template\'s own instance', str(report))
    check(fx is not None and fx.attr('ref') == f'effects/x/{tg}',
          'dbxport: a stand-in template reference into another partition is kept')

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
