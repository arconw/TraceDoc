# UI Preference Tests

Source: `src/lib/utils/ui-preferences.test.mjs`.

Cases:

- Missing/malformed data uses defaults.
- Active view validation and width clamping.
- One responsive width drives stored state, rendering, keyboard adjustment, and ARIA at 720px.

Target: [`ui-preferences.ts`](ui-preferences.ts.md).
