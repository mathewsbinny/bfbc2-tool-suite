A button runs one action, named by its label: a verb, or a verb and its object ("Delete 3 objects").

- **Primary** (`accent` fill, `on-accent` text): the one action a panel or dialog exists for (Build, Create map).
  At most one per panel.
- **Secondary** (`surface-2`, `line-strong` border, `ink`): every other action.
- **Ghost** (no fill, `ink-muted`): tool actions in toolbars and rows (Snap to ground, Focus).
- **Danger** (secondary with `status-error` text): actions that remove or overwrite; they ask to confirm in the
  panel, not in a pop-up.
- **Disabled** (`surface-1`, `ink-faint`, `line` border): shows why in its tooltip ("Close the game first").

The consumer provides the label, an optional shortcut (`kbd` chip, shown in the tooltip too) and the action. Height
is `control`, corners `radius-md`, keyboard focus a 2px `focus` ring.
