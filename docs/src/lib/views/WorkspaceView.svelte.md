# Workspace View

Source: `src/lib/views/WorkspaceView.svelte`.

Loaded workspace shell. Composes [`Sidebar`](../components/Sidebar.svelte.md), [`MarkdownEditor`](../components/MarkdownEditor.svelte.md), lazy [`MapView`](MapView.svelte.md), and [`LinkInspector`](../components/LinkInspector.svelte.md). Owns view/sidebar preferences and the one dirty-navigation modal.

## Inputs and state

- Props: project, generation/revision, selected ID, external-change versions, watcher error.
- `PendingAction` — document navigation or workspace replacement.
- Local state: editor/map handles, lazy import state, modal lifecycle, active view, responsive sidebar width/resize state.

## Operations

- `handleShortcut(event)` — Mod-1 editor, Mod-2 map, Mod-0 map+fit, Mod-F editor search; suppressed by modal.
- `isDirty()` — delegates to editor handle.
- `prepareWorkspaceChange()` — resolves immediately when clean or opens Save/Discard/Cancel and returns a promise.
- `requestDocument(documentId, openEditor)` — validates ID, applies same-selection view behavior, or opens dirty guard.
- `showView(view)` — switches/persists; initializes lazy map when required.
- `initializeMap()` / `startMapLoad()` — one-shot dynamic import with request-ID gating.
- `retryMapModule()` — retries only the active failed map state.
- `showMapAndFit()` — activates map, waits for mount, then calls fit.
- `persistPreferences()` — writes active view and effective width.
- `updateWorkspaceWidth(width)` — recalculates responsive max and clamps width.
- `startSidebarResize(event)`, `resizeSidebar(event)`, `finishSidebarResize()`, `stopSidebarResize()` — pointer capture/cancel-safe resize lifecycle.
- `resizeSidebarWithKeyboard(event)` — Arrow/Home/End adjustments through shared policy.
- `openDecision(action)` — records action/focus, makes background inert, opens native modal.
- `saveAndContinue()` — delegates editor save; retains modal on error.
- `discardAndContinue()` — accepts pending navigation without save.
- `cancelDecision()` / `handleDialogCancel(event)` — reject pending action.
- `finishDecision(proceed)` — closes modal, executes selected document/workspace action, restores focus.

## Invariants

- Every document/workspace change uses the same dirty guard.
- Map component stays mounted after first load; hidden state cancels/defer layouts without losing module/viewport.
- Selection mutation goes only through [`projectStore`](../stores/project.ts.md).
