# Tauri Commands

Thin async IPC adapters. They validate/capture generation state, move blocking work off the async runtime, and serialize errors as strings.

- [`app.rs`](app.rs.md) — product identity.
- [`workspace.rs`](workspace.rs.md) — open/refresh and watcher lifecycle.
- [`document.rs`](document.rs.md) — read/save/conflict acknowledgement.
- [`mod.rs`](mod.rs.md) — module exports.

Business logic belongs in services.
