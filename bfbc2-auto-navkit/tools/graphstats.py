import collections
import os
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools')
import hpdtool

for path in sys.argv[1:]:
    f = hpdtool.parse(open(path, 'rb').read())
    parent: dict[int, int] = {}
    def find(x, parent=parent):
        while parent.setdefault(x, x) != x:
            parent[x] = parent[parent[x]]; x = parent[x]
        return x
    deg: collections.Counter[int] = collections.Counter(); ids = set()
    for s in f.sectors:
        for e in s.edges:
            a, b = s.nodes[e.src].id, s.nodes[e.dst].id
            deg[a] += 1; parent[find(a)] = find(b)
        for n in s.nodes: ids.add(n.id); find(n.id)
    comps = collections.Counter(find(i) for i in ids)
    sizes = sorted(comps.values(), reverse=True)
    print(path.split('/')[-3] if 'Output' in path else path, f'nodes {len(ids)} isolated {sum(1 for i in ids if deg[i]==0)} '
          f'components {len(sizes)} largest {sizes[0]} ({sizes[0]/len(ids)*100:.1f}%) next {sizes[1:6]}')
