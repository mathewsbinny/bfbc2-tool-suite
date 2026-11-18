"""The <instance> elements of a converted level partition (DbxEdit's XML) and their fields' references, whatever
the attribute order.

Attribute order varies by author at both levels: DICE's files write <instance guid="..." type="..."> and
<field ref="..." name="...">, Napisal's Onslaught levels <instance type="..." guid="..."> and mostly <field
name="..." ref="..."> (ONS_MP_002: 17,454 name-first, 118 ref-first). Patterns that expected one order read
ONS_MP_002 as 0 placed objects, 0 soldier spawns and 0 AI orders (2026-10-20; spec KYNAPSE_SPEC.md section 4.5).
Any XML-matching code should go through instances() and field_ref() rather than its own patterns; one-attribute
elements (<complex name=...>, <array name=...>, <item ref=...>) are not affected.

Both run in the suite's Rust core when it is built (round F2b, rust/bc2-formats/src/xmlinst.rs: the three patterns
matched by hand with their exact semantics); where a match would rest on whether a non-ASCII character is a word
character (Python's \\w is Unicode-aware), or without the core (BFBC2_PURE_PYTHON=1), the patterns below run -
instances_reference and field_ref_reference, the reference."""
import re
from collections.abc import Iterator
from types import ModuleType

_INSTANCE = re.compile(r'<instance\b([^>]*)>(.*?)</instance>', re.S)
_ATTR = re.compile(r'(\w+)="([^"]*)"')
_FIELD = re.compile(r'<field\b([^>]*?)/?>')

_NATIVE: list[ModuleType | None] = []


def _native() -> ModuleType | None:
    """The Rust core with the xmlinst functions, or None."""
    if not _NATIVE:
        import os

        from . import dbx
        m = dbx._import_native() if os.environ.get(dbx.PURE_PYTHON_ENV, '') in ('', '0') else None
        _NATIVE.append(m if m is not None and hasattr(m, 'xmlinst_instances') else None)
    return _NATIVE[0]


def instances(text: str, type_suffix: str | None = None) -> Iterator[tuple[str, str, str]]:
    """(type, guid, body) of each <instance> in document order; with type_suffix, only those whose type ends with
    it (e.g. 'Entity.ReferenceObjectData', '.SoldierSpawnEntityData')."""
    m = _native()
    got = m.xmlinst_instances(text, type_suffix) if m is not None and type(text) is str and \
        (type_suffix is None or type(type_suffix) is str) else None
    if got is None:
        yield from instances_reference(text, type_suffix)
        return
    yield from got


def instances_reference(text: str, type_suffix: str | None = None) -> Iterator[tuple[str, str, str]]:
    """instances with the patterns (the reference)."""
    for m in _INSTANCE.finditer(text):
        attrs = dict(_ATTR.findall(m.group(1)))
        typ = attrs.get('type', '')
        if type_suffix is None or typ.endswith(type_suffix):
            yield typ, attrs.get('guid', ''), m.group(2)


def field_ref(body: str, name: str) -> str | None:
    """The ref attribute of the first field named <name> that has one, whatever the attribute order (DICE: ref then
    name; Napisal: name then ref); None when there is none."""
    m = _native()
    got = m.xmlinst_field_ref(body, name) if m is not None and type(body) is str and type(name) is str else None
    if got is None:
        return field_ref_reference(body, name)
    r: str | None = got[0]
    return r


def field_ref_reference(body: str, name: str) -> str | None:
    """field_ref with the patterns (the reference)."""
    for m in _FIELD.finditer(body):
        attrs = dict(_ATTR.findall(m.group(1)))
        if attrs.get('name') == name and 'ref' in attrs:
            return attrs['ref']
    return None
