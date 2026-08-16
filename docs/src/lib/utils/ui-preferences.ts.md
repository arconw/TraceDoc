# UI Preferences

Source: `src/lib/utils/ui-preferences.ts`.

Validated disposable localStorage preferences.

## API

- `WorkspaceView` — `editor | map`.
- `UiPreferences` / `defaultUiPreferences` — active view and 256px sidebar.
- `sidebarMaximumWidth(availableWidth)` — `min(384, max(192, 38%))`.
- `clampSidebarWidth(width,availableWidth)` — finite effective bounds.
- `adjustSidebarWidth(width,adjustment,availableWidth)` — 16px step or min/max action.
- `parseUiPreferences(value)` — safe JSON validation/defaulting.
- `loadUiPreferences()` / `saveUiPreferences(preferences)` — failure-tolerant localStorage boundary.

No workspace metadata is persisted.
