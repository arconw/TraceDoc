# Workspace Commands

Source: `src-tauri/src/commands/workspace.rs`.

- `open_workspace(app,session,watcher,rootPath)` — blocking scan/canonicalization, session activation, generation-bound native watcher start, then returns the activated session's snapshot directly (no redundant second scan).
- `refresh_workspace(session,generation)` — blocking full rescan through the active session with generation/revision checks.

Services: [`workspace scan`](../services/workspace.rs.md), [`document session`](../services/document.rs.md), [`watcher`](../services/watcher.rs.md). Frontend caller: [`projectStore`](../../../src/lib/stores/project.ts.md).
