# Theme Contract

Source: `src/lib/styles/theme.css`.

Global dark semantic token set and document defaults.

## Token groups

- Surfaces: background, base/raised/hover, border/strong.
- Text: foreground, subtle, muted, dim.
- States: focus, selection, error, warning.
- Map: folder/document/edge/active-edge colors.
- Scale: spaces, radii, font sizes, system and monospace families.
- Layout: compact application header height.

## Global rules

- `color-scheme: dark`; border-box sizing.
- `html`, `body`, `#app` fill the webview and prevent outer scrolling.
- Controls inherit font/color.
- `body` uses semantic background/foreground and system stack.
- `body.resizing-sidebar` forces `col-resize` cursor and disables selection during sidebar drag.
- `@media (forced-colors: active)` pins `:focus-visible` outline color to `Highlight`.

Consumers: every Svelte component. Contrast is gated by [`theme-contrast.test`](theme-contrast.test.mjs.md).
