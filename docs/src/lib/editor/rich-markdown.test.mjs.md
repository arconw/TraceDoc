# Rich Markdown Tests

Source: `src/lib/editor/rich-markdown.test.mjs`.

Runs the production module with real CodeMirror `EditorState`, CommonMark parser, decorations, history, and commands.

## Helpers

- `state(content, selection)` — creates Markdown EditorState.
- `decorations(editorState, visibleRanges, treeProvider)` — materializes decoration records.
- `hasClass(records, className, text, content)` — asserts a class over exact source text.
- `commandTarget(initialState)` — executes real state commands/transactions without a DOM view.

## Cases

- ATX/Setext headings, bold, italic, nested markup, inline/fenced code.
- Ordinary links, CommonMark URL/email autolinks, bracketed/empty destinations, image exclusions.
- Cursor-aware marker activation; malformed-source and parser-failure fallback.
- Incremental edits and >100k visible-range performance/source fidelity.
- Atomic undo/redo, paired cursors, reversed/multiple selections.
- Involutive star/underscore toggles and nested emphasis.
- Escaped pseudo-markers, structural/block/link/code guards, half-open cursor boundaries.
- Removal of matching outer emphasis around protected inline structures.

Targets: [`rich-markdown.ts`](rich-markdown.ts.md), [`markdown-editing.ts`](markdown-editing.ts.md).
