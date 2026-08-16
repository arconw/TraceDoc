# Frontend Root

Svelte application entry and shell. It binds the global project store to workspace status/view rendering and delegates feature behavior to `lib`.

- [`main.ts`](main.ts.md) — browser/Tauri webview bootstrap.
- [`App.svelte`](App.svelte.md) — product header, open/refresh commands, root shortcuts, event listener lifecycle.
- [`lib`](lib/README.md) — frontend feature and shared implementation.

Input: Tauri IPC/events. Output: mounted Svelte UI and global workspace actions.
