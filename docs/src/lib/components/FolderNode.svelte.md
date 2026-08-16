# Folder Node

Source: `src/lib/components/FolderNode.svelte`.

Recursive sidebar folder renderer over [`ProjectModel`](../types/workspace.ts.md).

## Contract

- Props: project, `folderId`, depth, expanded-ID set, selected document, toggle/select callbacks.
- Reactive `folder` resolves the normalized record; `expanded` derives from the shared set.
- `handleKeydown(event)` maps Right to expand and Left to collapse.
- Recursion renders child folders first, then [`DocumentRow`](DocumentRow.svelte.md) entries in model order.

The component owns no expansion or selection state.
