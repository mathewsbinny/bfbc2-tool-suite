The problem list shows live validation: what is wrong, why it matters, and a way to fix or find it.

- Each problem: its severity (icon and word in `status-error`, `status-warn` or `status-info`), one bold sentence
  saying what is wrong, one muted sentence saying why it matters, then actions.
- **Show** (ghost) jumps to the object in the 3D view and selects it. A **quick fix** (secondary) appears only where
  the fix is safe and undoable ("Snap to ground", "Add spawn").
- Errors stop a build; warnings do not. Errors sort first, then warnings, then information.
- No severity is shown by colour alone: the icon shape and the word always come with it.

The consumer provides the severity, the two sentences in the suite's voice (README: Voice), the object to show and
the optional fix.
