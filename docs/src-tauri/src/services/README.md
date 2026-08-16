# Backend Services

Filesystem and indexing implementation.

- [`workspace.rs`](workspace.rs.md) — deterministic full scan and ignore/path policy.
- [`markdown.rs`](markdown.rs.md) — heading/link parsing and resolution.
- [`document.rs`](document.rs.md) — authoritative session, optimistic reads/saves, recovery, revision history.
- [`watcher.rs`](watcher.rs.md) — native events, debounce, incremental reconciliation, patches.
- [`mod.rs`](mod.rs.md) — module exports.

Flow: scan → index → session snapshot; watcher/save → revisioned patch → frontend store.
