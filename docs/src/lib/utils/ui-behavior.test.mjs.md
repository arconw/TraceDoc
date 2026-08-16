# UI Behavior Tests

Source: `src/lib/utils/ui-behavior.test.mjs`.

## Fixtures

- `initialize()` / `render()` — retained lazy-map lifecycle harness.
- `base` — canonical shortcut modifiers.

## Cases

- Hidden delayed lazy-view resolve/reject.
- One retained map instance/layout/viewport across view switches.
- No pending/error map mount while editor is active.
- Stale lazy completion rejection.
- Modified/modal/Shift save shortcut blocking.
- Instant fit under reduced motion.

Target: [`ui-behavior.ts`](ui-behavior.ts.md).
