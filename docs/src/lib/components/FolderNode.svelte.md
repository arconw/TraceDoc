# Folder Node

Source: `src/lib/components/FolderNode.svelte`.

Recursive sidebar folder renderer over [`ProjectModel`](../types/workspace.ts.md).

## Contract

- Props (`$props()`, runes mode): project, `folderId`, depth, expanded-ID set, selected document, toggle/select callbacks.
- `folder`/`expanded` are `$derived` from the props. Reads the shared `expandedFolderIds` `SvelteSet` (owned by [`Sidebar`](Sidebar.svelte.md)) through `.has()`; a legacy `$:` reactive statement does not observe mutations (`.add`/`.delete`) made to a `SvelteSet` prop from a sibling/parent component, so this must stay a rune, not a reactive statement.
- Recurses via a self-import (`import FolderNode from './FolderNode.svelte'`), not the deprecated `<svelte:self>`.
- `handleKeydown(event)` maps Right to expand and Left to collapse.
- Recursion renders child folders first, then [`DocumentRow`](DocumentRow.svelte.md) entries in model order.

The component owns no expansion or selection state.
