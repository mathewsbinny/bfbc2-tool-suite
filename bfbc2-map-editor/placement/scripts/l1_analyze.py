"""L1: distributions of the measurements per mode and era group, with the outliers named.

    python l1_analyze.py [l1_measurements.json] [out.md]

Writes l1_distributions.md (tables the report quotes) and prints rule-check counts (how many levels obey each
proposed rule). Groups: BFBC2 (DICE's BFBC2 maps), BFBC1 (the BC1 remakes), Vietnam, and 'DICE' = the three
together (the BFBC2-engine content DICE shipped for PC); WW2 (OPFOR's BF1943 ports) and ONS (Onslaught console
ports) are kept apart.
"""
import json
import os
import sys
from collections import defaultdict

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
DICE = ('BFBC2', 'BFBC1', 'Vietnam')


def load(path):
    with open(path, encoding='utf-8') as fh:
        return json.load(fh)


def group_of(r):
    if r['mode'] == 'ONS':
        return 'ONS'
    return r['era']


def pct(a, q):
    return float(np.percentile(a, q)) if len(a) else None


def fmt(v, unit=''):
    if v is None:
        return '-'
    if unit == 'm2':
        return f'{v / 1000:.0f}k'
    if abs(v) >= 100:
        return f'{v:.0f}'
    return f'{v:.1f}'


class Data:
    def __init__(self, doc):
        self.recs = doc['records']

    def values(self, metric, groups, mode=None, where=None, reduce=None):
        """[(value, level, extra)] of a metric; reduce: per level 'min' / 'max' / 'median' / 'sum' / 'count'."""
        rows = [r for r in self.recs if r['metric'] == metric and (mode is None or r['mode'] in mode)
                and (group_of(r) in groups) and isinstance(r['value'], (int, float))
                and (where is None or where(r))]
        if reduce is None:
            return [(r['value'], r['level'] + (f" s{r['stage']}" if 'stage' in r else ''), r) for r in rows]
        by = defaultdict(list)
        for r in rows:
            by[r['level']].append(r['value'])
        f = {'min': min, 'max': max, 'median': lambda v: float(np.median(v)), 'sum': sum, 'count': len,
             'mean': lambda v: float(np.mean(v))}[reduce]
        return [(f(v), lv, None) for lv, v in by.items()]


def row(label, vals, unit=''):
    if not vals:
        return f'| {label} | 0 | | | | | | |'
    v = np.array([x[0] for x in vals], dtype=float)
    lo = min(vals, key=lambda x: x[0])
    hi = max(vals, key=lambda x: x[0])
    nlv = len({x[1].split(' ')[0] for x in vals})
    return (f'| {label} | {len(v)} ({nlv} lv) | {fmt(lo[0], unit)} ({lo[1]}) | {fmt(pct(v, 10), unit)} | '
            f'{fmt(pct(v, 50), unit)} | {fmt(pct(v, 90), unit)} | {fmt(hi[0], unit)} ({hi[1]}) |')


HEAD = '| metric | n | min (where) | p10 | median | p90 | max (where) |\n|---|---|---|---|---|---|---|'

SPECS = {
    'Conquest': [
        ('flags per level', 'cq.flag_count', None, None, ''),
        ('flag pair distance, m', 'cq.flag_pair_dist', None, None, 'm'),
        ('flag nearest-neighbour distance, m', 'cq.flag_nn_dist', None, None, 'm'),
        ('closest flag pair per level, m', 'cq.flag_nn_dist', None, 'min', 'm'),
        ('flag pair height difference, m', 'cq.flag_pair_dist', None, None, 'dy'),
        ('HQ to nearest flag, m', 'cq.hq_nearest_flag', None, None, 'm'),
        ('HQ to farthest flag, m', 'cq.hq_farthest_flag', None, None, 'm'),
        ('HQ to HQ, m', 'cq.hq_hq_dist', None, None, 'm'),
        ('HQ nearest-flag asymmetry, m', 'cq.hq_nearest_flag_asymmetry', None, None, 'm'),
        ('flag position along HQ axis (0 = Team1 HQ)', 'cq.flag_axis_t', None, None, ''),
        ('flag lateral offset / HQ distance', 'cq.flag_axis_lateral', None, None, ''),
        ('Index order vs axis order (Spearman)', 'cq.flag_index_axis_rho', None, None, ''),
        ('letter order vs axis order (Spearman)', 'cq.flag_letter_axis_rho', None, None, ''),
        ('layout thinness (0 = line)', 'cq.layout_thinness', None, None, ''),
        ('capture zone, equivalent radius m', 'cq.capture_zone', None, None, 'm'),
        ('  - sphere radius, m', 'cq.capture_zone', lambda r: r['zone']['kind'] == 'sphere', None, 'm'),
        ('  - outline, equivalent radius m', 'cq.capture_zone', lambda r: r['zone']['kind'].startswith('polygon'), None, 'm'),
        ('  - AABB (half-extents), equivalent radius m', 'cq.capture_zone', lambda r: r['zone']['kind'].startswith('aabb'), None, 'm'),
        ('  - CaptureRadius only, m', 'cq.capture_zone', lambda r: r['zone']['kind'] == 'radius', None, 'm'),
        ('  - capture zone reach from the flag, m', 'cq.capture_zone_reach', None, None, 'm'),
        ('time to capture, s', 'cq.time_to_capture', None, None, ''),
        ('flag spawn points', 'cq.flag_spawn_points', None, None, ''),
        ('flag spawn distance, median per flag, m', 'cq.flag_spawn_dist_median', None, None, 'm'),
        ('flag spawn distance, nearest per flag, m', 'cq.flag_spawn_dist_min', None, None, 'm'),
        ('flag spawn distance, farthest per flag, m', 'cq.flag_spawn_dist_max', None, None, 'm'),
        ('HQ spawn points', 'cq.hq_spawn_points', None, None, ''),
        ('HQ spawn distance, median, m', 'cq.hq_spawn_dist_median', None, None, 'm'),
        ('HQ spawn distance, farthest, m', 'cq.hq_spawn_dist_max', None, None, 'm'),
        ('HQ vehicles (one side)', 'cq.hq_vehicles', None, None, ''),
        ('flag vehicles (both sides\' entries)', 'cq.flag_vehicles', None, None, ''),
        ('tickets per team', 'cq.tickets', None, None, ''),
        ('combat area (union), m2', 'cq.combat_area_union', None, None, 'm2'),
        ('flag margin inside combat area, m', 'cq.flag_margin', None, None, 'm'),
        ('smallest flag margin per level, m', 'cq.flag_margin', None, 'min', 'm'),
        ('HQ margin inside own area, m', 'cq.hq_margin', None, None, 'm'),
        ('spawn point margin, smallest per level, m', 'cq.spawn_margin', None, 'min', 'm'),
        ('HQ vs enemy area (per-team areas; - = outside)', 'cq.hq_vs_enemy_area', None, None, 'm'),
        ('HQ spawns vs enemy area, max (- = all outside)', 'cq.hq_spawns_vs_enemy_area', None, None, 'm'),
        ('flag ground percentile in area, %', 'cq.flag.ground_percentile', None, None, ''),
        ('HQ ground percentile in area, %', 'cq.hq.ground_percentile', None, None, ''),
        ('flag height above ground, m', 'cq.flag.above_ground', None, None, 'm'),
        ('flag prominence (ground - median ground 30-100 m), m', 'cq.flag.prominence', None, None, 'm'),
        ('HQ prominence, m', 'cq.hq.prominence', None, None, 'm'),
    ],
    'Rush': [
        ('stages', 'rush.stage_count', None, None, ''),
        ('M-COM pair distance, m', 'rush.mcom_pair_dist', None, None, 'm'),
        ('M-COM pair height difference, m', 'rush.mcom_pair_dist', None, None, 'dy'),
        ('stage step (centroid to centroid), m', 'rush.stage_step', None, None, 'm'),
        ('M-COM pair angle to attack direction, deg', 'rush.mcom_pair_angle', None, None, ''),
        ('defender spawn depth behind objectives, m', 'rush.defender_spawn_depth', None, None, 'm'),
        ('attacker spawn depth (- = in front of objectives), m', 'rush.attacker_spawn_depth', None, None, 'm'),
        ('attacker base to previous defender base, m', 'rush.attacker_base_from_prev_defender_base', None, None, 'm'),
        ('attacker spawn to objective, nearest, m', 'rush.attacker_spawn_to_obj_min', None, None, 'm'),
        ('attacker spawn to objective, median, m', 'rush.attacker_spawn_to_obj_median', None, None, 'm'),
        ('attacker spawn to objective, nearest, stage 1, m', 'rush.attacker_spawn_to_obj_min',
         lambda r: r.get('stage') == 1, None, 'm'),
        ('attacker spawn to objective, nearest, stages 2+, m', 'rush.attacker_spawn_to_obj_min',
         lambda r: r.get('stage', 1) > 1, None, 'm'),
        ('defender spawn to objective, nearest, m', 'rush.defender_spawn_to_obj_min', None, None, 'm'),
        ('defender spawn to objective, median, m', 'rush.defender_spawn_to_obj_median', None, None, 'm'),
        ('attacker area, m2', 'rush.attacker_area', None, None, 'm2'),
        ('defender area, m2', 'rush.defender_area', None, None, 'm2'),
        ('attacker area kept from previous stage, %', 'rush.attacker_area_kept', None, None, ''),
        ('defender area kept from previous stage, %', 'rush.defender_area_kept', None, None, ''),
        ('objective margin inside attacker area, m', 'rush.obj_margin_attacker_area', None, None, 'm'),
        ('objective margin inside defender area, m', 'rush.obj_margin_defender_area', None, None, 'm'),
        ('attacker spawns in defender area (max; - = outside)', 'rush.attacker_spawns_in_enemy_area', None, None, 'm'),
        ('attacker spawns in defender area, stage 1', 'rush.attacker_spawns_in_enemy_area',
         lambda r: r.get('stage') == 1, None, 'm'),
        ('defender spawns in attacker area (max; - = outside)', 'rush.defender_spawns_in_enemy_area', None, None, 'm'),
        ('attacker vehicles per stage', 'rush.attacker_vehicles', None, None, ''),
        ('defender vehicles per stage', 'rush.defender_vehicles', None, None, ''),
        ('attacker lives (tickets)', 'rush.lives', lambda r: r.get('attacker') == 'true', None, ''),
        ('objective ground percentile, %', 'rush.obj.ground_percentile', None, None, ''),
        ('objective height above ground, m', 'rush.obj.above_ground', None, None, 'm'),
        ('objective prominence, m', 'rush.obj.prominence', None, None, 'm'),
        ('spawn ratio (attacker nearest / defender nearest)', 'rush.spawn_ratio', None, None, ''),
        ('defender spawn height minus objective (median)', 'rush.defender_spawn_dy_to_obj', None, None, 'm'),
        ('attacker spawn height minus objective (median)', 'rush.attacker_spawn_dy_to_obj', None, None, 'm'),
    ],
    'SDM': [
        ('squads', 'sdm.squad_count', None, None, ''),
        ('spawn points (distinct)', 'sdm.spawn_points', None, None, ''),
        ('spawn nearest-neighbour, median, m', 'sdm.spawn_nn_median', None, None, 'm'),
        ('spawn nearest-neighbour, min, m', 'sdm.spawn_nn_min', None, None, 'm'),
        ('spawn spread (largest distance), m', 'sdm.spawn_spread_max', None, None, 'm'),
        ('squad base spread, m', 'sdm.squad_base_spread', None, None, 'm'),
        ('combat area, m2', 'sdm.combat_area_union', None, None, 'm2'),
        ('spawn margin, smallest, m', 'sdm.spawn_margin_min', None, None, 'm'),
        ('spawn margin, median, m', 'sdm.spawn_margin_median', None, None, 'm'),
        ('vehicles', 'sdm.vehicles', None, None, ''),
    ],
    'ONS': [
        ('flags', 'ons.flag_count', None, None, ''),
        ('flag step, m', 'ons.flag_step', None, None, 'm'),
        ('HQ to first flag, m', 'ons.hq_to_first_flag', None, None, 'm'),
        ('capture zone, m', 'ons.capture_zone', None, None, 'm'),
        ('combat area, m2', 'ons.combat_area', None, None, 'm2'),
    ],
    'All': [
        ('soldier spawner SpawnEnemyRadius, m', 'all.spawn_enemy_radius', None, None, 'm'),
        ('vehicle respawn delay, s', 'all.vehicle_respawn_delay', None, None, ''),
    ],
}


def table(D, mode_name, specs, groups_list, metric_prefix=None):
    out = []
    for gname, groups in groups_list:
        out.append(f'\n**{gname}**\n\n{HEAD}')
        for label, metric, where, reduce, unit in specs:
            if metric_prefix:
                metric = metric.replace('rush.', metric_prefix)
            mode = None
            if unit == 'dy':
                vals = [(r[2].get('dy'), r[1], r[2]) for r in D.values(metric, groups, mode, where)
                        if r[2] is not None and r[2].get('dy') is not None]
                out.append(row(label, vals, 'm'))
                continue
            vals = D.values(metric, groups, mode, where, reduce)
            out.append(row(label, vals, unit))
    return '\n'.join(out)


def main(argv):
    src = argv[0] if argv else os.path.join(HERE, 'l1_measurements.json')
    dst = argv[1] if len(argv) > 1 else os.path.join(HERE, 'l1_distributions.md')
    D = Data(load(src))
    parts = ['# L1 distributions (generated by l1_analyze.py)\n',
             'n = records (levels); min / max name the level (and stage). Distances horizontal, metres; '
             'areas in thousands of m2 (k).\n']
    eras = [('DICE (BFBC2 + BFBC1 + Vietnam)', DICE), ('BFBC2', ('BFBC2',)), ('BFBC1', ('BFBC1',)),
            ('Vietnam', ('Vietnam',)), ('WW2 (OPFOR BF1943 ports)', ('WW2',))]
    parts.append('## Conquest\n' + table(D, 'Conquest', SPECS['Conquest'], eras))
    parts.append('\n## Rush\n' + table(D, 'Rush', SPECS['Rush'], eras[:4]))
    parts.append('\n## Squad Rush\n' + table(D, 'SR', SPECS['Rush'], eras[:4], metric_prefix='sr.'))
    parts.append('\n## Squad Deathmatch\n' + table(D, 'SDM', SPECS['SDM'], eras[:4]))
    parts.append('\n## Onslaught (console ports)\n' + table(D, 'ONS', SPECS['ONS'], [('ONS', ('ONS',))]))
    parts.append('\n## All modes\n' + table(D, 'All', SPECS['All'], eras + [('ONS', ('ONS',))]))
    with open(dst, 'w', encoding='utf-8') as fh:
        fh.write('\n'.join(parts) + '\n')
    print('wrote', dst)


if __name__ == '__main__':
    main(sys.argv[1:])
