The toolbar is the window's top bar: workspace switcher on the left, context in the middle, the build actions on the right.

- **Workspace switcher**: a segmented control (Objects, Gameplay, AI; later Terrain). The active segment has an
  `accent-subtle` ground with an `accent` underline.
- **Context**: the mode layout select and the content profile badge.
- **Actions**: Build (primary), Install, Play-test, right-aligned.

Height `toolbar`, ground `surface-2`, a `line` hairline below; groups are split by a vertical `line` separator,
controls spaced `space-2`. A panel's own toolbar uses the same parts at `panel-header` height without the
segmented control.
