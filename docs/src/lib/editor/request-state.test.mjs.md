# Editor Request-State Tests

Source: `src/lib/editor/request-state.test.mjs`.

## Cases

- `request` fixture models a versioned keep-mine operation.
- Rejects slow success and error paths after document switch.
- Keeps local `A` dirty after disk becomes `B` and Keep mine is selected.
- Keeps an edit-back-to-old-baseline dirty after an in-flight save conflict.
- Closes an externally deleted dirty buffer without disk access.

Target: [`request-state.ts`](request-state.ts.md).
