# Project Store Tests

Source: `src/lib/stores/project.test.mjs`.

## Fixtures

- `loadProjectStoreModule()` — transpiles `project.ts` and `project-state.ts` with esbuild to temporary sibling `.mjs` files (rewriting the relative import between them) so the real source runs under `node --test` with normal module resolution, then removes the generated files.
- `snapshotFixture(rootPath)` — minimal `WorkspaceSnapshot` with one folder and one document.

## Cases

- `openFolder()` invokes `open_workspace` with the selected path and moves the store to `loaded` with the returned project.
- Cancelling the picker (`open` resolves `null`) leaves an already-loaded workspace exactly as it was and never calls `invoke` again.
- A dialog failure (`open` rejects) surfaces `{ status: 'error', message }` with the thrown message.
- A backend scan failure (`invoke` rejects) surfaces `{ status: 'error', message }` instead of leaving the store stuck in `loading`.

Target: [`project.ts`](project.ts.md); dependencies are injected per [`ProjectStoreDependencies`](project.ts.md).
