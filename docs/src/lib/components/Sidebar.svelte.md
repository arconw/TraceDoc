# Sidebar

Source: `src/lib/components/Sidebar.svelte`.

Project explorer and local folder-expansion owner.

## Contract

- Props: `project`, `selectedDocumentId`, `onSelectDocument`.
- `rootFolder` selects the model folder whose `parentId` is null.
- Workspace-root change clears expansion and opens only the new root.
- `toggleFolder(folderId)` mutates the reactive expanded-ID set.
- [`FolderNode`](FolderNode.svelte.md) recursively renders the model.

Invariant: expansion is presentation state; document selection remains external. Empty project/tree states remain explicit.
