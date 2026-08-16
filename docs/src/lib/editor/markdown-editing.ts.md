# Markdown Marker Edits

Source: `src/lib/editor/markdown-editing.ts`.

Pure text/range engine used by [`rich-markdown`](rich-markdown.ts.md) to add or remove Markdown emphasis markers while preserving selection direction.

## Types and functions

- `MarkdownToggleEdit` — one `{from,to,insert}` source edit.
- `MarkdownToggleResult` — atomic edits plus resulting anchor/head.
- `exactMarkerAt(content, from, marker)` — validates an exact marker; prevents a single `*` from matching inside `**`.
- `unchanged(anchor, head)` — no-op result.
- `directedRange(from, to, reversed)` — reconstructs forward/backward selection.
- `markdownToggle(content, anchor, head, marker)` — removes outside/inside balanced markers or inserts a pair; rejects asymmetric, whitespace-delimited, and multi-block selections.

No parser, DOM, or editor dependency.
