The outliner lists everything in the map by kind (gameplay, objects, vehicles, AI, areas) as a searchable tree.

- Rows are `row` tall with `line` hairlines; a group row shows its count right-aligned in `value` figures.
- A team-owned entry starts with its team square and ends with its team label (`ink-muted`).
- The selected row: `accent-subtle` ground with a 2px `accent` marker on its left edge. Selecting a row selects the
  object in the 3D view and the other way round.
- Hover: `surface-2`. Hidden entries keep their row with their name in `ink-faint`.

The consumer provides the tree (kinds, entries, counts), names as the user would say them ("Flag C - Village"), and
the selection; search, hide and isolate act on it.
