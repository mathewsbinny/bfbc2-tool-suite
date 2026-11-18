The property grid edits the selection's fields, typed from the game's schema, in two columns: label and value.

- Labels in `ink-muted`, in plain words ("Capture radius"); the engine's type name sits in the header strip in
  `small` for those who want it.
- Values in `value` (mono, tabular figures) inside `surface-2` inputs with a `line-strong` border; vectors split into
  X, Y and Z inputs whose letters take `axis-x`, `axis-y`, `axis-z`. Units are shown (`m`, `s`, `deg`).
- Team fields show a team badge; references are `accent` links that select what they point to.
- Several selected objects: fields they share show one value; fields that differ show "Mixed" in `ink-faint` until
  edited.

The consumer provides the fields (name, type, value, unit) and applies each edit as one undoable operation.
