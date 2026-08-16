# Rust Models

Canonical serialized workspace contracts.

- [`workspace.rs`](workspace.rs.md) — folder/document/link graph, snapshots, patches, read/save/error DTOs.
- [`mod.rs`](mod.rs.md) — module export.

All maps use deterministic `BTreeMap`; serde emits frontend-compatible camelCase fields.
