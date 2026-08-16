# Frontend Bootstrap

Source: `src/main.ts`.

Mounts [`App.svelte`](App.svelte.md) into `#app` after loading Svelte Flow CSS and the global [`theme`](lib/styles/theme.css.md).

## Operations

- `target` — resolves the required DOM mount node; missing target is fatal.
- `mount(App, { target })` — creates the single Svelte application instance.
- default export `app` — exposes the mounted instance to tooling.

Side effect: initializes the complete frontend once per webview.
