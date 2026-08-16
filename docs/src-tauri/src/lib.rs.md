# Tauri Runtime Builder

Source: `src-tauri/src/lib.rs`.

- `run()` — builds Tauri, registers native dialog plugin, manages [`WorkspaceSession`](services/document.rs.md) and [`WorkspaceWatcher`](services/watcher.rs.md), registers all commands, and starts the event loop.
- Modules: private [`commands`](commands/README.md), public [`models`](models/README.md), public [`services`](services/README.md).

Invariant: singleton managed session/watcher state is shared by every command.
