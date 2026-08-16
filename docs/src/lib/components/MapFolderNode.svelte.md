# Map Folder Node

Source: `src/lib/components/MapFolderNode.svelte`.

Read-only compound-map container. Receives folder [`MapNodeData`](../map/elk-layout.ts.md), renders label/path, and disables pointer interception so routed edges inside nested folders remain interactive.

No methods or local state. Placement and dimensions come from the ELK layout; hierarchy comes from Svelte Flow `parentId`.
