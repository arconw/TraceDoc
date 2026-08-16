# Frontend Library

Frontend implementation boundaries. Canonical cross-feature APIs are indexed in [`../../SHARED.md`](../../SHARED.md).

- [`components`](components/README.md) — concrete Svelte UI nodes/panes.
- [`editor`](editor/README.md) — pure editor request/Markdown behavior.
- [`map`](map/README.md) — graph projection, ELK layout, routing, view state, viewport state.
- [`stores`](stores/README.md) — authoritative frontend workspace state.
- [`styles`](styles/README.md) — global semantic theme.
- [`types`](types/README.md) — serialized frontend contracts.
- [`utils`](utils/README.md) — reusable UI policies/preferences/identity.
- [`views`](views/README.md) — workspace and map orchestration.

Dependency direction: types/utils → feature logic → components/views; stores own IPC-facing state and must remain the only ProjectModel authority.
