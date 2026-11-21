"""L1 helper: print instances of the given short types from a level partition (exploration only; output not kept)."""
import sys

from l1_read import dbx, partition


def show(e, depth=0, out=None, maxd=6):
    name = e.attr('name') or e.tag
    if e.children is not None:
        extra = ' '.join(f'{k}={v}' for k, v in e.attributes if k not in ('name',))
        print('  ' * depth + f'{name} [{extra}]')
        if depth < maxd:
            for c in e.children:
                show(c, depth + 1)
    else:
        try:
            v = dbx.value(e)
        except Exception as x:  # noqa: BLE001
            v = f'<{x}>'
        ref = e.attr('ref')
        print('  ' * depth + f'{name} = {v}' + (f' ref={ref}' if ref else ''))


if __name__ == '__main__':
    lv, types = sys.argv[1], sys.argv[2].split(',')
    n = int(sys.argv[3]) if len(sys.argv) > 3 else 1
    _, _, f = partition(lv)
    for t in types:
        k = 0
        for e in f.instances:
            if (e.attr('type') or '').split('.')[-1] == t:
                show(e)
                k += 1
                if k >= n:
                    break
