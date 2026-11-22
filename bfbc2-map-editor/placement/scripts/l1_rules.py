"""L1: the proposed editor rules (V1) checked against DICE's stock levels - how many records each rule would flag.

    python l1_rules.py [l1_measurements.json] [out.md]

Each rule: an ID, the metric it reads, its scope (modes, era groups), a warning band and an info band (a value outside
the band is flagged), the message. The evidence column counts the stock records the rule would flag: a good rule
flags none (or names the few DICE exceptions it accepts). Writes l1_rules_check.md and l1_rules.json.
"""
import json
import os
import sys
from collections import defaultdict

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
DICE = ('BFBC2', 'BFBC1', 'Vietnam')
INF = float('inf')

# id, metric, modes, groups, warn (lo, hi), info (lo, hi), reduce, where, message ({v} = the value)
RULES = [
    # ---- Conquest
    ('L1-CQ-001', 'cq.flag_count', ('CQ',), DICE, (1, INF), (3, 4), None, None,
     'Conquest has {v:.0f} flags; DICE\'s Conquest maps have 3-4.'),
    ('L1-CQ-001w', 'cq.flag_count', ('CQ',), ('WW2',), (1, INF), (5, 5), None, None,
     'WW2 Conquest has {v:.0f} flags; the BF1943 maps have 5.'),
    ('L1-CQ-002', 'cq.flag_nn_dist', ('CQ',), DICE, (50, INF), (70, 300), None, None,
     'Flag {s} is {v:.0f} m from the nearest other flag; DICE keeps flags 58-450 m apart (median 121 m).'),
    ('L1-CQ-002w', 'cq.flag_nn_dist', ('CQ',), ('WW2',), (80, INF), (100, 250), None, None,
     'Flag {s} is {v:.0f} m from the nearest other flag; the WW2 maps keep 103-205 m.'),
    ('L1-CQ-003', 'cq.hq_nearest_flag', ('CQ',), DICE, (80, INF), (100, 400), None, None,
     'HQ {s} is {v:.0f} m from its nearest flag; on DICE\'s maps an HQ is 85-384 m from the nearest flag.'),
    ('L1-CQ-003w', 'cq.hq_nearest_flag', ('CQ',), ('WW2',), (150, INF), (200, 650), None, None,
     'HQ {s} is {v:.0f} m from its nearest flag; on the WW2 maps 202-633 m.'),
    ('L1-CQ-004', 'cq.hq_hq_dist', ('CQ',), DICE, (250, INF), (280, 1000), None, None,
     'The HQs are {v:.0f} m apart; DICE\'s are 285-1358 m apart (median 424 m).'),
    ('L1-CQ-005', 'cq.hq_nearest_flag_asymmetry', ('CQ',), DICE + ('WW2',), (-INF, INF), (0, 60), None, None,
     'One HQ is {v:.0f} m closer to its nearest flag than the other; DICE keeps this within 54 m.'),
    ('L1-CQ-006', 'cq.flag_axis_t', ('CQ',), DICE + ('WW2',), (0.0, 1.0), (0.1, 0.9), None, None,
     'Flag {s} is not between the HQs (at {v:.2f} of the way from Team 1\'s HQ); DICE\'s flags lie at 0.1-0.9.'),
    ('L1-CQ-007', 'cq.flag_axis_lateral', ('CQ',), DICE, (-INF, INF), (0, 0.5), None, None,
     'Flag {s} is {v:.2f} HQ-distances off the line between the HQs; DICE\'s are within 0.4.'),
    ('L1-CQ-008', 'cq.capture_zone', ('CQ',), DICE + ('WW2',), (5, INF), (7, 40),
     None, lambda r: not r['zone']['kind'].startswith('aabb'),
     'Flag {s} has a capture zone of {v:.0f} m (equivalent radius); DICE\'s are 7.5-37 m.'),
    ('L1-CQ-009', 'cq.time_to_capture', ('CQ',), DICE, (-INF, INF), (20, 25), None, None,
     'Flag {s} takes {v:.0f} s to capture; DICE\'s Conquest flags take 20 s (one 25 s).'),
    ('L1-CQ-009w', 'cq.time_to_capture', ('CQ',), ('WW2',), (-INF, INF), (10, 10), None, None,
     'Flag {s} takes {v:.0f} s to capture; BF1943\'s take 10 s.'),
    ('L1-CQ-010', 'cq.flag_spawn_points', ('CQ',), DICE + ('WW2',), (1, INF), (2, INF), None, None,
     'Flag {s} has {v:.0f} spawn points; DICE\'s flags have 2-21 (median 12; WW2 4-5).'),
    ('L1-CQ-011', 'cq.flag_spawn_dist_max', ('CQ',), DICE + ('WW2',), (-INF, 200), (-INF, 140), None, None,
     'A spawn point of flag {s} is {v:.0f} m from the flag; DICE\'s are within 137 m (median farthest 61 m).'),
    ('L1-CQ-012', 'cq.hq_spawn_points', ('CQ',), DICE + ('WW2',), (1, INF), (4, INF), None, None,
     'HQ {s} has {v:.0f} spawn points; DICE\'s HQs have 4-13 (median 9).'),
    ('L1-CQ-013', 'cq.flag_margin', ('CQ',), DICE + ('WW2',), (0, INF), (10, INF), None, None,
     'Flag {s} is {v:.0f} m inside the combat area edge (negative: outside); DICE\'s are 11 m or more inside.'),
    ('L1-CQ-014', 'cq.hq_margin', ('CQ',), DICE + ('WW2',), (0, INF), (1, INF), None, None,
     'HQ {s} is {v:.0f} m inside its team\'s combat area (negative: outside); every DICE HQ is inside.'),
    ('L1-CQ-015', 'cq.hq_vs_enemy_area', ('CQ',), DICE + ('WW2',), (-INF, 0), (-INF, -3), None, None,
     'HQ {s} is inside the enemy\'s combat area by {v:.0f} m; where each team has its own area, DICE keeps every '
     'HQ outside the enemy\'s (by 3-88 m).'),
    ('L1-CQ-016', 'cq.tickets', ('CQ',), DICE, (-INF, INF), (200, 350), None, None,
     'Conquest starts with {v:.0f} tickets; DICE\'s maps use 200-350.'),
    ('L1-CQ-017', 'cq.combat_area_union', ('CQ',), DICE, (-INF, INF), (40000, 500000), None, None,
     'The combat area is {v:.0f} m2; DICE\'s Conquest areas are 44,000-479,000 m2.'),
    # ---- Rush (Rush and Squad Rush scoped separately)
    ('L1-RU-001', 'rush.stage_count', ('Rush',), DICE, (2, INF), (3, 5), None, None,
     'Rush has {v:.0f} stages; DICE\'s Rush maps have 3-5.'),
    ('L1-RU-002', 'rush.mcoms_per_stage', ('Rush',), DICE, None, None, None, None,
     'A Rush stage has {v} M-COMs; every DICE Rush stage has 2.'),
    ('L1-RU-003', 'rush.mcom_pair_dist', ('Rush',), DICE, (10, 120), (25, 80), None, None,
     'The stage {st} M-COMs are {v:.0f} m apart; DICE\'s pairs are 12-91 m apart (median 40 m).'),
    ('L1-RU-004', 'rush.stage_step', ('Rush',), DICE, (75, INF), (100, 320), None, None,
     'Stage {st} is {v:.0f} m from stage {pst}; DICE\'s Rush stages are 93-316 m apart (median 174 m).'),
    ('L1-RU-004s', 'sr.stage_step', ('SR',), DICE, (40, INF), (60, 180), None, None,
     'Squad Rush stage {st} is {v:.0f} m from stage {pst}; DICE\'s are 45-169 m apart (median 119 m).'),
    ('L1-RU-005', 'rush.defender_spawn_to_obj_min', ('Rush',), DICE, (-INF, 100), (5, 65), None, None,
     'The defenders\' nearest spawn is {v:.0f} m from a stage {st} M-COM; DICE\'s are 6-61 m (median 30 m).'),
    ('L1-RU-005s', 'sr.defender_spawn_to_obj_min', ('SR',), DICE, (-INF, 100), (20, 75), None, None,
     'The defenders\' nearest spawn is {v:.0f} m from the stage {st} M-COM; DICE\'s are 23-72 m.'),
    ('L1-RU-006', 'rush.attacker_spawn_to_obj_min', ('Rush',), DICE, (40, INF), (60, 460), None, None,
     'The attackers\' nearest spawn is {v:.0f} m from a stage {st} M-COM; DICE\'s are 72-459 m (median 160 m).'),
    ('L1-RU-006s', 'sr.attacker_spawn_to_obj_min', ('SR',), DICE, (40, INF), (50, 170), None, None,
     'The attackers\' nearest spawn is {v:.0f} m from the stage {st} M-COM; DICE\'s are 55-165 m.'),
    ('L1-RU-007', 'rush.spawn_ratio', ('Rush',), DICE, (1.0, INF), (1.5, INF), None, None,
     'At stage {st} the attackers spawn only {v:.1f} times as far from the M-COMs as the defenders; DICE: 1.8-21x '
     '(median 5x).'),
    ('L1-RU-007s', 'sr.spawn_ratio', ('SR',), DICE, (0.75, INF), (1.0, INF), None, None,
     'At stage {st} the attackers spawn only {v:.1f} times as far from the M-COM as the defenders; DICE Squad Rush: 1.0-4.4x.'),
    ('L1-RU-008', 'rush.defender_spawn_depth', ('Rush', 'SR'), DICE, (-30, INF), (0, INF), None, None,
     'At stage {st} the defenders spawn {v:.0f} m behind the M-COMs (negative: in front of them, towards the '
     'attackers); DICE\'s spawn behind (median 39 m), with 2 exceptions down to -27 m.'),
    ('L1-RU-009', 'rush.attacker_spawn_depth', ('Rush', 'SR'), DICE, (-INF, 0), (-INF, -60), None, None,
     'At stage {st} the attackers spawn only {v:.0f} m in front of the M-COMs; DICE\'s spawn 64-490 m in front.'),
    ('L1-RU-010', 'rush.attacker_spawns_in_enemy_area', ('Rush', 'SR'), DICE, (-INF, 0), (-INF, -1),
     None, lambda r: r.get('stage') == 1,
     'A stage-1 attacker spawn point is {v:.0f} m inside the defenders\' combat area; DICE keeps the attackers\' '
     'first base outside it (by 2-141 m).'),
    ('L1-RU-011', 'rush.obj_margin_attacker_area', ('Rush', 'SR'), DICE, (0, INF), (10, INF), None, None,
     'A stage {st} M-COM is {v:.0f} m inside the attackers\' combat area edge; DICE\'s are 6 m or more inside '
     '(p10 18 m).'),
    ('L1-RU-012', 'rush.obj_margin_defender_area', ('Rush', 'SR'), DICE, (0, INF), (10, INF), None, None,
     'A stage {st} M-COM is {v:.0f} m inside the defenders\' combat area edge; DICE\'s are 6 m or more inside.'),
    ('L1-RU-013', 'rush.attacker_area_kept', ('Rush', 'SR'), DICE, (-INF, INF), (75, INF), None, None,
     'The attackers\' combat area at stage {st} keeps only {v:.0f}% of the stage before; DICE grows it (99.9% '
     'median; Cold War is the exception at 38%).'),
    ('L1-RU-014', 'rush.mcom_pair_dist', ('Rush',), DICE, (-INF, INF), (-INF, 10), None, None,
     'The stage {st} M-COMs differ by {v:.0f} m in height; DICE\'s pairs are within 8.5 m.'),
    ('L1-RU-015', 'rush.lives', ('Rush',), DICE, (-INF, INF), (75, 100), None, lambda r: r.get('attacker') == 'true',
     'The attackers have {v:.0f} lives; DICE\'s Rush uses 75-100 (Squad Rush 20).'),
    # ---- Squad Deathmatch
    ('L1-SD-001', 'sdm.squad_count', ('SDM',), DICE, (2, INF), (4, 4), None, None,
     'Squad Deathmatch has {v:.0f} squads; DICE\'s has 4.'),
    ('L1-SD-002', 'sdm.spawn_points', ('SDM',), DICE, (8, INF), (12, 40), None, None,
     'Squad Deathmatch has {v:.0f} spawn points; DICE\'s maps have 14-34.'),
    ('L1-SD-003', 'sdm.spawn_nn_median', ('SDM',), DICE, (-INF, INF), (10, 40), None, None,
     'Spawn points are {v:.0f} m apart (median nearest neighbour); DICE\'s: 13-38 m.'),
    ('L1-SD-004', 'sdm.combat_area_union', ('SDM',), DICE, (-INF, INF), (14000, 90000), None, None,
     'The Squad Deathmatch area is {v:.0f} m2; DICE\'s are 15,000-84,000 m2.'),
    ('L1-SD-005', 'sdm.spawn_margin_min', ('SDM',), DICE, (-5, INF), (0, INF), None, None,
     'A spawn point is {v:.0f} m inside the combat area edge (negative: outside); DICE\'s are all 2 m or more inside.'),
    ('L1-SD-006', 'sdm.vehicles', ('SDM',), DICE, (-INF, INF), (1, 1), None, None,
     'Squad Deathmatch has {v:.0f} vehicles; DICE\'s maps have one (an IFV or tank for all squads).'),
    # ---- all modes
    ('L1-AL-001', 'all.spawn_enemy_radius', None, DICE + ('WW2',), (-INF, INF), (10, 20), None, None,
     'A spawner checks for enemies within {v:.0f} m; DICE uses 20 m (10 m on one map).'),
    ('L1-AL-002', 'cq.spawn_margin', ('CQ',), DICE + ('WW2',), (0, INF), (1, INF), None, None,
     'A spawn point of {s} is {v:.0f} m inside its team\'s combat area (negative: outside); DICE: all 1.3 m or more inside.'),
    ('L1-HT-001', 'cq.flag.prominence', ('CQ',), DICE, (-INF, INF), (-10, 10), None, None,
     'Flag {s} stands {v:.0f} m above (or below) the ground around it; DICE\'s flags are within 7.5 m of it.'),
    ('L1-HT-002', 'cq.flag_pair_dist', ('CQ',), DICE, (-INF, INF), (-INF, 25), None, None,
     'Two flags differ by {v:.0f} m in height; DICE\'s differ by up to 24 m.'),
]
DY_RULES = {'L1-RU-014', 'L1-HT-002'}


def group_of(r):
    return 'ONS' if r['mode'] == 'ONS' else r['era']


def check(doc):
    recs = doc['records']
    out = []
    for rid, metric, modes, groups, warn, info, reduce, where, msg in RULES:
        rows = [r for r in recs if r['metric'] == metric and group_of(r) in groups
                and (modes is None or r['mode'] in modes) and (where is None or where(r))]
        if metric.startswith('rush.') and modes and 'SR' in modes:          # rush.* metrics are sr.* on Squad Rush
            rows += [r for r in recs if r['metric'] == metric.replace('rush.', 'sr.') and group_of(r) in groups
                     and r['mode'] == 'SR' and (where is None or where(r))]
        flagged_w, flagged_i, vals = [], [], []
        for r in rows:
            if rid == 'L1-RU-002':
                v = r['value']
                bad = [n for n in v if n != 2]
                vals.append(len(bad))
                if bad:
                    flagged_w.append((r['level'], str(v)))
                continue
            v = r.get('dy') if rid in DY_RULES else r['value']
            if v is None or not isinstance(v, (int, float)):
                continue
            vals.append(v)
            where_s = r['level'] + (f" s{r['stage']}" if 'stage' in r else '') + (f" {r['subject']}" if 'subject' in r else '')
            if warn and not (warn[0] <= v <= warn[1]):
                flagged_w.append((where_s, round(v, 1)))
            elif info and not (info[0] <= v <= info[1]):
                flagged_i.append((where_s, round(v, 1)))
        lv = len({r['level'] for r in rows})
        out.append({'id': rid, 'metric': metric, 'modes': modes, 'groups': groups, 'warn': warn, 'info': info,
                    'message': msg, 'records': len(rows), 'levels': lv, 'warn_flagged': flagged_w,
                    'info_flagged': flagged_i,
                    'range': [round(min(vals), 1), round(max(vals), 1)] if vals and rid != 'L1-RU-002' else None})
    return out


def band(b):
    if b is None:
        return '-'
    lo, hi = b
    if lo == -INF and hi == INF:
        return '-'
    if lo == -INF:
        return f'<= {hi:g}'
    if hi == INF:
        return f'>= {lo:g}'
    return f'{lo:g}-{hi:g}'


def main(argv):
    src = argv[0] if argv else os.path.join(HERE, 'l1_measurements.json')
    dst = argv[1] if len(argv) > 1 else os.path.join(HERE, 'l1_rules_check.md')
    doc = json.load(open(src, encoding='utf-8'))
    res = check(doc)
    lines = ['# L1 rule check against the stock levels (generated by l1_rules.py)', '',
             '| ID | metric | scope | warning outside | info outside | stock range | records (levels) | '
             'stock flagged: warning | stock flagged: info |', '|---|---|---|---|---|---|---|---|---|']
    for r in res:
        scope = f"{'/'.join(r['modes']) if r['modes'] else 'all'}; {'+'.join(r['groups'])}"
        w = '; '.join(f'{a} ({b})' for a, b in r['warn_flagged'][:6]) or 'none'
        i = '; '.join(f'{a} ({b})' for a, b in r['info_flagged'][:6]) or 'none'
        if len(r['warn_flagged']) > 6:
            w += f" ... {len(r['warn_flagged'])} in all"
        if len(r['info_flagged']) > 6:
            i += f" ... {len(r['info_flagged'])} in all"
        rng = f"{r['range'][0]:g}-{r['range'][1]:g}" if r['range'] else '-'
        lines.append(f"| {r['id']} | `{r['metric']}` | {scope} | {band(r['warn'])} | {band(r['info'])} | {rng} | "
                     f"{r['records']} ({r['levels']}) | {len(r['warn_flagged'])}: {w} | {len(r['info_flagged'])}: {i} |")
    with open(dst, 'w', encoding='utf-8') as fh:
        fh.write('\n'.join(lines) + '\n')
    js = [{k: (list(v) if isinstance(v, tuple) else v) for k, v in r.items()} for r in res]
    for r in js:
        for k in ('warn', 'info'):
            if r[k] is not None:
                r[k] = [None if abs(x) == INF else x for x in r[k]]
    with open(os.path.join(os.path.dirname(dst), 'l1_rules.json'), 'w', encoding='utf-8') as fh:
        json.dump(js, fh, indent=1, sort_keys=True)
    print('wrote', dst)


if __name__ == '__main__':
    main(sys.argv[1:])
