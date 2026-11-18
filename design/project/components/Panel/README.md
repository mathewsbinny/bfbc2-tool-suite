A panel is one dockable area of the window: its tab, a header strip and a body.

- **Tabs** (`panel-header` tall, `label` captions in capitals): the active tab has a `surface-3` ground and an
  `accent` underline; the others `ink-muted` on `surface-1`.
- **Header strip** (`surface-3`, `panel-header` tall): a caption on the left and the panel's tools (ghost buttons)
  on the right.
- **Body** (`surface-1`, `space-3` padding, or none for lists that run edge to edge).

Docked panels sit edge to edge with `line` hairlines and no shadow; a floating panel takes `radius-lg` and
`shadow-popover`. The consumer provides the caption, the tools and the body; the docking (drag, dock, tab, float,
Reset layout) is the GUI stack's.
