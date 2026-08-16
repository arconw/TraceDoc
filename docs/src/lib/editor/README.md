# Editor Logic

Pure CodeMirror/Markdown behavior and race guards used by `MarkdownEditor.svelte`.

- [`markdown-editing.ts`](markdown-editing.ts.md) / [`tests`](markdown-editing.test.mjs.md) — marker toggle edit calculation.
- [`request-state.ts`](request-state.ts.md) / [`tests`](request-state.test.mjs.md) — async read/conflict guards.
- [`rich-markdown.ts`](rich-markdown.ts.md) / [`tests`](rich-markdown.test.mjs.md) — visible-range syntax decorations and parser-backed formatting commands.

This directory does not own workspace persistence; it receives content/tokens through the editor component.
