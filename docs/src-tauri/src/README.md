# Rust Runtime Root

Tauri runtime composition.

- [`main.rs`](main.rs.md) — executable entry.
- [`lib.rs`](lib.rs.md) — builder, managed state, plugins, command registration.
- [`commands`](commands/README.md) — IPC boundary.
- [`models`](models/README.md) — serialized canonical model.
- [`services`](services/README.md) — scan, index, session/save, watcher behavior.

Commands remain thin; services own blocking work and invariants.
