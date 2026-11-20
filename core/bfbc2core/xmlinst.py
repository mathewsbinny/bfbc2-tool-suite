"""The <instance> elements of a converted level partition (DbxEdit's XML) and their fields' references, whatever
the attribute order.

Attribute order varies by author at both levels: DICE's files write <instance guid="..." type="..."> and
<field ref="..." name="...">, Napisal's Onslaught levels <instance type="..." guid="..."> and mostly <field
name="..." ref="..."> (ONS_MP_002: 17,454 name-first, 118 ref-first). Patterns that expected one order read
ONS_MP_002 as 0 placed objects, 0 soldier spawns and 0 AI orders (2026-10-20; spec KYNAPSE_SPEC.md section 4.5).
Any XML-matching code should go through instances() and field_ref() rather than its own patterns; one-attribute
elements (<complex name=...>, <array name=...>, <item ref=...>) are not affected.

Both run in the suite's Rust core (round F2b, rust/bc2-formats/src/xmlinst.rs: the patterns <instance\\b([^>]*)>
(.*?)</instance>, (\\w+)="([^"]*)" and <field\\b([^>]*?)/?> matched by hand with their exact semantics), which they
need (bfbc2core.native.NativeMissing says how to get it). Where a match would rest on whether a non-ASCII character
is a word character (Python's \\w is Unicode-aware: an attribute or tag name with such a character) they raise
bfbc2core.native.Refused (a ValueError); none of the 791 converted XML files of OPFOR's work levels has one. The
patterns' Python (instances_reference, field_ref_reference) was retired in round RT2 against its recorded answers
(core/tests/f2b_xmlinst_replay*.json)."""
from collections.abc import Iterator
from types import ModuleType


def _core() -> ModuleType:
    """The Rust core (bfbc2core.native.require: NativeMissing, saying how to get it, when it is not there)."""
    from . import native
    return native.require('level XML instances (bfbc2core.xmlinst)', 'xmlinst_instances')


def _refused(what: str) -> Exception:
    from . import native
    return native.refused(f'{what}: an attribute or tag name with a non-ASCII character (the XML reader takes '
                          'ASCII names only)')


def instances(text: str, type_suffix: str | None = None) -> Iterator[tuple[str, str, str]]:
    """(type, guid, body) of each <instance> in document order; with type_suffix, only those whose type ends with
    it (e.g. 'Entity.ReferenceObjectData', '.SoldierSpawnEntityData')."""
    got = _core().xmlinst_instances(text, type_suffix)
    if got is None:
        raise _refused('<instance> elements')
    yield from got


def field_ref(body: str, name: str) -> str | None:
    """The ref attribute of the first field named <name> that has one, whatever the attribute order (DICE: ref then
    name; Napisal: name then ref); None when there is none."""
    got = _core().xmlinst_field_ref(body, name)
    if got is None:
        raise _refused(f'the field {name}')
    r: str | None = got[0]
    return r
