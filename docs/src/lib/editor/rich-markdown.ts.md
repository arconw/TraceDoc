# Rich Markdown Editing

Source: `src/lib/editor/rich-markdown.ts`.

CodeMirror syntax-tree integration. Decorates only visible Markdown ranges and provides parser-backed atomic Mod-B/Mod-I toggles without replacing source text.

## Decoration pipeline

- `LinkedSyntaxNode`, `MarkdownNodeRef` — minimal traversable syntax-node shapes.
- `selectionTouches(state, from, to)` — cursor/selection overlap predicate for active syntax.
- `linkLabelRange(node)` — locates visible label content inside a Markdown link.
- `buildRichMarkdownDecorations(state, visibleRanges, treeProvider)` — emits heading, emphasis, code, fence, link-content, and marker decorations; returns `Decoration.none` on parser failure.
- `RichMarkdownPlugin.constructor(view)` — builds initial visible decorations.
- `RichMarkdownPlugin.update(update)` — rebuilds on document, selection, viewport, or geometry changes.
- `richMarkdownPlugin` — exposes plugin decorations to the view.

## Formatting pipeline

- `EmphasisLayer` — parsed bold/italic content and marker ranges.
- `emphasisLayers(state, from, to)` — discovers enclosing parsed emphasis layers and actual marker text.
- `layerMatches(layer, from, to)` — matches full or semantic selection to a layer.
- `removeLayer(range, layer)` — removes only the selected parsed layer and maps selection.
- `mapPosition(position)` — maps positions through marker removal edits.
- `blockAt(state, position, side)` — resolves the nearest syntax block.
- `staysInOneInlineBlock(state, from, to)` — rejects cross-block formatting.
- `escapedAt(content, position)` — tests odd backslash escaping.
- `unparsedCursorPair(...)` — safely toggles a raw adjacent marker pair at a cursor.
- `markerBoundaryWithoutLayer(...)` — detects asymmetric parsed boundaries.
- `escapedAdjacentPair(...)` — protects escaped pseudo-markers.
- `intersectsStructuralSyntax(...)` — blocks edits through headings, lists, quotes, code, link destinations/titles/references, and escapes; cursor membership is half-open.
- `toggleMarkdownMarker(marker)` — CodeMirror `Command`; removes matching parsed layer before structural guards, otherwise applies [`markdownToggle`](markdown-editing.ts.md) to all selections in one isolated undo transaction.
- `richMarkdownEditing` — exported extension combining decoration plugin and Mod-B/Mod-I keymap.

Invariant: presentation never mutates Markdown; formatting emits one atomic transaction and preserves multi-selection direction.
