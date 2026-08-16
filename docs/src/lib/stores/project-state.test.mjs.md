# Project State Tests

Source: `src/lib/stores/project-state.test.mjs`.

## Fixtures

- `firstDocument`, `secondDocument`, `loadedState(generation)` — canonical state records.
- `update`, `watcherPatch`, `savePatch`, `snapshot`, `patch`, `deleted` — save/watcher/open/delete payloads.

## Cases

- Save completion after selection change; rejection across workspace generations.
- Single live patch and old-generation patch rejection.
- Older save after newer watcher result; save catch-up of delayed unrelated patch.
- Old-generation and older same-generation error rejection.
- Old refresh after workspace switch.
- Patch buffering/replay while opening.
- Closing selection after external deletion.

Target: [`project-state.ts`](project-state.ts.md).
