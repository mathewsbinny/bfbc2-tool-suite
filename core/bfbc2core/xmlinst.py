"""The <instance> elements of a converted level partition (DbxEdit's XML), whatever their attribute order.

An instance's attributes come in either order depending on who wrote the level: DICE's files write
<instance guid="..." type="...">, Napisal's Onslaught levels <instance type="..." guid="...">. Patterns that expected
one order read Napisal's ONS_MP_002 as 0 placed objects, 0 soldier spawns and 0 AI orders (2026-10-20; spec
KYNAPSE_SPEC.md section 4.5). Every reader of instances goes through here, in one pass over the text."""
import re

_INSTANCE = re.compile(r'<instance\b([^>]*)>(.*?)</instance>', re.S)
_ATTR = re.compile(r'(\w+)="([^"]*)"')


def instances(text, type_suffix=None):
    """(type, guid, body) of each <instance> in document order; with type_suffix, only those whose type ends with
    it (e.g. 'Entity.ReferenceObjectData', '.SoldierSpawnEntityData')."""
    for m in _INSTANCE.finditer(text):
        attrs = dict(_ATTR.findall(m.group(1)))
        typ = attrs.get('type', '')
        if type_suffix is None or typ.endswith(type_suffix):
            yield typ, attrs.get('guid', ''), m.group(2)
