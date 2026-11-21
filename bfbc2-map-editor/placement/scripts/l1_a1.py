"""L1: the A1 inputs - a typical layout per mode, as parameters (medians and inter-quartile ranges) - plus the
vehicle and spawner patterns the tables do not show. Prints markdown; writes l1_a1_params.json."""
import json
import os
import sys
from collections import Counter, defaultdict

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
DICE = ('BFBC2', 'BFBC1', 'Vietnam')


def grp(r):
    return 'ONS' if r['mode'] == 'ONS' else r['era']


def vals(recs, metric, groups, where=None, key='value'):
    return [r[key] for r in recs if r['metric'] == metric and grp(r) in groups and isinstance(r.get(key), (int, float))
            and (where is None or where(r))]


def q(v):
    if not v:
        return None
    a = np.array(v, dtype=float)
    return {'median': round(float(np.median(a)), 1), 'p25': round(float(np.percentile(a, 25)), 1),
            'p75': round(float(np.percentile(a, 75)), 1), 'min': round(float(a.min()), 1),
            'max': round(float(a.max()), 1), 'n': len(a)}


def main(argv):
    src = argv[0] if argv else os.path.join(HERE, 'l1_measurements.json')
    recs = json.load(open(src, encoding='utf-8'))['records']
    P = {}
    for gname, groups in (('DICE', DICE), ('WW2', ('WW2',))):
        cq = {
            'flags': q(vals(recs, 'cq.flag_count', groups)),
            'flag_nn_m': q(vals(recs, 'cq.flag_nn_dist', groups)),
            'hq_nearest_flag_m': q(vals(recs, 'cq.hq_nearest_flag', groups)),
            'hq_hq_m': q(vals(recs, 'cq.hq_hq_dist', groups)),
            'flag_axis_t': q(vals(recs, 'cq.flag_axis_t', groups)),
            'flag_lateral': q(vals(recs, 'cq.flag_axis_lateral', groups)),
            'capture_radius_m': q(vals(recs, 'cq.capture_zone', groups, lambda r: not r['zone']['kind'].startswith('aabb'))),
            'time_to_capture_s': q(vals(recs, 'cq.time_to_capture', groups)),
            'flag_spawn_points': q(vals(recs, 'cq.flag_spawn_points', groups)),
            'flag_spawn_dist_median_m': q(vals(recs, 'cq.flag_spawn_dist_median', groups)),
            'flag_spawn_dist_min_m': q(vals(recs, 'cq.flag_spawn_dist_min', groups)),
            'flag_spawn_dist_max_m': q(vals(recs, 'cq.flag_spawn_dist_max', groups)),
            'hq_spawn_points': q(vals(recs, 'cq.hq_spawn_points', groups)),
            'hq_spawn_dist_median_m': q(vals(recs, 'cq.hq_spawn_dist_median', groups)),
            'hq_spawn_dist_max_m': q(vals(recs, 'cq.hq_spawn_dist_max', groups)),
            'tickets': q(vals(recs, 'cq.tickets', groups)),
            'ticket_loss_per_min': q(vals(recs, 'cq.tickets', groups, key='loss_per_min')),
            'combat_area_m2': q(vals(recs, 'cq.combat_area_union', groups)),
            'flag_margin_m': q(vals(recs, 'cq.flag_margin', groups)),
            'hq_margin_m': q(vals(recs, 'cq.hq_margin', groups)),
            'flag_prominence_m': q(vals(recs, 'cq.flag.prominence', groups)),
            'flag_pair_dy_m': q(vals(recs, 'cq.flag_pair_dist', groups, key='dy')),
        }
        # flag spawner patterns: per-team spawners vs one neutral spawner
        pat = Counter()
        for r in recs:
            if r['metric'] == 'cq.flag_spawners' and grp(r) in groups:
                t = r.get('teams') or {}
                pat['+'.join(f'{k}x{v}' for k, v in sorted(t.items()))] += 1
        cq['flag_spawner_patterns'] = dict(pat.most_common())
        # vehicle pools: spawners whose nearest base / flag is that one, within 150 m
        pool = defaultdict(Counter)
        for r in recs:
            if r['metric'] == 'cq.vehicle_spawner' and grp(r) in groups and r['value'] <= 150:
                key = (r['level'], r['subject'], 'hq' if r.get('near_hq') else 'flag')
                for v in r.get('vehicles') or []:
                    pool[key][v.split(':')[1]] += 1
                    break
        hq = [sum(c.values()) for k, c in pool.items() if k[2] == 'hq']
        fl = [sum(c.values()) for k, c in pool.items() if k[2] == 'flag']
        hq_cls = Counter()
        fl_cls = Counter()
        for k, c in pool.items():
            (hq_cls if k[2] == 'hq' else fl_cls).update(c)
        cq['hq_vehicle_pool_150m'] = q(hq)
        cq['flag_vehicle_pool_150m'] = q(fl)
        cq['hq_vehicle_classes_total'] = dict(hq_cls.most_common())
        cq['flag_vehicle_classes_total'] = dict(fl_cls.most_common())
        # HQ pool symmetry per level
        by_lv = defaultdict(list)
        for k, c in pool.items():
            if k[2] == 'hq':
                by_lv[k[0]].append(sum(c.values()))
        cq['hq_pool_difference_per_level'] = {lv: abs(v[0] - v[1]) if len(v) == 2 else None for lv, v in by_lv.items()}
        P[f'CQ {gname}'] = cq
    for mode, pre in (('Rush', 'rush.'), ('SR', 'sr.')):
        g = DICE
        ru = {
            'stages': q(vals(recs, pre + 'stage_count', g)),
            'mcom_pair_m': q(vals(recs, pre + 'mcom_pair_dist', g)),
            'mcom_pair_angle_deg': q(vals(recs, pre + 'mcom_pair_angle', g)),
            'stage_step_m': q(vals(recs, pre + 'stage_step', g)),
            'attacker_spawn_to_obj_min_m_stage1': q(vals(recs, pre + 'attacker_spawn_to_obj_min', g, lambda r: r.get('stage') == 1)),
            'attacker_spawn_to_obj_min_m_later': q(vals(recs, pre + 'attacker_spawn_to_obj_min', g, lambda r: r.get('stage', 1) > 1)),
            'defender_spawn_to_obj_min_m': q(vals(recs, pre + 'defender_spawn_to_obj_min', g)),
            'defender_spawn_to_obj_median_m': q(vals(recs, pre + 'defender_spawn_to_obj_median', g)),
            'defender_spawn_depth_m': q(vals(recs, pre + 'defender_spawn_depth', g)),
            'attacker_spawn_depth_m': q(vals(recs, pre + 'attacker_spawn_depth', g)),
            'attacker_base_from_prev_defender_base_m': q(vals(recs, pre + 'attacker_base_from_prev_defender_base', g)),
            'attacker_area_m2_stage1': q(vals(recs, pre + 'attacker_area', g, lambda r: r.get('stage') == 1)),
            'defender_area_m2_stage1': q(vals(recs, pre + 'defender_area', g, lambda r: r.get('stage') == 1)),
            'attacker_area_new_m2': q(vals(recs, pre + 'attacker_area_new', g)),
            'obj_margin_attacker_area_m': q(vals(recs, pre + 'obj_margin_attacker_area', g)),
            'obj_margin_defender_area_m': q(vals(recs, pre + 'obj_margin_defender_area', g)),
            'stage1_attacker_spawns_outside_defender_area_m': q(vals(recs, pre + 'attacker_spawns_in_enemy_area', g, lambda r: r.get('stage') == 1)),
            'attacker_lives': q(vals(recs, pre + 'lives', g, lambda r: r.get('attacker') == 'true')),
            'defender_lives_field': q(vals(recs, pre + 'lives', g, lambda r: r.get('attacker') != 'true')),
            'attacker_vehicles_per_stage': q(vals(recs, pre + 'attacker_vehicles', g)),
            'defender_vehicles_per_stage': q(vals(recs, pre + 'defender_vehicles', g)),
            'obj_prominence_m': q(vals(recs, pre + 'obj.prominence', g)),
        }
        cls = {'attacker': Counter(), 'defender': Counter()}
        for r in recs:
            for side in cls:
                if r['metric'] == pre + f'{side}_vehicles' and grp(r) in g:
                    cls[side].update(r.get('by_class') or {})
        ru['vehicle_classes_total'] = {k: dict(v.most_common()) for k, v in cls.items()}
        P[mode] = ru
    P['SDM'] = {
        'squads': q(vals(recs, 'sdm.squad_count', DICE)),
        'spawn_points': q(vals(recs, 'sdm.spawn_points', DICE)),
        'spawn_nn_median_m': q(vals(recs, 'sdm.spawn_nn_median', DICE)),
        'spawn_spread_m': q(vals(recs, 'sdm.spawn_spread_max', DICE)),
        'combat_area_m2': q(vals(recs, 'sdm.combat_area_union', DICE)),
        'spawn_margin_median_m': q(vals(recs, 'sdm.spawn_margin_median', DICE)),
        'vehicles': q(vals(recs, 'sdm.vehicles', DICE)),
    }
    kc = Counter()
    for r in recs:
        if r['metric'] == 'sdm.kill_counter' and grp(r) in DICE:
            kc[str((r.get('fields') or {}).get('MaxKillCount'))] += 1
    P['SDM']['kill_counter_max'] = dict(kc)
    # vehicle respawn delay by class (DICE)
    dl = defaultdict(list)
    for r in recs:
        if r['metric'] == 'all.vehicle_respawn_delay' and grp(r) in DICE and isinstance(r['value'], (int, float)):
            dl[r.get('vclass')].append(r['value'])
    P['vehicle_respawn_delay_s'] = {k: q(v) for k, v in sorted(dl.items()) if len(v) >= 5}
    out = os.path.join(HERE, 'l1_a1_params.json')
    with open(out, 'w', encoding='utf-8') as fh:
        json.dump(P, fh, indent=1)
    print(json.dumps(P, indent=1))


if __name__ == '__main__':
    main(sys.argv[1:])
