# Markdown Marker Edit Tests

Source: `src/lib/editor/markdown-editing.test.mjs`.

Loads the TypeScript module through Vite and validates the pure edit contract.

## Helpers

- `applyChanges(content, changes)` — applies ordered edit specs to source text.

## Cases

- Forward and backward selection wrapping.
- Removing surrounding, selected bold, and selected italic markers.
- Preserving selection direction after removal.
- Empty-cursor paired insertion.
- Rejecting asymmetric/whitespace-delimited emphasis.
- Nesting italic around strong text without crossing markers.

Target: [`markdown-editing.ts`](markdown-editing.ts.md).
