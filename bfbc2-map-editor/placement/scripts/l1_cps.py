"""L1 exploration: capture points, their spawners, geometry and combat areas of a level (output not kept)."""
import sys
from collections import Counter

from l1_level import load


def r(p):
    return None if p is None else tuple(round(x) for x in p)


def main(lv):
    L = load(lv)
    print('==', lv, L.entry, 'events', len(L.events))
    for c in L.of('CapturePointEntityData'):
        sp = c.refs('Spawners')
        tys = Counter(L.insts[s].type if s in L.insts else '?' for s in sp)
        geo = c.refs('Geometry')
        gt = Counter(L.insts[s].type if s in L.insts else '?' for s in geo)
        rad = [round(L.insts[g].num('Radius') or 0, 1) for g in geo if g in L.insts and L.insts[g].type == 'SphereGeometryEntityData']
        teams = Counter(L.insts[s].text('Team') for s in sp if s in L.insts)
        print(f"CP {c.name!r:28} loc={c.text('LocationName')!r:14} team={c.text('InitialOwnerTeam'):11} base={c.text('UnableToChangeTeam')} "
              f"en={c.text('Enabled')} vis={c.text('IsVisible')} idx={c.whole('Index')} capR={c.num('CaptureRadius')} "
              f"t={c.num('TimeToGetControl')} pos={r(c.pos)} spawners={dict(tys)} teams={dict(teams)} geo={dict(gt)} rad={rad} "
              f"flag={bool(c.extrefs('CapturePoint'))} only={c.whole('OnlyTakeableByTeam')}")
    for c in L.of('CombatAreaTriggerEntityData'):
        print(f"CA {c.name!r} team={c.text('Team')} en={c.text('Enabled')} restrict={c.text('UseTeamRestriction')} "
              f"geo={len(c.refs('Geometry'))} sur={len(c.refs('SurroundingGeometry'))}")
    cnt = Counter(i.type for i in L.insts.values())
    for t in ('SoldierSpawnEntityData', 'AlternateSpawnEntityData', 'VehicleSpawnEntityData', 'TicketCounterEntityData',
              'LifeCounterEntityData', 'KillCounterEntityData', 'TeamEntityData', 'SphereGeometryEntityData',
              'DamageAreaTriggerEntityData', 'AreaTriggerEntityData', 'ClearAreaTriggerEntityData'):
        print(f'  {t}: {cnt.get(t, 0)}')
    ss = L.of('SoldierSpawnEntityData')
    print('  spawn PlayerType', Counter(s.text('PlayerType') for s in ss), 'team', Counter(s.text('Team') for s in ss))


if __name__ == '__main__':
    for lv in sys.argv[1:]:
        main(lv)
