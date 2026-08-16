# Document Row

Source: `src/lib/components/DocumentRow.svelte`.

Leaf sidebar row for one [`Document`](../types/workspace.ts.md).

## Contract

- Props: `document`, nesting `depth`, `selected`, `onSelect(documentId)`.
- Click callback emits only the stable document ID.
- `aria-current=page` identifies the active document.
- Indentation is derived from tree depth; title exposes the workspace-relative path.

Rendered by [`FolderNode`](FolderNode.svelte.md); selection authority remains in [`projectStore`](../stores/project.ts.md).
