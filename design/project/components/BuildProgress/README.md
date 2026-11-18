Build progress shows the apply step graph while it runs in the background: each step, its state and its time.

- States: done (tick, `status-ok`), running (`status-info` dot with a progress bar), waiting (hollow circle,
  `ink-muted`), failed (octagon, `status-error`, with the step's first error and a jump to the Problems list).
- Steps that run at the same time are said to: "(in parallel)", or shown side by side in a lane.
- Times in `value` figures; the header shows the elapsed total.
- The window stays usable while a build runs; Cancel is a secondary button in the header.

The consumer provides the steps (name, state, progress 0-100 or none, time) as the build graph reports them.
