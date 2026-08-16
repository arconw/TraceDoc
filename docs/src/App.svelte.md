# Application Shell

Source: `src/App.svelte`.

Root UI boundary. Loads runtime identity, owns Open/Refresh toolbar actions, installs the global Mod-O shortcut and workspace-event listeners, then switches between [`WorkspaceStatus`](lib/views/WorkspaceStatus.svelte.md) and [`WorkspaceView`](lib/views/WorkspaceView.svelte.md).

## Operations

- `openFolder()` — asks the workspace view's dirty guard, then delegates native selection/open to [`projectStore`](lib/stores/project.ts.md).
- `refreshWorkspace()` — guards toolbar busy state around the store refresh command.
- `handleShortcut(event)` — on mount, maps unmodified Mod-O to `openFolder`.
- mount lifecycle — subscribes to workspace patch/error events, calls [`applyAppInfo`](lib/utils/app-identity.ts.md), and removes all listeners on destroy.

Inputs: global project state. Outputs: open/refresh actions and loaded workspace props. Invariant: workspace replacement never bypasses the editor dirty guard.
