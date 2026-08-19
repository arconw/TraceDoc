# Markdown Editor

Source: `src/lib/components/MarkdownEditor.svelte`.

Single-buffer CodeMirror 6 editor. Owns document read/save lifecycle, dirty state, line-ending fidelity, external-change conflicts, search, and accessible status UI. Uses [`request guards`](../editor/request-state.ts.md), [`rich Markdown`](../editor/rich-markdown.ts.md), and the shared [`projectStore`](../stores/project.ts.md).

## Inputs and state

- Props: document/selection, external change version, workspace generation/revision, save-shortcut enablement.
- State: active/requested document, saved content/token, dirty flag, request version, status/message/warning, modified/deleted conflict.
- IPC: `read_document`, `write_document`, `acknowledge_save_conflict` from [backend commands](../../../src-tauri/src/commands/document.rs.md).

## Operations

- `isDirty()` — exposes current buffer divergence.
- `getSaveError()` — exposes current save/load message to the navigation dialog.
- `find()` — focuses CodeMirror and opens search for a loaded document.
- `save()` — validates readiness/conflict, sends optimistic token+revision save, and applies returned patches. Staleness is decided by [`writeResultIsStale`](../editor/request-state.ts.md) against the revision snapshot captured when the request started, never against the live `workspaceRevision` prop, which can advance mid-flight over the separate `workspace-patch` event channel for reasons unrelated to this save.
- `createEditorState(content, editable)` — builds the complete CodeMirror extension/keymap/read-only state with original line separator.
- `loadDocument(target, targetId, discardLocal)` — version/generation-gated read; resets or activates the canonical buffer.
- `retryLoad()` — repeats the current failed read.
- `handleExternalChange()` — reloads clean content or raises modified/deleted conflict without replacing a dirty buffer.
- `keepLocalChanges()` — acknowledges recovery, reads the new disk baseline, and keeps local text dirty against it.
- `reloadExternal()` — acknowledges and reloads disk content; deleted documents close locally without reading a missing path.
- `errorMessage(error)` — normalizes unknown errors.
- `detectLineSeparator(content)` — selects CRLF/CR/LF for CodeMirror state.
- `handleBeforeUnload(event)` — warns only for dirty content.
- `handleWindowSave(event)` — applies shared Mod-S policy; blocks Shift-S/modal saves.

## Invariants

- One CodeMirror buffer is authoritative for the active document.
- Async results must match request version, document ID, workspace generation, and monotonic revision — compared against the value captured when the request started, not the live reactive value, which can change while the request is in flight.
- Failed/conflicted saves never discard text.
- Store index patches are applied even if selection changes after the save request.
