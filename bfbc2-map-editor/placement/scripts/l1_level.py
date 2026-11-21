"""L1: a level partition as a small graph of instances, fields, positions and events (read only).

Built on l1_read.partition (the suite's bundle reader and dbx parser). Everything here is derived from the data:
- an instance's position is its Transform's translation (floats 12-14: rows right, up, forward, translation;
  bfbc2core.level.Entity.matrix uses the same layout);
- events are LevelData.EventConnections items (Source, Target, SourceEvent.Name, TargetEvent.Name), the same edges
  bc2-levelwrite's LevelWriter::connection_arrays reads.
"""
import struct
from dataclasses import dataclass, field

from l1_read import dbx, partition


def _local(ref: str | None) -> str | None:
    if not ref or ref == 'null' or '/' in ref:
        return None
    return ref.upper()


def fval(e: dbx.Element | None):
    """A leaf's value: floats read from raw bits (DBX denormal trap: never via to_xml), else dbx.value."""
    if e is None:
        return None
    if e.kind == dbx.NUMBERS:
        size, vals = dbx.numbers(e)
        if size == 4:
            out = []
            for v in vals:
                i = struct.unpack('>i', v)[0]
                f = struct.unpack('>f', v)[0]
                out.append(f)
                _ = i
            return out[0] if len(out) == 1 else out
    try:
        return dbx.value(e)
    except Exception:  # noqa: BLE001
        return None


@dataclass
class Inst:
    guid: str
    type: str
    el: dbx.Element

    def f(self, name: str) -> dbx.Element | None:
        return self.el.child(name)

    def text(self, name: str) -> str | None:
        c = self.f(name)
        if c is None or c.children is not None:
            return None
        try:
            return dbx.value(c)
        except Exception:  # noqa: BLE001
            return None

    def num(self, name: str) -> float | None:
        """A 4-byte float field read from its raw bits."""
        c = self.f(name)
        if c is None or c.kind != dbx.NUMBERS:
            return None
        size, vals = dbx.numbers(c)
        if size == 4 and len(vals) == 1:
            return struct.unpack('>f', vals[0])[0]
        return None

    def whole(self, name: str) -> int | None:
        c = self.f(name)
        if c is None or c.kind != dbx.NUMBERS:
            return None
        size, vals = dbx.numbers(c)
        if size == 4 and len(vals) == 1:
            return struct.unpack('>i', vals[0])[0]
        return None

    def boolean(self, name: str) -> bool | None:
        t = self.text(name)
        return None if t is None else t == 'true'

    def refs(self, name: str) -> list[str]:
        """Local references of an array field (upper-case guids)."""
        c = self.f(name)
        if c is None or c.children is None:
            return []
        out = []
        for it in c.children:
            g = _local(it.attr('ref'))
            if g:
                out.append(g)
        return out

    def extrefs(self, name: str) -> list[str]:
        c = self.f(name)
        if c is None:
            return []
        if c.children is None:
            r = c.attr('ref')
            return [r] if r and r != 'null' else []
        out = []
        for it in c.children:
            r = it.attr('ref')
            if r and r != 'null':
                out.append(r)
            for cc in it.children or ():
                r2 = cc.attr('ref')
                if r2 and r2 != 'null':
                    out.append(r2)
        return out

    def ref(self, name: str) -> str | None:
        c = self.f(name)
        return None if c is None else _local(c.attr('ref'))

    @property
    def matrix(self) -> list[float] | None:
        c = self.f('Transform')
        if c is None or c.kind != dbx.NUMBERS:
            return None
        size, vals = dbx.numbers(c)
        if size != 4 or len(vals) < 15:
            return None
        return [struct.unpack('>f', v)[0] for v in vals]

    @property
    def pos(self) -> tuple[float, float, float] | None:
        m = self.matrix
        return None if m is None else (m[12], m[13], m[14])

    @property
    def name(self) -> str:
        return self.text('Name') or ''


@dataclass
class Event:
    src: str
    dst: str
    sev: str
    tev: str


@dataclass
class Level:
    lv: str
    bundle: str
    entry: str
    insts: dict[str, Inst] = field(default_factory=dict)
    order: list[str] = field(default_factory=list)
    events: list[Event] = field(default_factory=list)

    def of(self, t: str) -> list[Inst]:
        return [self.insts[g] for g in self.order if self.insts[g].type == t]

    def out(self, g: str) -> list[Event]:
        return self._out.get(g, [])

    def inn(self, g: str) -> list[Event]:
        return self._in.get(g, [])

    def index(self) -> None:
        self._out: dict[str, list[Event]] = {}
        self._in: dict[str, list[Event]] = {}
        for e in self.events:
            self._out.setdefault(e.src, []).append(e)
            self._in.setdefault(e.dst, []).append(e)


def load(lv: str) -> Level:
    bundle, entry, f = partition(lv)
    L = Level(lv.lower(), bundle, entry)
    for e in f.instances:
        g = (e.attr('guid') or '').upper()
        t = (e.attr('type') or '').split('.')[-1]
        L.insts[g] = Inst(g, t, e)
        L.order.append(g)
    for i in L.of('LevelData'):
        c = i.f('EventConnections')
        for cx in (c.children or []) if c is not None else []:
            s = _local((cx.child('Source') or dbx.Element('', 0)).attr('ref'))
            d = _local((cx.child('Target') or dbx.Element('', 0)).attr('ref'))
            se = cx.child('SourceEvent')
            te = cx.child('TargetEvent')
            sn = se.child('Name') if se is not None else None
            tn = te.child('Name') if te is not None else None
            if s and d:
                L.events.append(Event(s, d, dbx.value(sn) if sn is not None else '',
                                      dbx.value(tn) if tn is not None else ''))
    L.index()
    return L


# ------------------------------------------------------------------------------------------------ geometry

def chain(L: Level, start: str) -> tuple[list[str], list[tuple[float, float, float]], float]:
    """An outline from its first AreaGeometryEntityData point, following Next (spec LEVEL-ENT-2): (guids, points,
    the first point's Height)."""
    gs, pts, seen = [], [], set()
    cur = start
    h = None
    while cur and cur not in seen and cur in L.insts:
        seen.add(cur)
        i = L.insts[cur]
        if i.type != 'AreaGeometryEntityData':
            break
        gs.append(cur)
        if i.pos:
            pts.append(i.pos)
        if h is None:
            h = i.num('Height')
        cur = i.ref('Next')
    return gs, pts, h or 0.0


def polygons(L: Level, i: Inst, fld: str = 'Geometry') -> list[dict]:
    """The outlines of a field: one per distinct chain (a chain reached from two starts counted once)."""
    out, seen = [], set()
    for s in i.refs(fld):
        if s not in L.insts:
            continue
        t = L.insts[s].type
        if t == 'AreaGeometryEntityData':
            gs, pts, h = chain(L, s)
            key = frozenset(gs)
            if key in seen or len(pts) < 3:
                continue
            seen.add(key)
            out.append({'kind': 'polygon', 'guids': gs, 'points': pts, 'height': h})
        elif t == 'SphereGeometryEntityData':
            out.append({'kind': 'sphere', 'guids': [s], 'center': L.insts[s].pos, 'radius': L.insts[s].num('Radius')})
        elif t == 'AabbGeometryEntityData':
            c = L.insts[s].f('Extents')
            ext = None
            if c is not None and c.kind == dbx.NUMBERS:
                size, vals = dbx.numbers(c)
                ext = [struct.unpack('>f', v)[0] for v in vals[:3]]
            out.append({'kind': 'aabb', 'guids': [s], 'center': L.insts[s].pos, 'matrix': L.insts[s].matrix,
                        'extents': ext})
    return out
