# App Identity Helper

Source: `src/lib/utils/app-identity.ts`.

- `applyAppInfo(request, applyName, applyTitle)` — awaits [`AppInfo`](../types/app.ts.md), applies the runtime product name, then sets title to `name version`.

Dependency injection keeps Tauri invocation and DOM mutation outside the helper. Consumer: [`App.svelte`](../../App.svelte.md).
