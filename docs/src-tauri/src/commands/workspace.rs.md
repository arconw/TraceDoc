# Workspace Commands

Source: `src-tauri/src/commands/workspace.rs`.

- `open_workspace(app,session,watcher,rootPath)` — resolves and arms the native watcher on the canonical root *before* scanning, so changes made during the scan are queued instead of missed; blocking scan, session activation, then folds any changes queued during the scan into the session and starts live delivery, then returns the activated session's snapshot directly (no redundant second full scan).
- `refresh_workspace(session,generation)` — blocking full rescan through the active session with generation/revision checks.

Services: [`workspace scan`](../services/workspace.rs.md), [`document session`](../services/document.rs.md), [`watcher`](../services/watcher.rs.md). Frontend caller: [`projectStore`](../../../src/lib/stores/project.ts.md).
