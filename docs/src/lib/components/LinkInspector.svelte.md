# Link Inspector

Source: `src/lib/components/LinkInspector.svelte`.

Read-only debug view of indexed metadata for the selected document.

## Derived state

- `selectedDocument` — active model record.
- `outgoingLinks` — links whose source is active.
- `resolvedLinks` / `unresolvedLinks` — diagnostic partition.
- `targetLabel(targetDocumentId)` — resolves a target ID to path, falling back to the ID or `Unknown target`.

Displays title, headings, raw targets, resolved paths, and unresolved reasons from [`ProjectModel`](../types/workspace.ts.md). It performs no parsing or mutation.
