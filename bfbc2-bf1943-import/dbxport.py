r"""dbxport.py - Battlefield 1943's DBX partitions in BFBC2's schema.

The binary format is the same in both games (strings, varints, big-endian numbers: the suite's `bfbc2core.dbx`
reads both); what differs is the schema: a few fields per type (BFBC2's SkinnedMeshAsset has MemoryReportGroup,
HighPriorityOccluderMesh, RadiosityMesh, RadiosityAlbedoTexture that BF1943's lacks, for instance). A partition is
ported by aligning each instance with a BFBC2 instance of the same type (its template): fields both have keep
BF1943's value, in BFBC2's order; fields only BFBC2 has take the template's value; fields only BF1943 has are left out.

    cat = dbxcatalogue.load(ix)                                        # BFBC2's schema, guids, class names
    t = dbxport.Templates.from_index(ix, [<BFBC2 partitions>], catalogue=cat)
    data, report = dbxport.port(bf1943_dbx_bytes, t, resolve, ours=..., stand_ins=..., donors=[...])
    report.records                                                     # [Record(path, action, old, new, source, step)]

The template of an instance (Templates.choose; each instance's choice is a 'template' record):
  1. counterpart  the BFBC2 instance with the same guid (DICE's own version of the instance)
  2. donor        an instance of the type in a partition the caller pairs with this one (donors=[...])
  3. role / pool  an instance of the type in the pool, the one whose role matches best: its input actions
                  (InputAction, FireInputAction, RotationInput, ...), its slot (the array of local references naming
                  it, and where: ZoomLevels[1] is zoom level 2) and its position among the partition's instances of
                  the type (the seat); 'pool' when nothing tells candidates apart (the first, in pool order)
  4. default      the catalogue's DICE instance of the type (the class default) when the pool has none
Array items are aligned with the template's item at the same position (its first when it has fewer); an empty
template array takes its item from another instance of the type (or the catalogue's prototype).

References (one resolver over (partition, guid)), in this order:
  1. local: an instance of this partition that is kept;
  2. another partition's instance that will exist (ours: the partitions the build writes, with their guids; the
     catalogue: BFBC2's; resolve(partition): the caller's partition-level answer);
  3. relocated: the guid is in another BFBC2 partition (DICE moved objects/soldiers/common/beach/defaulthaloaimer to
     objects/soldiers/common/defaulthaloaimer, keeping the guids): the reference is rewritten;
  4. stand-in: the caller's STAND_INS entry (field, target) -> a reference, a partition, None or TEMPLATE (the
     template's value), or the counterpart's own reference (the template is DICE's version of the instance);
  5. otherwise null, reported. A template's value is never substituted for another role's (the Sherman's cannon
     no longer fires with the Humvee's horn); ROLE_GUARDS refuse what a template adds into the wrong role.
Arrays of references lose the items that will not resolve; with prune, arrays of structs lose the items one of
whose references will not resolve (any depth). primaryInstance is re-pointed when its instance is left out.

Schema: every field's shape is checked against the template (and the catalogue): a container where BFBC2 has a value,
another number count or kind, an enum name the executable lacks, is an error (SchemaError) unless a converter
handles it: ENUM_NUMBERS (BF1943's numbers for BFBC2's enum names, with or without a template), a bool into a number
(the target's own number type: no float bits into an integer), ONE_TO_ARRAY, RENAMED, NESTED (an explicit map),
an empty number list where BFBC2 has a curve (the template's).

Value passes after the alignment: BF1943's EntryInputAction spellings (EIAFire, EIUndefined) as BFBC2's (EiaFire,
EiaUndefined) in every field; the seat-hint texts; bone indices on BFBC2's skeleton (bones=BoneTables: BoneId by its
BoneName, SoldierIKData / SkeletonAsset indices by bone name); DICE's conventions for the BF1943 weapons it brought
into BFBC2 (DICE_CONVENTIONS, derived from the 109 weapon partitions both games ship - dbxparity.py --predict).
Empty arrays are written as TEXT elements, as DICE writes all of them.

The report is a Report: a list of the old text lines (callers that read lines keep working), built from records
(report.records) callers read instead of parsing text.
"""
import copy
import dataclasses
import glob
import os
import re
import struct
import sys
from collections.abc import Callable, Iterable
from typing import Any, NamedTuple

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx  # noqa: E402


class PortError(ValueError):
    """A port that cannot be done as asked (strict mode: anything it would otherwise only report)."""


class MissingTemplate(PortError):
    """A template partition the caller named does not exist."""


class SchemaError(PortError):
    """Fields whose shape BFBC2 does not have and no converter handles."""

    def __init__(self, problems: list[str]):
        super().__init__(f'{len(problems)} schema mismatch(es): ' + '; '.join(problems[:8]) +
                         (' ...' if len(problems) > 8 else ''))
        self.problems = problems


# ------------------------------------------------------------------------------------------------ reports

@dataclasses.dataclass
class Record:
    """One thing the port did. action: template, added, left out, left out instance, duplicate, type, untemplated,
    converted, renamed, nested, value, UNMAPPED, convention, bones, relocated, substituted, nulled, unresolved,
    guarded, pruned, emptied, primary, unverified, schema, enum, derived."""
    path: str
    action: str
    old: str | None = None
    new: str | None = None
    source: str | None = None           # the partition the new value came from (template, stand-in, relocation)
    step: str | None = None             # the template step: counterpart / donor / role / pool / default / none
    note: str | None = None
    partition: str | None = None        # the partition ported (set by callers that gather several reports)
    text: str | None = None             # a caller's own line (effectport's summaries): line() gives it as it is

    def line(self) -> str:
        if self.text is not None:
            return self.text
        a, p = self.action, self.path
        frm = f' from {self.source}' if self.source else ''
        if a == 'added':
            v = f': {self.new}' if self.new not in (None, '') else ''
            return f'added {p} (BFBC2\'s value{v}{frm})'
        if a == 'left out':
            return f'left out {p} (BF1943 only)'
        if a == 'duplicate':
            return f'left out {p} (a second field of that name: BF1943\'s duplicate)'
        if a == 'left out instance':
            return f'left out instance {p}: {self.note}'
        if a == 'type':
            return f'type {p} written as {self.new}'
        if a == 'untemplated':
            return f'type {p}: no BFBC2 template - kept as BF1943 has it'
        if a == 'template':
            return f'template {p}: {self.step}{frm}'
        if a in ('value', 'convention', 'bones'):
            why = f' ({self.note})' if self.note else ''
            return f'{"value" if a == "value" else a} {p}: {self.old} -> {self.new}{why}'
        if a == 'UNMAPPED':
            return f'UNMAPPED {p}: BF1943 number {self.old} has no known BFBC2 name - the template\'s value'
        if a == 'relocated':
            return f'relocated {p}: {self.old} -> {self.new} (the guid is in that BFBC2 partition)'
        if a == 'substituted':
            return f'substituted {p} (refers to {self.old}): {self.new} ({self.note}{frm})'
        if a in ('nulled', 'unresolved'):
            return f'nulled {p} (refers to {self.old}): {self.note or "no BFBC2 value"}'
        if a == 'guarded':
            return f'guarded {p}: {self.new}{frm} refused ({self.note}) - null'
        if a == 'pruned':
            return f'pruned {p}: {self.note}'
        if a == 'emptied':
            return f'emptied {p}: {self.note}'
        if a in ('converted', 'renamed', 'nested'):
            return f'{a} {p}{(": " + self.note) if self.note else ""}'
        return f'{a} {p}' + (f': {self.old} -> {self.new}' if self.old is not None or self.new is not None else '') + \
            (f' ({self.note})' if self.note else '')


class Report(list):
    """The report lines (a list of str, as before), with the records they were made from (records)."""

    def __init__(self, lines: Iterable[str] = ()) -> None:
        super().__init__(lines)
        self.records: list[Record] = []

    def add(self, path: str, action: str, **kw: Any) -> Record:
        r = Record(path, action, **kw)
        self.records.append(r)
        self.append(r.line())
        return r

    def of(self, *actions: str) -> list[Record]:
        return [r for r in self.records if r.action in actions]

    def extend_from(self, other: 'Report', partition: str | None = None,
                    lines: Callable[[Record], str | None] | None = None) -> None:
        """Another report's records (their partition set), and the lines lines(record) gives (default: theirs)."""
        for r in other.records:
            if partition is not None and r.partition is None:
                r.partition = partition
            self.records.append(r)
            line = r.line() if lines is None else lines(r)
            if line is not None:
                self.append(line)


# ------------------------------------------------------------------------------------------------ templates

def _local(r: str | None) -> bool:
    return bool(r) and r != 'null' and '/' not in (r or '')


INPUT_FIELDS = re.compile(r'(InputAction|RotationInput|ActionToSuppress)$')


class Candidate(NamedTuple):
    partition: str
    element: dbx.Element
    ordinal: int                         # its position among its partition's instances of its type
    slot: tuple[str, int] | None         # (array field, index) of the local reference array naming it
    inputs: tuple[tuple[str, str], ...]  # its input-action fields' values (normalized)


def _roles(instances: list[dbx.Element]) -> dict[str, tuple[int, tuple[str, int] | None, tuple]]:
    """{guid: (ordinal, slot, inputs)} of a partition's instances."""
    slots: dict[str, tuple[str, int]] = {}

    def walk(e: dbx.Element) -> None:
        for c in e.children or ():
            if c.tag == 'array' and c.children:
                for i, it in enumerate(c.children):
                    if not it.children and _local(it.attr('ref')):
                        slots.setdefault((it.attr('ref') or '').lower(), (c.attr('name') or '', i))
            if c.kind == dbx.CONTAINER:
                walk(c)
    for inst in instances:
        walk(inst)
    seen: dict[str, int] = {}
    out = {}
    for inst in instances:
        t = inst.attr('type') or ''
        n = seen.get(t, 0)
        seen[t] = n + 1
        g = (inst.attr('guid') or '').lower()
        out[g] = (n, slots.get(g), _inputs(inst))
    return out


def _inputs(inst: dbx.Element) -> tuple[tuple[str, str], ...]:
    out = []
    t = (inst.attr('type') or '').rsplit('.', 1)[-1]
    for c in inst.children or ():
        n = c.attr('name') or ''
        if not INPUT_FIELDS.search(n) or c.kind == dbx.CONTAINER:
            continue
        if c.kind == dbx.NUMBERS:
            vals = dbx.numbers(c)[1]
            if len(vals) != 1:
                continue
            num = int.from_bytes(vals[0], 'big', signed=True)
            v = ENUM_NUMBERS.get((t, n), {}).get(num, str(num))
        else:
            v = eia_value(c.text.strip())
        out.append((n, v))
    return tuple(out)


class Templates:
    """BFBC2 instances by type: every instance of each type in the given partitions, in the order given (folders:
    their .dbx files sorted). Folders of unpacked bundles, .dbx files, or (name, bytes) pairs - a pair whose bytes
    are None (a partition the game copy does not have) is an error (MissingTemplate)."""

    def __init__(self, sources: Iterable[Any], catalogue: Any = None) -> None:
        self.by_type: dict[str, dbx.Element] = {}        # the first of each type (as before)
        self.where: dict[str, str] = {}
        self.candidates: dict[str, list[Candidate]] = {}
        self.by_guid: dict[str, Candidate] = {}
        self.partitions: list[str] = []
        self.catalogue = catalogue
        for s in sources:
            items: list[tuple[str, bytes | None]] = []
            if isinstance(s, tuple):
                if s[1] is None:
                    raise MissingTemplate(f'template partition {s[0]}: not in the game copy')
                items = [s]
            elif os.path.isdir(s):
                items = [(p, None) for p in sorted(glob.glob(os.path.join(s, '**', '*.dbx'), recursive=True))]
            elif os.path.isfile(s):
                items = [(s, None)]
            else:
                raise MissingTemplate(f'template source {s}: no such file or folder')
            for name, data in items:
                try:
                    f = dbx.parse(data if data is not None else open(name, 'rb').read())
                except (ValueError, IndexError):
                    continue
                self.add(name, f)

    def add(self, name: str, f: dbx.DbxFile) -> None:
        part = name[:-4] if name.lower().endswith('.dbx') else name
        part = part.replace('\\', '/').lower()
        self.partitions.append(part)
        roles = _roles(f.instances)
        for inst in f.instances:
            t = inst.attr('type')
            if not t:
                continue
            g = (inst.attr('guid') or '').lower()
            o, slot, inputs = roles[g]
            c = Candidate(part, inst, o, slot, inputs)
            self.candidates.setdefault(t, []).append(c)
            self.by_guid.setdefault(g, c)
            if t not in self.by_type:
                self.by_type[t] = inst
                self.where[t] = name

    @classmethod
    def from_index(cls, ix: Any, names: Iterable[str], catalogue: Any = None) -> 'Templates':
        """Templates from BFBC2 partitions by name (meshview.Index); a name the game copy lacks is an error."""
        return cls([(n, ix.read(n + '.dbx')) for n in names], catalogue)

    def __contains__(self, t: object) -> bool:
        return t in self.candidates

    def choose(self, inst: dbx.Element, role: tuple, donors: Iterable[str] = (),
               catalogue: Any = None) -> tuple[str, Candidate | None]:
        """(step, candidate) of an instance: counterpart, donor, role / pool, default, none."""
        t = inst.attr('type') or ''
        g = (inst.attr('guid') or '').lower()
        c = self.by_guid.get(g)
        if c is not None and c.element.attr('type') == t:
            return 'counterpart', c
        cands = self.candidates.get(t, [])
        donors = [d.lower() for d in donors]
        if donors:
            paired = [c for d in donors for c in cands if c.partition == d]
            if paired:
                return 'donor', _best(paired, role)[1]
        if cands:
            score, best = _best(cands, role)
            return ('role' if score > 0 and len(cands) > 1 else 'pool'), best
        cat = catalogue if catalogue is not None else self.catalogue
        if cat is not None and t in getattr(cat, 'defaults', {}):
            p, e = cat.defaults[t]
            return 'default', Candidate(p, e, 0, None, ())
        return 'none', None


def _best(cands: list[Candidate], role: tuple) -> tuple[int, Candidate]:
    ordinal, slot, inputs = role
    best, top = cands[0], -1
    for c in cands:
        s = 4 * len(set(c.inputs) & set(inputs)) + 2 * (slot is not None and c.slot == slot) + (c.ordinal == ordinal)
        if s > top:
            best, top = c, s
    return top, best


# ------------------------------------------------------------------------------------------------ the tables

# BFBC2 array field: the BF1943 single-reference field it replaced (a weapon state's meshes)
ONE_TO_ARRAY = {'Meshes1p': 'Mesh1p', 'Meshes3p': 'Mesh3p'}
# BFBC2 field: the BF1943 field it was renamed from, aligned as the same field (a physics asset's part table: BF1943's
# {Name, MaterialName} items, BFBC2's with HashName, MaterialIndex, Aabb, Translation - all 0 in DICE's files)
RENAMED = {'PartInfoDatas': 'PartMaterialDatas',
           # found by dbxparity.py on the instances both games have (same GUID and type), 2026-11-08: DICE split
           # BF1943's single damping / collective input scale into two fields and fixed a typo; aligned without these,
           # the fields took the template's values (the Willys' steering wheel damped 0.5 instead of BF1943's 0.1)
           'AngularMomentumDampingAcc': 'AngularMomentumDamping', 'AngularMomentumDampingDeacc': 'AngularMomentumDamping',
           'CollectiveThrottleInputScale': 'CollectiveInputScale', 'CollectiveBrakeInputScale': 'CollectiveInputScale',
           'UpsideDownAngle': 'UpsideDownAngel'}
# BF1943 fields BFBC2 keeps one level deeper: (the struct, BFBC2's new struct in it) -> the fields moved (dbxparity
# 'nested', 2026-11-08: FiringFunctionData.FireLogic.MaxRecoilAngleX -> FireLogic.Recoil.MaxRecoilAngleX). An explicit
# map: matching names alone moved any BF1943 field into any BFBC2 struct with a child of that name (Name, Enabled)
NESTED = {('FireLogic', 'Recoil'): ('MaxRecoilAngleX', 'MinRecoilAngleX', 'MaxRecoilAngleY', 'MinRecoilAngleY',
                                    'MaxRecoilAngleZ', 'MinRecoilAngleZ', 'MaxRecoilFov', 'MinRecoilFov',
                                    'RecoilFollowsDispersion')}
# Enum fields BF1943 stores as numbers (NUMBERS) where BFBC2 stores the name (TEXT): (type, field) -> {number: name}.
# The pairs are DICE's own, from the instances both games have (dbxparity.py: the Kurogane, Chi-Ha, Bofors, .30 cal,
# KORD, Zero, Corsair); BFBC2's EntryInputActionEnum is EiaThrottle 0 .. EiaUndefined 49 (its type table, 0x1180C58),
# BF1943's agrees up to 6 (Roll) and differs after (its 7 is DICE's EiaFire, BFBC2's 7 EiaSwitchPrimaryWeapon), so
# only paired numbers are mapped. A number not in its map is reported (UNMAPPED) and takes the template's value.
# Kept as numbers, BFBC2 read no input: the ported steering wheels and gun mounts did not turn (playtest 2026-11-08).
# 16 drives the planes' landing gear (its legs, wheels and hatches: BF1943's LandingGearEntryComponentData raises it
# on its own input); DICE's prologue planes (never player-flown) put it on EiaPitch, which would fold the gear with
# every pitch input - BFBC2 has no gear input, so it is left undefined: the gear stays down. The maps run with or
# without a template (the Bofors' ChildMovingBodyData.InputAction 0 is DICE's EiaFire).
ENUM_NUMBERS = {
    ('ChildRotationBodyData', 'RotationInput'): {0: 'EiaThrottle', 4: 'EiaYaw', 5: 'EiaPitch', 6: 'EiaRoll',
                                                 7: 'EiaFire', 16: 'EiaUndefined'},
    ('ChildMovingBodyData', 'InputAction'): {0: 'EiaFire', 7: 'EiaFire'},
}
# a placement needs its object: port(required=REQUIRED) leaves out one whose object no partition will supply
REQUIRED = {'Entity.ReferenceObjectData': 'ReferencedObject'}
# BF1943 classes BFBC2 lacks whose instances become a BFBC2 class (aligned on its template; the BF1943-only fields
# reported as left out): the planes' pilot seat, a player entry that also retracts the landing gear (BFBC2 has no
# such class - its executable has no LandingGear string - and no gear input; DICE's prologue planes have a plain
# PlayerEntryComponentData there). Applied before anything is left out.
TYPE_AS = {'GameSharedResources.LandingGearEntryComponentData': 'GameSharedResources.PlayerEntryComponentData'}
# BF1943 types BFBC2's executable has no class for (2026-10-31): dropped by port(drop_types=NOT_IN_BFBC2); with a
# catalogue, drop_untemplated finds them (and any other) by the executable's type names
NOT_IN_BFBC2 = {'GameSharedResources.AirRaidEffectsData', 'GameSharedResources.TeamVehicleProjectileEntityData',
                'GameSharedResources.CarHornComponentData', 'GameSharedResources.ScorePartComponentData',
                # the planes' HUD tracker (their pilot seat's HudData): no such class in BFBC2 (2026-11-08)
                'GameSharedResources.HudVehicleTrackerData'}

# Field values BF1943 writes otherwise than BFBC2. Input actions: BF1943 spells the EntryInputAction enum EIA... /
# EIUndefined, BFBC2 Eia... / EiaUndefined - the engine resolves enums by name, so BF1943's spellings named nothing
# (the seat hints named no key; the planes' SuppressVehicleInput suppressed nothing): eia_value, in every field.
# BF1943's countermeasure action is BFBC2's EiaFireCountermeasure (its value 11). The seat-hint texts VH_SWITCH (BFBC2
# has it, without a PC text) and VH_BOAT_THROTTLE (BFBC2 has none) become the texts BFBC2's own vehicles show
# (VEHICLE_HINT_CHANGE_POSITION, whose PC text WIN32_... names the key; VEHICLE_HINT_ACCELERATE).
HINT_ACTIONS = {'EIUndefined': 'EiaUndefined', 'EIAFireCountermeasure': 'EiaFireCountermeasure'}
HINT_TEXTS = {'VH_SWITCH': 'VEHICLE_HINT_CHANGE_POSITION', 'VH_BOAT_THROTTLE': 'VEHICLE_HINT_ACCELERATE'}


def eia_value(v: str) -> str:
    """BF1943's EntryInputAction spelling as BFBC2's (EIAFire -> EiaFire); other values unchanged."""
    if v in HINT_ACTIONS:
        return HINT_ACTIONS[v]
    if re.fullmatch(r'EIA[A-Z]\w*', v):
        return 'Eia' + v[3:]
    return v


def hint_value(field: str, value: str) -> str:
    """The BFBC2 value of a BF1943 seat-hint field's value (unchanged when it needs none)."""
    v = value.strip()
    if field in ('HintAction1', 'HintAction2'):
        new = eia_value(v)
        return new if new != v else value
    if field in ('HintLine1', 'HintLine2') and v in HINT_TEXTS:
        return HINT_TEXTS[v]
    return value


# A template's reference never goes into another role: (field path pattern, refused target pattern, why). Checked on
# what a template adds and on TEMPLATE stand-ins (the caller's explicit references are its own choice).
ROLE_GUARDS: list[tuple[Any, ...]] = [   # (field path, refused target, why[, the partitions it applies to])
    (re.compile(r'FiringFunctionData\..*(Sound|SoundEffect)$'), re.compile(r'horn', re.I), 'a horn as a weapon sound'),
    (re.compile(r'DetonationEffect$'), re.compile(r'vehicleexplosions|/fx_vexp', re.I),
     'a vehicle\'s death explosion as a weapon\'s detonation', re.compile(r'firing|projectile|bomb', re.I)),
    (re.compile(r'AnimationEntityData\.Animation$'), re.compile(r'(^|/)(ai_|soldier)', re.I),
     'a soldier animation on an object'),
]

TEMPLATE = object()                  # a STAND_INS value: the template's value for that field


# ------------------------------------------------------------------------------------------------ DICE's conventions

# Classes DICE keeps out of the weapon partitions it brought from BF1943 (their instances left out; what named them is
# resolved like any reference): BF1943's embedded PersistentWeapon (DICE's live in persistence/weapons: 13 of 13)
DICE_MOVED_CLASSES = {'GameSharedResources.PersistentWeapon'}
# DICE's conventions for BF1943's weapons (the 109 weapon partitions both games ship; WW2_WEAPON_ANIM_AUDIT.md and
# the user's ruling 2026-11-09: ported weapons follow DICE's BFBC2 conventions). Applied by _conventions:
#  UseAutoAiming       true -> false (7 of 7 firearms, the planes' guns too)
#  Bullet Gravity      0 -> -9.81 on firearms (their shot faster than 100 m/s; 5 of 5; the knives keep 0)
#  Shot.InitialSpeed   300 - 599 m/s -> 600 on firearm bullets (9 of 9: pistols, rifles, SMGs, the M95, the KORD)
#  Hand grenades       throw speed 24 -> 20, GrenadeEntityData.Gravity -9 -> -8 (2 of 2)
#  NumberOfMagazines   -1 on a firearm -> the template's (finite: 4, 6, 9999 by class); AmmoPickupMin / Max 0 / 0 ->
#                      the template's (DICE gave every firearm a pickup)
#  ZoomType            hold -> press, unless the template's is hold (melee and thrown kept hold)
#  SprintLookSpeedMultiplier 0.5 -> 1 (23 of 23 zoom levels)
#  AlwaysAimHead       false -> the template's (DICE's varies by class)
#  AnimBaseSet         wabsShg -> wabsRif (the knife); with a counterpart or paired donor, the donor's (the wrench on
#                      the power tool's wabsHg, a pistol's wabsHg)
#  FireLogic.Recoil    the template's (DICE zeroed or retuned every nonzero BF1943 recoil: 27 firings)
FIREARM_SPEED = 100.0
# the weapon conventions apply to the soldiers' weapons (handheld, stationary): DICE kept the planes' and tanks'
# guns as BF1943 had them (the Zero's bullets: 480 m/s, no drop - dbxparity --predict)
WEAPON_PARTITION = re.compile(r'^objects/weapons/', re.I)


# ------------------------------------------------------------------------------------------------ bones

class BoneTables:
    """The source (BF1943) and target (BFBC2) bone names of a skeleton, for the bone-index fields: BoneFakePhysicsData
    BoneId (by its BoneName), SoldierIKData BoneIndices / WeaponBoneIndex and SkeletonAsset MinBoneIndex (by bone name
    through the two tables; MinBoneIndex by MinBoneName), MaxMainBoneIndex (DICE: the target's last bone when the
    source's main range does not map onto a range)."""

    def __init__(self, source: list[str], target: list[str], skeleton: str = 'characters/skeletons/ske01') -> None:
        self.source, self.target, self.skeleton = list(source), list(target), skeleton.lower()
        self.at = {n: i for i, n in enumerate(self.target)}

    def index(self, i: int) -> int | None:
        if 0 <= i < len(self.source):
            return self.at.get(self.source[i])
        return None

    def map(self, i: int) -> int:
        """A source bone index on the target skeleton (unchanged when it names no bone there)."""
        j = self.index(i)
        return i if j is None else j

    @classmethod
    def from_resources(cls, source_res: bytes, target_res: bytes,
                       skeleton: str = 'characters/skeletons/ske01') -> 'BoneTables':
        suite_paths.use('anim-tools')
        import granny

        def names(b: bytes) -> list[str]:
            return [x['Name'] for x in granny.Granny(b, 'skeleton').root()['Skeletons'][0]['Bones']]
        return cls(names(source_res), names(target_res), skeleton)

    @classmethod
    def ske01(cls, src: Any, ix: Any) -> 'BoneTables':
        """BF1943's and BFBC2's soldier skeleton (bf1943import.Source, meshview.Index)."""
        return cls.from_resources(src.data('characters/skeletons/ske01.res'), ix.read('characters/skeletons/ske01.res'))


# ------------------------------------------------------------------------------------------------ the port

def guids_of(data: bytes) -> set[str]:
    """The instance guids of a partition (lower case): for port(ours={partition: guids})."""
    return {(i.attr('guid') or '').lower() for i in dbx.parse(data).instances}


def _field(e: dbx.Element | None, name: str) -> dbx.Element | None:
    return next((c for c in (e.children if e is not None else None) or () if c.attr('name') == name), None)


def _num(e: dbx.Element) -> list[float]:
    size, vals = dbx.numbers(e)
    if size == 4:
        return [struct.unpack('>f', v)[0] for v in vals]
    if size == 8:
        return [struct.unpack('>d', v)[0] for v in vals]
    return []


def _ints(e: dbx.Element) -> list[int]:
    return [int.from_bytes(v, 'big', signed=True) for v in dbx.numbers(e)[1]]


def _set_numbers(e: dbx.Element, vals: list[bytes], size: int) -> None:
    raw = bytearray()
    dbx._put_varint(raw, len(vals))
    dbx._put_varint(raw, size)
    for v in vals:
        raw += v
    e.raw = bytes(raw)


def _with_ref(e: dbx.Element, ref: str) -> dbx.Element:
    out = copy.copy(e)
    out.attributes = [(k, ref if k == 'ref' else v) for k, v in e.attributes]
    return out


def _named(children: list[dbx.Element] | None) -> tuple[dict[str, dbx.Element], list[str]]:
    out: dict[str, dbx.Element] = {}
    dups = []
    for c in children or ():
        n = c.attr('name')
        if n is None:
            continue
        if n in out:
            dups.append(n)
        else:
            out[n] = c
    return out, dups


def _empty_array(e: dbx.Element) -> dbx.Element:
    """An empty array as DICE writes it: a TEXT element."""
    return dbx.Element(e.tag, dbx.TEXT, list(e.attributes), '', b'', None)


def _shape(e: dbx.Element) -> str:
    if e.kind == dbx.CONTAINER:
        return f'{e.tag}-container'
    if e.attr('ref') is not None:
        return 'ref'
    if e.kind == dbx.TEXT:
        return 'array-text' if e.tag == 'array' else 'text'
    if e.kind == dbx.NUMBERS:
        size, vals = dbx.numbers(e)
        return f'num{size}x{len(vals)}'
    return f'k{e.kind}'                      # a bool (6) / byte value, as the catalogue names it


class _Port:
    def __init__(self, f: dbx.DbxFile, templates: Templates, resolve: Callable[[str], bool] | None, opts: dict) -> None:
        self.f = f
        self.T = templates
        self.resolve = resolve
        self.o = opts
        self.cat = opts.get('catalogue') or templates.catalogue
        self.ours: dict[str, set[str]] | None = opts.get('ours')
        self.stand_ins: dict = opts.get('stand_ins') or {}
        self.strict = opts.get('strict', False)
        self.report = Report()
        self.dropped: set[str] = set()
        self.local = {(i.attr('guid') or '').lower() for i in f.instances}
        self.by_guid = {(i.attr('guid') or '').lower(): i for i in f.instances}
        self.inlined: set[str] = set()
        self.errors: list[str] = []
        self.chosen: dict[str, tuple[str, Candidate | None]] = {}
        self.name = (opts.get('name') or '').lower()
        self.checking = resolve is not None or self.ours is not None

    # ---- reporting
    def add(self, path: str, action: str, **kw: Any) -> None:
        self.report.add(path, action, **kw)

    def fail(self, msg: str) -> None:
        if self.strict:
            raise PortError(msg)

    # ---- references
    def exists(self, p: str, g: str) -> bool | None:
        """True / False when known; None: the partition is known to exist only as a whole (resolve)."""
        pl, gl = p.lower(), g.lower()
        if self.ours is not None and pl in self.ours:
            return gl in self.ours[pl]
        if self.cat is not None and self.cat.exists(pl, gl):
            return True
        if self.resolve is not None and self.resolve(p):
            return None
        return False

    def relocation(self, p: str, g: str) -> str | None:
        """The one BFBC2 partition holding the guid (DICE moved the instance there); None when there is none or several
        (a material's guid is in all 74 level grids: which one is the caller's choice - a stand-in, 'partition/*')."""
        if self.cat is None:
            return None
        if self.ours is not None and p.lower() in self.ours:
            return None
        w = [x for x in self.cat.where(g) if x != p.lower()]
        return w[0] if len(w) == 1 else None

    def stand_in_ref(self, si: str, target_ref: str) -> str:
        """A stand-in's reference: 'partition/guid' as it is, 'partition/*' keeping the reference's own guid (a copy
        of the instance under another partition: the level's own material grid), a partition its primary instance."""
        if si.endswith('/*'):
            return f'{si[:-2].lower()}/{target_ref.rsplit("/", 1)[-1]}'
        if re.search(r'/[0-9a-fA-F-]{36}$', si):
            return si
        return self.primary_ref(si)

    def ref_ok(self, r: str) -> bool:
        """Whether a reference will resolve as it is (no rewriting)."""
        if not r or r == 'null':
            return True
        if '/' not in r:
            g = r.lower()
            return g in self.local and g not in self.dropped
        if not self.checking:
            return True
        p, g = r.rsplit('/', 1)
        e = self.exists(p, g)
        return e is not False

    def stand_in(self, field: str, target: str, wildcard: bool = True) -> Any:
        """The caller's stand-in for a reference to target in field: (field, target), then target, then (with
        wildcard) (field, '*'); KeyError when there is none."""
        for k in ((field, target), target) + (((field, '*'),) if wildcard else ()):
            if k in self.stand_ins:
                return self.stand_ins[k]
        return KeyError

    def use_stand_in(self, si: Any, path: str, target: str, t: dbx.Element | None, step: str, ref: str = '') -> str:
        if si is None:
            self.add(path, 'substituted', old=target, new='null', note='stand-in: null')
            return 'null'
        if si is TEMPLATE:
            tr = t.attr('ref') if t is not None else None
            if tr is None:
                self.add(path, 'nulled', old=target, note='stand-in TEMPLATE: the template has no value')
                return 'null'
            return self.template_ref(tr, path, t, step, why='stand-in: the template\'s', target=target)
        new = self.stand_in_ref(si, ref or target)
        self.add(path, 'substituted', old=target, new=new, source=new.rsplit('/', 1)[0], note='stand-in')
        return new

    def resolve_ref(self, r: str, path: str, t: dbx.Element | None, step: str,
                    from_template: bool = False) -> tuple[str, str | None]:
        """(the reference to write, None or how it was rewritten) of a reference r at path."""
        if not r or r == 'null':
            return r, None
        field = path.rsplit('.', 1)[-1].split('[', 1)[0]
        if '/' not in r:
            g = r.lower()
            if g in self.local and g not in self.dropped and not from_template:
                return r, None
            target = '#' + g
            p = None
        else:
            p, g = r.rsplit('/', 1)
            if not self.checking:
                return r, None
            ex = self.exists(p, g)
            if ex is True:
                return r, None
            if ex is None and not (self.cat is not None and self.cat.has_partition(p)):
                return r, None                   # a partition the caller says will exist (no guids to check)
            # (a BFBC2 partition the build does not replace, without the guid: dangling - DICE removed the instance)
            target = p.lower()
        if p is not None:
            moved = self.relocation(p, g)
            if moved is not None:
                new = f'{moved}/{g}'
                self.add(path, 'relocated', old=r, new=new, source=moved)
                return new, 'relocated'
        if from_template:
            return 'null', 'null'
        si = self.stand_in(field, target, wildcard=False)          # the caller's stand-in for this very target
        if si is not KeyError:
            return self.use_stand_in(si, path, target, t, step, r), 'stand-in'
        if step == 'counterpart' and t is not None and t.attr('ref') is not None:
            new = self.template_ref(t.attr('ref') or 'null', path, t, step, why='the counterpart\'s', guard=False,
                                    target=target)
            return new, 'counterpart'
        si = self.stand_in(field, target)                           # the caller's stand-in for the field
        if si is not KeyError:
            return self.use_stand_in(si, path, target, t, step, r), 'stand-in'
        self.add(path, 'nulled', old=target, note='no BFBC2 value: nothing stands in for it')
        self.fail(f'{path}: refers to {target}, which will not exist (no stand-in)')
        return 'null', 'null'

    def primary_ref(self, partition: str) -> str:
        p = partition.lower()
        g = self.cat.primary.get(p) if self.cat is not None else None
        if g is None:
            raise PortError(f'stand-in {partition}: no such BFBC2 partition (give partition/guid)')
        return f'{p}/{g}'

    def template_ref(self, tr: str, path: str, t: dbx.Element | None, step: str, why: str, guard: bool = True,
                     target: str | None = None) -> str:
        """A template's reference written here (for BF1943's reference to target): its own local instances only
        when this partition keeps the same guid (a counterpart), another partition's when it will exist, the role
        guard."""
        if tr == 'null':
            self.add(path, 'substituted', old=target, new='null', note=why)
            return 'null'
        if '/' not in tr:
            g = tr.lower()
            if step == 'counterpart' and g in self.local and g not in self.dropped:
                self.add(path, 'substituted', old=target, new=tr, note=why)
                return tr
            self.add(path, 'nulled', old='#' + g, note=f'{why} names the template\'s own instance')
            return 'null'
        if guard and not self.guard_ok(path, tr):
            return 'null'
        new, how = self.resolve_ref(tr, path, None, step, from_template=True)
        if how == 'null':
            self.add(path, 'nulled', old=tr.rsplit('/', 1)[0], note=f'{why} will not exist either')
            return 'null'
        self.add(path, 'substituted', old=target, new=new, source=new.rsplit('/', 1)[0], note=why)
        return new

    def guard_ok(self, path: str, ref: str) -> bool:
        target = ref.rsplit('/', 1)[0]
        for pat, bad, why, *where in ROLE_GUARDS:
            if where and not where[0].search(self.name or ''):
                continue
            if pat.search(path) and bad.search(target):
                self.add(path, 'guarded', new=ref, source=target, note=why)
                self.fail(f'{path}: {target} refused ({why})')
                return False
        return True

    # ---- alignment
    def item_proto(self, itype: str, tpath: str, t: dbx.Element | None) -> dbx.Element | None:
        """An item for an array the template has empty: another instance's of the type, or the catalogue's."""
        for c in self.T.candidates.get(itype, []):
            a = _at_path(c.element, tpath)
            if a is not None and a.tag == 'array' and a.kind == dbx.CONTAINER and a.children:
                return a.children[0]
        if self.cat is not None:
            p = self.cat.protos.get((itype, tpath + '[]'))
            if p is not None:
                return p
        return None

    def align(self, e: dbx.Element, t: dbx.Element | None, path: str, tpath: str, itype: str,
              step: str) -> dbx.Element | None:
        """e (BF1943) aligned with t (BFBC2's, None: no template): a new element (None: the item is left out)."""
        ref = e.attr('ref')
        if ref is not None and not e.children:
            if t is not None and t.attr('ref') is None and t.kind != dbx.CONTAINER and t.tag != 'array':
                self.schema(path, e, t)
                return e
            new, how = self.resolve_ref(ref, path, t, step)
            return e if new == ref else _with_ref(e, new)
        if e.tag == 'array' and e.kind in (dbx.CONTAINER, dbx.TEXT):      # (an array of numbers is a value)
            return self.align_array(e, t, path, tpath, itype, step)
        if e.kind == dbx.CONTAINER:
            if t is not None and (t.kind != dbx.CONTAINER or t.tag != e.tag):
                self.schema(path, e, t)
                return e
            return self.align_struct(e, t, path, tpath, itype, step)
        # a leaf value
        if t is None:
            return self.convert_untemplated(e, path, tpath, itype)
        if t.kind == dbx.CONTAINER or t.attr('ref') is not None:
            self.schema(path, e, t)
            return e
        if e.kind == t.kind and (e.kind != dbx.NUMBERS or _shape(e) == _shape(t)):
            return e
        conv = self.convert(e, t, path, tpath, itype)
        if conv is None:
            if e.kind == dbx.NUMBERS and t.kind == dbx.NUMBERS and self.variable_count(itype, tpath, e, t):
                return e
            self.schema(path, e, t)
            return e
        conv.attributes = [(k, e.attr('name') or v if k == 'name' else v) for k, v in conv.attributes]
        return conv

    def variable_count(self, itype: str, tpath: str, e: dbx.Element, t: dbx.Element) -> bool:
        """Whether DICE writes this number field with varying counts (a curve, a list): BF1943's count is fine."""
        se, st = dbx.numbers(e)[0], dbx.numbers(t)[0]
        if se != st and dbx.numbers(e)[1]:
            return False
        if e.tag == 'array' and t.tag == 'array':            # an array of numbers: its length is its own ...
            return not self.fixed_count(itype, tpath, st)    # ... unless DICE always writes the same count (a curve)
        if self.cat is None or not dbx.numbers(e)[1]:
            return False
        seen = self.cat.shapes.get((itype, tpath)) or {}
        counts = {s for s in seen if s.startswith(f'num{st}x')}
        return len(counts) > 1

    def fixed_count(self, itype: str, tpath: str, size: int) -> bool:
        """Whether DICE always writes this number field with one count (the catalogue's evidence)."""
        if self.cat is None:
            return False
        seen = self.cat.shapes.get((itype, tpath)) or {}
        return len({s for s in seen if s.startswith(f'num{size}x')}) == 1

    def schema(self, path: str, e: dbx.Element, t: dbx.Element) -> None:
        self.errors.append(f'{path}: BF1943 {_shape(e)}, BFBC2 {_shape(t)}')

    def convert(self, e: dbx.Element, t: dbx.Element, path: str, tpath: str, itype: str) -> dbx.Element | None:
        """A leaf whose kind / count differs from the template's, by a registered converter; None: none applies."""
        n = e.attr('name') or ''
        owner = path.rsplit('.', 1)[0].rsplit('.', 1)[-1].split('[', 1)[0]      # 'Ns.Type.Field' -> 'Type'
        if e.kind == dbx.NUMBERS and t.kind == dbx.TEXT:
            return self.enum_number(e, t, path, owner, n)
        if t.kind == dbx.NUMBERS and e.kind not in (dbx.TEXT, dbx.NUMBERS, dbx.CONTAINER) and len(e.raw) == 2:
            size, vals = dbx.numbers(t)
            if len(vals) != 1:
                return None
            v = e.raw[1]
            if size == 4:
                whole = self.cat.is_whole(itype, tpath) if self.cat is not None else None
                if whole is None:
                    w = int.from_bytes(vals[0], 'big', signed=True)
                    whole = w != 0 and (w >> 24) in (0, -1)
                packed = struct.pack('>i', v) if whole else struct.pack('>f', float(v))
            elif size == 8:
                packed = struct.pack('>d', float(v))
            elif size == 2:
                packed = struct.pack('>H', v)
            else:
                return None
            out = copy.deepcopy(t)
            _set_numbers(out, [packed], size)
            old = 'true' if v == 1 else 'false' if v == 0 else str(v)
            self.add(path, 'value', old=old, new=dbx.value(out), note='a bool / byte as BFBC2\'s number')
            return out
        if e.kind == dbx.NUMBERS and t.kind == dbx.NUMBERS:
            se, ve = dbx.numbers(e)
            st, vt = dbx.numbers(t)
            if not ve and vt and se in (st, 0) and (e.tag != 'array' or self.fixed_count(itype, tpath, st)):
                out = copy.deepcopy(t)
                self.add(path, 'value', old='(empty)', new=dbx.value(out),
                         note='an empty number list where BFBC2 has values: the template\'s')
                return out
        return None

    def enum_number(self, e: dbx.Element, t: dbx.Element | None, path: str, owner: str, n: str) -> dbx.Element | None:
        size, vals = dbx.numbers(e)
        table = ENUM_NUMBERS.get((owner, n))
        if len(vals) != 1 or table is None:
            return None
        num = int.from_bytes(vals[0], 'big', signed=True)
        name = table.get(num)
        if name is None:
            if t is None:
                return None
            self.add(path, 'UNMAPPED', old=str(num))
            self.fail(f'{path}: BF1943 number {num} has no known BFBC2 name')
            return copy.deepcopy(t)
        out = dbx.Element(e.tag, dbx.TEXT, list(e.attributes), name, b'', None)
        self.add(path, 'value', old=str(num), new=name)
        return out

    def convert_untemplated(self, e: dbx.Element, path: str, tpath: str, itype: str) -> dbx.Element:
        """A leaf with no template: the enum maps still apply; the catalogue's shape is checked."""
        n = e.attr('name') or ''
        owner = itype.rsplit('.', 1)[-1] if tpath.count('.') == 1 else \
            path.rsplit('.', 1)[0].rsplit('.', 1)[-1].split('[', 1)[0]
        if e.kind == dbx.NUMBERS:
            conv = self.enum_number(e, None, path, owner, n)
            if conv is not None:
                return conv
        if self.cat is not None:
            seen = self.cat.shapes.get((itype, tpath))
            if seen:
                s = _shape(e)
                want = {('text' if x in ('field-text', 'complex-text') else x) for x in seen}
                ok = s in want or (s.startswith('num') and any(w.startswith(s.split('x')[0] + 'x') for w in want)
                                   and len({w for w in want if w.startswith('num')}) > 1)
                if not ok:
                    self.errors.append(f'{path}: BF1943 {s}, BFBC2 {"/".join(sorted(want))} (no template)')
        return e

    def align_array(self, e: dbx.Element, t: dbx.Element | None, path: str, tpath: str, itype: str,
                    step: str) -> dbx.Element:
        items = list(e.children or ())
        if t is not None and t.tag != 'array':
            self.schema(path, e, t)
            return e
        titems = list(t.children or ()) if t is not None and t.kind == dbx.CONTAINER else []
        proto = None
        if not titems and t is not None and any(c.kind == dbx.CONTAINER for c in items):
            proto = self.item_proto(itype, tpath, t)
        keep: list[dbx.Element] = []
        pruned = 0
        tref = bool(titems) and titems[0].attr('ref') is not None and not titems[0].children
        for i, c in enumerate(items):
            ipath = f'{path}[{i}]'
            r = c.attr('ref')
            if titems and (r is not None and not c.children) != tref:
                inline = self.inlined_item(c, titems[i] if i < len(titems) else titems[0], ipath, tpath, itype, step)
                if inline is not None:
                    keep.append(inline)
                    continue
                # items of another kind than BFBC2's (references where BFBC2 holds structs inline, or the reverse)
                self.schema(ipath, c, titems[0])
                keep.append(c)
                continue
            if r is not None and not c.children:
                if self.ref_ok(r):
                    keep.append(c)
                    continue
                if '/' in r and self.checking:
                    p, g = r.rsplit('/', 1)
                    moved = self.relocation(p, g)
                    if moved is not None:
                        new = f'{moved}/{g}'
                        self.add(ipath, 'relocated', old=r, new=new, source=moved)
                        keep.append(_with_ref(c, new))
                        continue
                target = '#' + r.lower() if '/' not in r else r.rsplit('/', 1)[0].lower()
                si = self.stand_in(path.rsplit('.', 1)[-1].split('[', 1)[0], target)
                if isinstance(si, str):
                    new = self.stand_in_ref(si, r)
                    self.add(ipath, 'substituted', old=target, new=new, source=new.rsplit('/', 1)[0], note='stand-in')
                    keep.append(_with_ref(c, new))
                    continue
                pruned += 1
                continue
            if c.kind == dbx.CONTAINER:
                if self.o.get('prune') and self.lost_inside(c):
                    pruned += 1
                    continue
                ti = titems[i] if i < len(titems) else titems[0] if titems else proto
                if ti is not None and ti.kind != dbx.CONTAINER:
                    ti = None
                got = self.align(c, ti, ipath, tpath + '[]', itype, step)
                if got is not None:
                    keep.append(got)
                continue
            # a value item (numbers, text)
            keep.append(c)
        if pruned:
            self.add(path, 'pruned', note=f'{pruned} item(s) referring to what will not exist')
        if not keep:
            return _empty_array(e)
        return dbx.Element(e.tag, dbx.CONTAINER, list(e.attributes), e.text, e.raw, keep)

    def inlined_item(self, c: dbx.Element, ti: dbx.Element, ipath: str, tpath: str, itype: str,
                     step: str) -> dbx.Element | None:
        """INLINE converter: BF1943's reference to a local instance where BFBC2 holds the struct inline (DICE's mp_005
        RichPresenceConnection.ContextPairs, BF1943's RichPresenceContextPair instances): the instance's fields as the
        item, aligned on BFBC2's; the instance is left out when nothing else names it."""
        r = c.attr('ref')
        if r is None or c.children or '/' in r or ti.kind != dbx.CONTAINER or ti.attr('ref') is not None:
            return None
        inst = self.by_guid.get(r.lower())
        if inst is None:
            return None
        item = dbx.Element('complex', dbx.CONTAINER, [], '', b'', copy.deepcopy(inst.children or []))
        out = self.align(item, ti, ipath, tpath + '[]', itype, step)
        self.inlined.add(r.lower())
        self.add(ipath, 'converted', note=f'a reference to {inst.attr("type")} ({inst.attr("guid")}) as BFBC2\'s '
                 'inline struct')
        return out

    def lost_inside(self, e: dbx.Element) -> bool:
        """Whether a struct item holds a reference (any depth) that will not resolve, even relocated."""
        r = e.attr('ref')
        if r is not None and not e.children and not self.ref_ok(r):
            if '/' in r and self.checking:
                p, g = r.rsplit('/', 1)
                if self.relocation(p, g) is not None:
                    return False
            return True
        return any(self.lost_inside(c) for c in e.children or ())

    def align_struct(self, e: dbx.Element, t: dbx.Element | None, path: str, tpath: str, itype: str,
                     step: str) -> dbx.Element:
        mine, dups = _named(e.children)
        for dn in dups:
            self.add(f'{path}.{dn}', 'duplicate')
        if t is None:
            kept: list[dbx.Element] = []
            for c in e.children or ():
                cn = c.attr('name')
                if cn is None:
                    kept.append(c)
                    continue
                if cn in dups and c is not mine[cn]:
                    continue
                a = self.align(c, None, f'{path}.{cn}', f'{tpath}.{cn}', itype, step)
                if a is not None:
                    kept.append(a)
            return dbx.Element(e.tag, e.kind, list(e.attributes), e.text, e.raw, kept)
        theirs, _ = _named(t.children)
        here = path.rsplit('.', 1)[-1].split('[', 1)[0]
        new: list[dbx.Element] = []
        hoisted: set[str] = set()
        for c in t.children or ():
            n = c.attr('name')
            if n is None:
                continue
            p, tp = f'{path}.{n}', f'{tpath}.{n}'
            single = ONE_TO_ARRAY.get(n)
            if n not in mine and single in mine and c.tag == 'array':
                # BF1943's single reference -> BFBC2's array of references (Mesh1p -> Meshes1p): one item, or none
                r = mine[single].attr('ref') or 'null'
                if r == 'null':
                    arr = _empty_array(c)
                    self.add(p, 'converted', note=f'{single} -> {n}: null, an empty array')
                else:
                    item = dbx.Element('item', dbx.TEXT, [('ref', r)], '', b'', None)
                    arr = self.align(dbx.Element('array', dbx.CONTAINER, list(c.attributes), '', b'', [item]),
                                     c, p, tp, itype, step) or _empty_array(c)
                    self.add(p, 'converted', note=f'{single} -> {n} (one item)')
                new.append(arr)
                continue
            old = RENAMED.get(n)
            if n not in mine and old in mine:
                el = copy.deepcopy(mine[old])
                el.attributes = [(k, n if k == 'name' else v) for k, v in el.attributes]
                a = self.align(el, c, p, tp, itype, step)
                new.append(a if a is not None else c)
                self.add(p, 'renamed', note=f'{old} -> {n}')
                continue
            moved = NESTED.get((here, n))
            if n not in mine and moved and c.kind == dbx.CONTAINER and any(m in mine for m in moved):
                new.append(self.nested(mine, moved, c, p, tp, itype, step))
                hoisted.update(m for m in moved if m in mine)
                continue
            if n in mine:
                a = self.align(mine[n], c, p, tp, itype, step)
                if a is not None:
                    new.append(a)
            else:
                new.append(self.added(c, p, step))
        converted = {ONE_TO_ARRAY[n] for n in theirs if n in ONE_TO_ARRAY} | {RENAMED[n] for n in theirs if n in RENAMED}
        for n in mine:
            if n not in theirs and n not in converted and n not in hoisted:
                self.add(f'{path}.{n}', 'left out')
        # unnamed children the template has none of (rare): kept as they are
        new += [c for c in (e.children or ()) if c.attr('name') is None and e.tag != 'array']
        return dbx.Element(e.tag, e.kind, list(e.attributes), e.text, e.raw, new)

    def nested(self, mine: dict, moved: tuple, c: dbx.Element, p: str, tp: str, itype: str, step: str) -> dbx.Element:
        """BFBC2's struct c built from BF1943's fields one level up (NESTED); with DICE's conventions the template's
        values (DICE zeroed or retuned BF1943's recoil everywhere)."""
        conv = self.o.get('conventions', True)
        kids, used = [], []
        for k in c.children or ():
            kn = k.attr('name')
            if kn in mine and kn in moved:
                used.append(kn)
                if conv:
                    kids.append(copy.deepcopy(k))
                    old, new = dbx.value(mine[kn]) if mine[kn].kind != dbx.CONTAINER else '...', \
                        dbx.value(k) if k.kind != dbx.CONTAINER else '...'
                    if old != new:
                        self.add(f'{p}.{kn}', 'convention', old=old, new=new,
                                 note='recoil: the template\'s (DICE retuned BF1943\'s)')
                else:
                    a = self.align(mine[kn], k, f'{p}.{kn}', f'{tp}.{kn}', itype, step)
                    kids.append(a if a is not None else k)
            else:
                kids.append(self.added(k, f'{p}.{kn}', step))
        self.add(p, 'nested', note=f'{{{", ".join(u or "" for u in used)}}} -> {p.rsplit(".", 1)[-1]}' +
                 (' (the template\'s values: DICE\'s convention)' if conv else ''))
        return dbx.Element(c.tag, c.kind, list(c.attributes), c.text, c.raw, kids)

    def added(self, c: dbx.Element, path: str, step: str) -> dbx.Element:
        """A field only BFBC2 has: the template's value, its references checked (the template's own instances are
        not here; another partition's must exist; a class default's references are another object's: null)."""
        out = copy.deepcopy(c)
        src = self.chosen_partition
        self.add(path, 'added', new=_short(out), source=src)
        return self.template_refs(out, path, step)

    def template_refs(self, e: dbx.Element, path: str, step: str) -> dbx.Element:
        r = e.attr('ref')
        if r is not None and not e.children:
            if r == 'null':
                return e
            if step == 'default':
                self.add(path, 'nulled', old=r.rsplit('/', 1)[0] if '/' in r else '#' + r.lower(),
                         note='the class default\'s reference names another object')
                return _with_ref(e, 'null')
            if '/' not in r:
                if step == 'counterpart' and r.lower() in self.local and r.lower() not in self.dropped:
                    return e
                self.add(path, 'nulled', old='#' + r.lower(), note='the template\'s own instance')
                return _with_ref(e, 'null')
            if not self.guard_ok(path, r):
                return _with_ref(e, 'null')
            new, how = self.resolve_ref(r, path, None, step, from_template=True)
            if how == 'null':
                self.add(path, 'nulled', old=r.rsplit('/', 1)[0], note='the template\'s reference will not exist')
            return e if new == r else _with_ref(e, new)
        if e.children:
            keep = []
            for i, c in enumerate(e.children):
                if e.tag == 'array' and not c.children and c.attr('ref') is not None:
                    cr = c.attr('ref') or ''
                    if cr == 'null':
                        keep.append(c)
                        continue
                    if step == 'default' or '/' not in cr and not (step == 'counterpart' and cr.lower() in self.local
                                                                  and cr.lower() not in self.dropped):
                        continue
                    if '/' in cr:
                        if not self.guard_ok(f'{path}[{i}]', cr):
                            continue
                        new, how = self.resolve_ref(cr, f'{path}[{i}]', None, step, from_template=True)
                        if how == 'null':
                            continue
                        keep.append(_with_ref(c, new) if new != cr else c)
                        continue
                    keep.append(c)
                    continue
                n = c.attr('name')
                keep.append(self.template_refs(c, f'{path}.{n}' if n is not None else f'{path}[{i}]', step))
            if len(keep) != len(e.children):
                self.add(path, 'emptied', note=f'{len(e.children) - len(keep)} item(s) naming what will not exist')
            if e.tag == 'array' and not keep:
                return _empty_array(e)
            e.children = keep
        return e

    chosen_partition: str | None = None


def _short(e: dbx.Element) -> str:
    if e.kind == dbx.CONTAINER:
        return f'<{e.tag} {len(e.children or [])}>'
    r = e.attr('ref')
    if r is not None:
        return r
    try:
        v = dbx.value(e)
    except ValueError:
        return '?'
    return v if len(v) <= 80 else v[:77] + '...'


def _at_path(e: dbx.Element, tpath: str) -> dbx.Element | None:
    """The element at a schema path ('.A.B', '.Arr[].C': an array's first item) under e."""
    cur: dbx.Element | None = e
    for part in re.findall(r'\.([^.\[]+)|(\[\])', tpath):
        if cur is None:
            return None
        cur = (cur.children[0] if cur.children else None) if part[1] else _field(cur, part[0])
    return cur


def port(data: bytes, templates: Templates, resolve: Callable[[str], bool] | None = None, drop_untemplated: bool = False,
         drop_types: Iterable[str] = (), prune: bool = False, required: dict[str, str] | None = None, *,
         catalogue: Any = None, ours: dict[str, set[str]] | None = None, stand_ins: dict | None = None,
         donors: Iterable[str] = (), strict: bool = False, schema: str = 'error', conventions: bool = True,
         bones: BoneTables | None = None, name: str | None = None) -> tuple[bytes, Report]:
    """(BFBC2 .dbx bytes, Report) for a BF1943 partition.

    resolve(partition): whether a referenced partition will exist (the caller's partition-level answer: shader folders,
      the partitions it ports); None, with no ours: references into other partitions are kept as they are.
    ours {partition: guids}: the partitions the build writes, with their guids (guids_of), checked per guid.
    catalogue: dbxcatalogue.Catalogue (default: templates.catalogue) - BFBC2's guids (references checked per guid,
      relocated), class names, schema, class defaults.
    stand_ins {target partition | (field, target) | (field, '*'): 'partition/guid' | partition | None | TEMPLATE}:
      what stands in for a reference that will not resolve (target '#<guid>' for a left-out local instance).
    donors: the BFBC2 partitions paired with this one (their instances are the templates before the pool's).
    drop_untemplated: instances of a class BFBC2 lacks (the catalogue's classes and executable names) are left out.
    drop_types: types left out whatever the templates (NOT_IN_BFBC2).
    prune (a level): arrays of structs lose the items with a reference that will not resolve.
    required {type: field}: an instance whose reference in that field will not resolve is left out (REQUIRED).
    strict: an unresolved reference, a refused one, an unmapped enum or an untemplated type fails the port.
    schema: 'error' (SchemaError on a shape no converter handles) or 'report' ('schema' records).
    conventions: DICE's conventions for BF1943's weapons (DICE_CONVENTIONS; False: BF1943's values kept).
    bones: BoneTables - bone indices on BFBC2's skeleton.
    name: the partition's name (records, the weapon conventions)."""
    f = dbx.parse(data)
    P = _Port(f, templates, resolve, dict(catalogue=catalogue, ours=ours, stand_ins=stand_ins, strict=strict,
                                          prune=prune, conventions=conventions, name=name))
    cat = P.cat
    report = P.report
    part = f.partition
    drop_types = set(drop_types)
    if drop_untemplated and cat is None:
        raise PortError('drop_untemplated needs a catalogue (BFBC2\'s class names)')
    insts: list[dbx.Element] = []
    for inst in part.children or ():
        if inst.tag != 'instance':
            continue
        t = inst.attr('type') or ''
        if t in TYPE_AS and t not in templates and not (cat is not None and cat.has_class(t)):
            # a BF1943 class BFBC2 lacks whose instances BFBC2 writes as another class (the planes' pilot seat)
            new_t = TYPE_AS[t]
            inst = dbx.Element(inst.tag, inst.kind, [(k, new_t if k == 'type' else v) for k, v in inst.attributes],
                               inst.text, inst.raw, inst.children)
            P.add(f'{t} ({inst.attr("guid")})', 'type', old=t, new=new_t)
            t = new_t
        g = (inst.attr('guid') or '').lower()
        if t in drop_types:
            P.dropped.add(g)
            P.add(f'{t} ({inst.attr("guid")})', 'left out instance', note='BFBC2 has no such class')
        elif drop_untemplated and not cat.has_class(t) and t not in templates:
            P.dropped.add(g)
            P.add(f'{t} ({inst.attr("guid")})', 'left out instance', note='BFBC2 has no such class (catalogue)')
        elif conventions and t in DICE_MOVED_CLASSES:
            P.dropped.add(g)
            P.add(f'{t} ({inst.attr("guid")})', 'left out instance',
                  note='DICE keeps these out of the partition (persistence/weapons)')
        insts.append(inst)
    changed = True
    while changed:                             # required: a left-out placement can leave others without their object
        changed = False
        for inst in insts:
            g = (inst.attr('guid') or '').lower()
            fld = (required or {}).get(inst.attr('type') or '')
            if not fld or g in P.dropped:
                continue
            fe = _field(inst, fld)
            r = fe.attr('ref') if fe is not None else None
            if not r or r == 'null' or P.ref_ok(r):
                continue
            if '/' in r and P.checking and P.relocation(*r.rsplit('/', 1)) is not None:
                continue
            if '/' not in r and resolve is None:
                pass
            P.dropped.add(g)
            changed = True
            P.add(f'{inst.attr("type")} ({inst.attr("guid")})', 'left out instance',
                  note=f'its {fld} {r.rsplit("/", 1)[0] if "/" in r else r} will not exist')
    roles = _roles(insts)
    new: list[dbx.Element] = []
    for inst in insts:
        t = inst.attr('type') or ''
        g = (inst.attr('guid') or '').lower()
        if g in P.dropped:
            continue
        step, cand = templates.choose(inst, roles[g], donors, cat)
        P.chosen[g] = (step, cand)
        P.chosen_partition = cand.partition if cand is not None else None
        P.add(f'{t} ({inst.attr("guid")})', 'template', step=step, source=P.chosen_partition)
        if cand is None:
            P.add(f'{t} ({inst.attr("guid")})', 'untemplated')
            P.fail(f'{t}: no BFBC2 template and no class default')
        out = P.align(inst, cand.element if cand is not None else None, t, '', t, step)
        if out is not None:
            new.append(out)
    if P.inlined:                              # instances now held inline: left out unless something else names them
        still: set[str] = set()

        def named(e: dbx.Element) -> None:
            r = e.attr('ref')
            if r and '/' not in r:
                still.add(r.lower())
            for c in e.children or ():
                named(c)
        for inst in new:
            if (inst.attr('guid') or '').lower() not in P.inlined:
                named(inst)
        gone = P.inlined - still
        new = [i for i in new if (i.attr('guid') or '').lower() not in gone]
        P.errors = [e for e in P.errors if not any(g in e.lower() for g in gone)]
        for g in sorted(gone):
            P.add(f'#{g}', 'left out instance', note='held inline by what named it (BFBC2\'s shape)')
    if P.errors:
        if schema == 'error':
            raise SchemaError(P.errors)
        for err in P.errors:
            P.add(err.split(':', 1)[0], 'schema', note=err.split(':', 1)[1].strip())
    _part_link_names(new, P)
    by = {(i.attr('guid') or '').lower(): i for i in new}
    for inst in new:
        _values(inst, inst.attr('type') or '', '', P)
        if conventions:
            _conventions(inst, by, P)
        if bones is not None:
            _bones(inst, bones, P)
    _gate(new, P, schema)
    attrs = list(part.attributes)
    prim = (part.attr('primaryInstance') or '').lower()
    if prim and prim not in by and new:
        pick = next((i for i in new if i.attr('id') is not None), new[0])
        attrs = [(k, (pick.attr('guid') or '') if k == 'primaryInstance' else v) for k, v in attrs]
        P.add('primaryInstance', 'primary', old=prim, new=pick.attr('guid'), note='the primary instance was left out')
    new = [_text_empty_arrays(i) for i in new]
    written = dbx.DbxFile(f.strings, [dbx.Element(part.tag, part.kind, attrs, part.text, part.raw, new)] + f.roots[1:])
    return dbx.write(written), report


def _text_empty_arrays(e: dbx.Element) -> dbx.Element:
    """Empty arrays as TEXT elements (DICE writes all 9,400 of its empty arrays so)."""
    if e.tag == 'array' and e.kind == dbx.CONTAINER and not e.children:
        return _empty_array(e)
    if e.children:
        e.children = [_text_empty_arrays(c) for c in e.children]
    return e


def _values(e: dbx.Element, itype: str, path: str, P: _Port) -> None:
    """BF1943's spellings as BFBC2's (in place): EntryInputAction names in every field, the seat-hint texts."""
    for c in e.children or ():
        n = c.attr('name')
        p = f'{path}.{n}' if n is not None else f'{path}[]'
        if c.kind == dbx.TEXT and c.attr('ref') is None and c.tag != 'array' and c.text:
            v = c.text.strip()
            new = eia_value(v)
            if new == v and n in ('HintLine1', 'HintLine2'):
                new = HINT_TEXTS.get(v, v)
            if new != v:
                P.add(f'{itype}{p}', 'value', old=v, new=new)
                c.text = new
        if c.kind == dbx.CONTAINER:
            _values(c, itype, p, P)


def _gate(instances: list[dbx.Element], P: _Port, schema: str) -> None:
    """The output against the catalogue: each instance's fields in DICE's order, enum names the executable has."""
    cat = P.cat
    if cat is None:
        return
    problems = []
    for inst in instances:
        t = inst.attr('type') or ''
        order = cat.order.get(t)
        if order is not None:
            names = [c.attr('name') or '' for c in inst.children or () if c.attr('name') is not None]
            if names != order:
                miss = [x for x in order if x not in names]
                extra = [x for x in names if x not in order]
                problems.append(f'{t}: fields not DICE\'s ({"missing " + ",".join(miss[:4]) if miss else ""}'
                                f'{" extra " + ",".join(extra[:4]) if extra else ""}{" order" if not miss and not extra else ""})')

        def walk(e: dbx.Element, path: str, t: str = t) -> None:
            for c in e.children or ():
                n = c.attr('name')
                p = f'{path}.{n}' if n is not None else f'{path}[]'
                if c.kind == dbx.TEXT and c.attr('ref') is None and c.tag != 'array' and c.text:
                    valid = cat.enum_names(t, p)
                    if valid is not None and re.fullmatch(r'[A-Za-z][A-Za-z0-9_]*', c.text) and \
                            not cat.valid_enum(c.text):
                        P.add(f'{t}{p}', 'enum', old=c.text, note='not a name BFBC2\'s executable has')
                        P.fail(f'{t}{p}: {c.text} is no enum name BFBC2 has')
                if c.kind == dbx.CONTAINER:
                    walk(c, p, t)
        walk(inst, '', t)
    if problems:
        if schema == 'error':
            raise SchemaError(problems)
        for pr in problems:
            P.add(pr.split(':', 1)[0], 'schema', note=pr.split(':', 1)[1].strip())


# ------------------------------------------------------------------------------------------------ value passes

def _set_value(e: dbx.Element, P: _Port, path: str, new_raw: list[bytes], size: int, why: str, action: str) -> None:
    old = dbx.value(e)
    _set_numbers(e, new_raw, size)
    if dbx.value(e) != old:
        P.add(path, action, old=old, new=dbx.value(e), note=why)


def _template_of(inst: dbx.Element, P: _Port) -> tuple[str, dbx.Element | None]:
    step, cand = P.chosen.get((inst.attr('guid') or '').lower(), ('none', None))
    return step, (cand.element if cand is not None else None)


def _conventions(inst: dbx.Element, by: dict[str, dbx.Element], P: _Port) -> None:
    """DICE's conventions for BF1943's weapons (DICE_CONVENTIONS above), in place."""
    t = inst.attr('type') or ''
    short = t.rsplit('.', 1)[-1]
    step, tpl = _template_of(inst, P)
    path = t
    if short == 'WeaponFiringData':
        a = _field(inst, 'UseAutoAiming')
        if a is not None and a.kind not in (dbx.TEXT, dbx.NUMBERS, dbx.CONTAINER) and a.raw[1:] == b'\x01':
            a.raw = a.raw[:1] + b'\x00'
            P.add(f'{path}.UseAutoAiming', 'convention', old='true', new='false', note='DICE: no auto-aim')
    if not _weaponish(P, step):
        return
    if short == 'FiringFunctionData':
        shot = _field(inst, 'Shot')
        speed = _field(shot, 'InitialSpeed')
        proj = _field(shot, 'ProjectileData')
        target = by.get((proj.attr('ref') or '').lower()) if proj is not None else None
        ptype = (target.attr('type') or '').rsplit('.', 1)[-1] if target is not None else ''
        z = _num(speed)[2] if speed is not None and len(_num(speed)) >= 3 else None
        if z is not None and ptype == 'BulletEntityData' and 300 <= z < 600 and speed is not None:
            vals = _num(speed)
            vals[2] = 600.0
            _set_value(speed, P, f'{path}.Shot.InitialSpeed', [struct.pack('>f', v) for v in vals], 4,
                       'DICE: firearm bullets at 600 m/s', 'convention')
        if z is not None and ptype == 'GrenadeEntityData' and abs(z - 24) < 1e-3 and speed is not None:
            vals = _num(speed)
            vals[2] = 20.0
            _set_value(speed, P, f'{path}.Shot.InitialSpeed', [struct.pack('>f', v) for v in vals], 4,
                       'DICE: hand grenades thrown at 20', 'convention')
        if target is not None and ptype == 'BulletEntityData' and z is not None and z >= FIREARM_SPEED:
            gr = _field(target, 'Gravity')
            if gr is not None and _num(gr) == [0.0]:
                _set_value(gr, P, f'{target.attr("type")}.Gravity', [struct.pack('>f', -9.81)], 4,
                           'DICE: bullet drop on firearms', 'convention')
        if target is not None and ptype == 'GrenadeEntityData':
            gr = _field(target, 'Gravity')
            if gr is not None and _num(gr) and abs(_num(gr)[0] + 9) < 1e-3:
                _set_value(gr, P, f'{target.attr("type")}.Gravity', [struct.pack('>f', -8.0)], 4,
                           'DICE: hand grenades', 'convention')
        ammo, tammo = _field(inst, 'Ammo'), _field(tpl, 'Ammo')
        firearm = z is not None and z >= FIREARM_SPEED and ptype == 'BulletEntityData'
        if ammo is not None and tammo is not None and firearm:
            mags, tm = _field(ammo, 'NumberOfMagazines'), _field(tammo, 'NumberOfMagazines')
            if mags is not None and tm is not None and mags.kind == dbx.NUMBERS and _ints(mags) == [-1] and \
                    _ints(tm) != [-1]:
                _set_value(mags, P, f'{path}.Ammo.NumberOfMagazines', dbx.numbers(tm)[1], 4,
                           'DICE: finite magazines (the template\'s)', 'convention')
            lo, hi = _field(ammo, 'AmmoPickupMinAmount'), _field(ammo, 'AmmoPickupMaxAmount')
            tlo, thi = _field(tammo, 'AmmoPickupMinAmount'), _field(tammo, 'AmmoPickupMaxAmount')
            if lo is not None and hi is not None and tlo is not None and thi is not None and \
                    _ints(lo) == [0] and _ints(hi) == [0]:
                _set_value(lo, P, f'{path}.Ammo.AmmoPickupMinAmount', dbx.numbers(tlo)[1], 4,
                           'DICE: ammo pickups (the template\'s)', 'convention')
                _set_value(hi, P, f'{path}.Ammo.AmmoPickupMaxAmount', dbx.numbers(thi)[1], 4,
                           'DICE: ammo pickups (the template\'s)', 'convention')
    elif short == 'SoldierAimingSimulationData':
        zt, tz = _field(inst, 'ZoomType'), _field(tpl, 'ZoomType')
        if zt is not None and zt.text == 'ztHoldButtonToZoom' and \
                not (tz is not None and tz.text == 'ztHoldButtonToZoom'):
            P.add(f'{path}.ZoomType', 'convention', old=zt.text, new='ztPressButtonToSwitchZoomLevel',
                  note='DICE: press to zoom')
            zt.text = 'ztPressButtonToSwitchZoomLevel'
    elif short == 'ZoomLevelData':
        s = _field(inst, 'SprintLookSpeedMultiplier')
        if s is not None and s.kind == dbx.NUMBERS and _num(s) == [0.5]:
            _set_value(s, P, f'{path}.SprintLookSpeedMultiplier', [struct.pack('>f', 1.0)], 4,
                       'DICE: full look speed while sprinting', 'convention')
    elif short == 'SoldierWeaponData':
        a, ta = _field(inst, 'AnimBaseSet'), _field(tpl, 'AnimBaseSet')
        if a is not None:
            new = None
            if a.text == 'wabsShg':
                new = 'wabsRif'
            elif step in ('counterpart', 'donor') and ta is not None and ta.text and ta.text != a.text:
                new = ta.text
            if new:
                P.add(f'{path}.AnimBaseSet', 'convention', old=a.text, new=new, note='DICE\'s body set for the class')
                a.text = new
        states, tstates = _field(inst, 'WeaponStates'), _field(tpl, 'WeaponStates')
        tlist = (tstates.children if tstates is not None else None) or []
        for i, s in enumerate((states.children if states is not None else None) or ()):
            ts = tlist[min(i, len(tlist) - 1)] if tlist else None
            h, th = _field(s, 'AlwaysAimHead'), _field(ts, 'AlwaysAimHead')
            if h is not None and th is not None and h.raw[1:] == b'\x00' and th.raw[1:] == b'\x01':
                h.raw = h.raw[:1] + b'\x01'
                P.add(f'{path}.WeaponStates[{i}].AlwaysAimHead', 'convention', old='false', new='true',
                      note='DICE: the class\'s (the template\'s)')


def _weaponish(P: _Port, step: str) -> bool:
    """Whether the partition is a weapon's (its name, or its templates'): the weapon conventions apply."""
    if P.name:
        return bool(WEAPON_PARTITION.search(P.name))
    return any(c is not None and WEAPON_PARTITION.search(c.partition) for _, c in P.chosen.values())


def _bones(inst: dbx.Element, B: BoneTables, P: _Port) -> None:
    """Bone indices on BFBC2's skeleton (in place)."""
    t = inst.attr('type') or ''
    short = t.rsplit('.', 1)[-1]
    if short == 'BoneFakePhysicsData':
        name, bid = _field(inst, 'BoneName'), _field(inst, 'BoneId')
        if name is not None and bid is not None and bid.kind == dbx.NUMBERS:
            new = B.at.get(name.text.strip())
            old = _ints(bid)
            if new is not None and old != [new] and old != [0]:    # 0: an unset entry (DICE keeps it 0)
                _set_value(bid, P, f'{t}.BoneId', [struct.pack('>i', new)], 4, f'bone {name.text.strip()}', 'bones')
        return
    nm = _field(inst, 'Name')
    if short == 'SkeletonAsset' and (nm is None or nm.text.strip().lower() != B.skeleton):
        return
    if short not in ('SkeletonAsset', 'SoldierIKData') and not _field(inst, 'WeaponBoneIndex'):
        return

    def walk(e: dbx.Element, path: str) -> None:
        for c in e.children or ():
            n = c.attr('name') or ''
            p = f'{path}.{n}' if n else f'{path}[]'
            if c.kind == dbx.NUMBERS and n in ('WeaponBoneIndex', 'BoneIndices', 'MinBoneIndex', 'MaxMainBoneIndex'):
                old = _ints(c)
                if n == 'MinBoneIndex' and _field(e, 'MinBoneName') is not None:
                    w = B.at.get((_field(e, 'MinBoneName') or dbx.Element('f', dbx.TEXT)).text.strip())
                    new = [w if w is not None else v for v in old]
                elif n == 'MaxMainBoneIndex':
                    mapped = [B.index(i) for i in range(old[0] + 1)] if old and old[0] >= 0 else []
                    got = [m for m in mapped if m is not None]
                    contiguous = bool(mapped) and len(got) == len(mapped) and sorted(got) == list(range(len(got)))
                    new = [max(got) if contiguous else len(B.target) - 1] if old else old
                else:
                    new = [B.map(v) for v in old]
                if new != old:
                    _set_value(c, P, f'{t}{p}', [struct.pack('>i', v) for v in new], 4, 'BFBC2\'s skeleton', 'bones')
            elif c.tag == 'array' and n == 'BoneIndices' and c.children:
                for it in c.children:
                    if it.kind == dbx.NUMBERS:
                        old = _ints(it)
                        new = [B.map(v) for v in old]
                        if new != old:
                            _set_value(it, P, f'{t}{p}[]', [struct.pack('>i', v) for v in new], 4,
                                       'BFBC2\'s skeleton', 'bones')
            if c.kind == dbx.CONTAINER:
                walk(c, p)
    walk(inst, '')


def _part_link_names(instances: list[dbx.Element], P: _Port) -> None:
    """A part link's PartComponentName1 / 2 (BFBC2 only; a template's value after aligning): the names of the
    components its indices point at, in its object's Components list - as DICE's files hold them (house_01_10x8,
    barrack, house_12_14x8: 112 of 112 links)."""
    by = {(i.attr('guid') or '').lower(): i for i in instances if i.tag == 'instance'}
    for i in by.values():
        links, comps = _field(i, 'PartLinks'), _field(i, 'Components')
        if links is None or comps is None:
            continue
        names = []
        for c in comps.children or ():
            target = by.get((c.attr('ref') or '').lower())
            n = _field(target, 'Name') if target is not None else None
            names.append(n.text if n is not None else None)
        fixed = 0
        for item in links.children or ():
            link = by.get((item.attr('ref') or '').lower())
            if link is None:
                continue
            for k in ('1', '2'):
                idx, name = _field(link, 'PartComponentIndex' + k), _field(link, 'PartComponentName' + k)
                if idx is None or name is None:
                    continue
                j = int(dbx.value(idx))
                want = names[j] or '' if 0 <= j < len(names) else ''
                if name.text != want:
                    name.text = want
                    fixed += 1
        if fixed:
            P.add(i.attr('type') or '', 'derived', note=f'{fixed} part link names from their indices')
