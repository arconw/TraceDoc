import { syntaxTree } from '@codemirror/language';
import { isolateHistory } from '@codemirror/commands';
import {
  EditorSelection,
  type EditorState,
  type Extension,
  type Range,
  type SelectionRange,
} from '@codemirror/state';
import {
  Decoration,
  EditorView,
  keymap,
  ViewPlugin,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view';
import { markdownToggle } from './markdown-editing';

interface LinkedSyntaxNode {
  name: string;
  from: number;
  to: number;
  firstChild: LinkedSyntaxNode | null;
  nextSibling: LinkedSyntaxNode | null;
  parent: LinkedSyntaxNode | null;
}

interface MarkdownNodeRef {
  node: { firstChild: LinkedSyntaxNode | null };
}

const headingPattern = /^(?:ATX|Setext)Heading([1-6])$/;
const markerNames = new Set([
  'HeaderMark',
  'EmphasisMark',
  'CodeMark',
  'LinkMark',
]);

function selectionTouches(state: EditorState, from: number, to: number) {
  return state.selection.ranges.some(
    (range) => range.from <= to && range.to >= from,
  );
}

function linkLabelRange(node: MarkdownNodeRef) {
  const marks: { from: number; to: number }[] = [];
  let child = node.node.firstChild;
  while (child) {
    if (child.name === 'LinkMark') marks.push(child);
    child = child.nextSibling;
  }
  return marks.length >= 2 && marks[0].to < marks[1].from
    ? { from: marks[0].to, to: marks[1].from }
    : null;
}

export function buildRichMarkdownDecorations(
  state: EditorState,
  visibleRanges: readonly { from: number; to: number }[],
  treeProvider: typeof syntaxTree = syntaxTree,
): DecorationSet {
  try {
    const ranges: Range<Decoration>[] = [];
    const tree = treeProvider(state);

    for (const visible of visibleRanges) {
      tree.iterate({
        from: visible.from,
        to: visible.to,
        enter(node) {
          const heading = headingPattern.exec(node.name);
          if (heading) {
            const level = heading[1];
            ranges.push(
              Decoration.mark({
                class: `cm-md-heading cm-md-heading-${level}`,
              }).range(node.from, node.to),
            );
            ranges.push(
              Decoration.line({
                class: `cm-md-heading-line cm-md-heading-line-${level}`,
              }).range(state.doc.lineAt(node.from).from),
            );
            return;
          }

          if (node.name === 'StrongEmphasis') {
            ranges.push(
              Decoration.mark({ class: 'cm-md-strong' }).range(
                node.from,
                node.to,
              ),
            );
          } else if (node.name === 'Emphasis') {
            ranges.push(
              Decoration.mark({ class: 'cm-md-emphasis' }).range(
                node.from,
                node.to,
              ),
            );
          } else if (node.name === 'InlineCode') {
            ranges.push(
              Decoration.mark({ class: 'cm-md-inline-code' }).range(
                node.from,
                node.to,
              ),
            );
          } else if (node.name === 'Link') {
            const label = linkLabelRange(node);
            if (label) {
              ranges.push(
                Decoration.mark({ class: 'cm-md-link' }).range(
                  label.from,
                  label.to,
                ),
              );
            }
          } else if (node.name === 'URL') {
            const parentName = node.node.parent?.name;
            if (
              parentName === 'Link' ||
              parentName === 'Autolink' ||
              parentName === 'Paragraph'
            ) {
              const bracketedDestination =
                parentName === 'Link' &&
                state.sliceDoc(node.from, node.from + 1) === '<' &&
                state.sliceDoc(node.to - 1, node.to) === '>';
              const linkFrom = bracketedDestination ? node.from + 1 : node.from;
              const linkTo = bracketedDestination ? node.to - 1 : node.to;
              if (linkFrom < linkTo) {
                ranges.push(
                  Decoration.mark({ class: 'cm-md-link' }).range(
                    linkFrom,
                    linkTo,
                  ),
                );
              }
              if (bracketedDestination) {
                const parent = node.node.parent;
                const active = selectionTouches(
                  state,
                  parent?.from ?? node.from,
                  parent?.to ?? node.to,
                );
                const syntaxClass = active
                  ? 'cm-md-syntax cm-md-syntax-active'
                  : 'cm-md-syntax';
                ranges.push(
                  Decoration.mark({ class: syntaxClass }).range(
                    node.from,
                    node.from + 1,
                  ),
                  Decoration.mark({ class: syntaxClass }).range(
                    node.to - 1,
                    node.to,
                  ),
                );
              }
            }
          } else if (node.name === 'FencedCode') {
            const lastPosition = Math.min(node.to, visible.to);
            let line = state.doc.lineAt(Math.max(node.from, visible.from));
            while (line.from <= lastPosition) {
              ranges.push(
                Decoration.line({ class: 'cm-md-code-block-line' }).range(
                  line.from,
                ),
              );
              if (line.to >= lastPosition || line.number === state.doc.lines)
                break;
              line = state.doc.line(line.number + 1);
            }
          }

          if (markerNames.has(node.name)) {
            const parent = node.node.parent;
            const active = selectionTouches(
              state,
              parent?.from ?? node.from,
              parent?.to ?? node.to,
            );
            ranges.push(
              Decoration.mark({
                class: active
                  ? 'cm-md-syntax cm-md-syntax-active'
                  : 'cm-md-syntax',
              }).range(node.from, node.to),
            );
          }
        },
      });
    }

    return Decoration.set(ranges, true);
  } catch {
    return Decoration.none;
  }
}

class RichMarkdownPlugin {
  decorations: DecorationSet;

  constructor(view: EditorView) {
    this.decorations = buildRichMarkdownDecorations(
      view.state,
      view.visibleRanges,
    );
  }

  update(update: ViewUpdate) {
    if (
      update.docChanged ||
      update.selectionSet ||
      update.viewportChanged ||
      update.geometryChanged
    ) {
      this.decorations = buildRichMarkdownDecorations(
        update.state,
        update.view.visibleRanges,
      );
    }
  }
}

const richMarkdownPlugin = ViewPlugin.fromClass(RichMarkdownPlugin, {
  decorations: (plugin) => plugin.decorations,
});

interface EmphasisLayer {
  kind: 'bold' | 'italic';
  from: number;
  to: number;
  openFrom: number;
  openTo: number;
  closeFrom: number;
  closeTo: number;
  semanticFrom: number;
  semanticTo: number;
}

function emphasisLayers(
  state: EditorState,
  from: number,
  to: number,
): EmphasisLayer[] {
  const layers: EmphasisLayer[] = [];
  syntaxTree(state).iterate({
    from,
    to,
    enter(node) {
      if (node.name !== 'Emphasis' && node.name !== 'StrongEmphasis') return;
      const marks: LinkedSyntaxNode[] = [];
      let child = node.node.firstChild;
      while (child) {
        if (child.name === 'EmphasisMark') marks.push(child);
        child = child.nextSibling;
      }
      if (marks.length < 2) return;
      const open = marks[0];
      const close = marks[marks.length - 1];
      let semanticFrom = open.to;
      let semanticTo = close.from;
      let nested = node.node.firstChild;
      while (nested) {
        if (
          (nested.name === 'Emphasis' || nested.name === 'StrongEmphasis') &&
          nested.from === semanticFrom &&
          nested.to === semanticTo
        ) {
          const nestedMarks: LinkedSyntaxNode[] = [];
          let nestedChild = nested.firstChild;
          while (nestedChild) {
            if (nestedChild.name === 'EmphasisMark') {
              nestedMarks.push(nestedChild);
            }
            nestedChild = nestedChild.nextSibling;
          }
          if (nestedMarks.length < 2) break;
          semanticFrom = nestedMarks[0].to;
          semanticTo = nestedMarks[nestedMarks.length - 1].from;
          nested = nested.firstChild;
          continue;
        }
        nested = nested.nextSibling;
      }
      layers.push({
        kind: node.name === 'StrongEmphasis' ? 'bold' : 'italic',
        from: node.from,
        to: node.to,
        openFrom: open.from,
        openTo: open.to,
        closeFrom: close.from,
        closeTo: close.to,
        semanticFrom,
        semanticTo,
      });
    },
  });
  return layers.sort(
    (left, right) => left.to - left.from - (right.to - right.from),
  );
}

function layerMatches(layer: EmphasisLayer, from: number, to: number) {
  return (
    (from === layer.from && to === layer.to) ||
    (from === layer.openTo && to === layer.closeFrom) ||
    (from === layer.semanticFrom && to === layer.semanticTo)
  );
}

function removeLayer(
  range: SelectionRange,
  layer: EmphasisLayer,
): ReturnType<typeof markdownToggle> {
  const openLength = layer.openTo - layer.openFrom;
  const closeLength = layer.closeTo - layer.closeFrom;
  const mapPosition = (position: number) =>
    position -
    (position >= layer.openTo ? openLength : 0) -
    (position >= layer.closeTo ? closeLength : 0);
  return {
    changes: [
      { from: layer.openFrom, to: layer.openTo, insert: '' },
      { from: layer.closeFrom, to: layer.closeTo, insert: '' },
    ],
    anchor: mapPosition(range.anchor),
    head: mapPosition(range.head),
  };
}

function blockAt(state: EditorState, position: number, side: -1 | 1) {
  let node: LinkedSyntaxNode | null = syntaxTree(state).resolveInner(
    position,
    side,
  );
  while (node) {
    if (
      node.name === 'InlineCode' ||
      node.name === 'FencedCode' ||
      node.name === 'CodeBlock' ||
      node.name === 'CodeText'
    ) {
      return null;
    }
    if (
      node.name === 'Paragraph' ||
      /^(?:ATX|Setext)Heading[1-6]$/.test(node.name) ||
      node.name === 'ListItem'
    ) {
      return { name: node.name, from: node.from, to: node.to };
    }
    node = node.parent;
  }
  return null;
}

function staysInOneInlineBlock(state: EditorState, from: number, to: number) {
  if (from === to) return true;
  const first = blockAt(state, from, 1);
  const last = blockAt(state, to, -1);
  return (
    first !== null &&
    last !== null &&
    first.name === last.name &&
    first.from === last.from &&
    first.to === last.to
  );
}

function escapedAt(content: string, position: number) {
  let backslashes = 0;
  for (
    let index = position - 1;
    index >= 0 && content[index] === '\\';
    index -= 1
  ) {
    backslashes += 1;
  }
  return backslashes % 2 === 1;
}

function unparsedCursorPair(
  state: EditorState,
  position: number,
  delimiters: readonly string[],
) {
  const content = state.sliceDoc();
  for (const delimiter of delimiters) {
    const openFrom = position - delimiter.length;
    const closeTo = position + delimiter.length;
    if (
      openFrom < 0 ||
      content.slice(openFrom, position) !== delimiter ||
      content.slice(position, closeTo) !== delimiter ||
      escapedAt(content, openFrom)
    ) {
      continue;
    }
    let parsedMarker = false;
    syntaxTree(state).iterate({
      from: openFrom,
      to: closeTo,
      enter(node) {
        if (node.name === 'EmphasisMark') parsedMarker = true;
      },
    });
    if (!parsedMarker) {
      return {
        changes: [
          { from: openFrom, to: position, insert: '' },
          { from: position, to: closeTo, insert: '' },
        ],
        anchor: openFrom,
        head: openFrom,
      };
    }
  }
  return null;
}

function markerBoundaryWithoutLayer(
  state: EditorState,
  from: number,
  to: number,
) {
  const selected = state.sliceDoc(from, to);
  return /^[*_]/.test(selected) || /[*_]$/.test(selected);
}

function escapedAdjacentPair(
  state: EditorState,
  from: number,
  to: number,
  delimiters: readonly string[],
) {
  const content = state.sliceDoc();
  return delimiters.some((delimiter) => {
    const openFrom = from - delimiter.length;
    return (
      openFrom >= 0 &&
      content.slice(openFrom, from) === delimiter &&
      content.slice(to, to + delimiter.length) === delimiter &&
      escapedAt(content, openFrom)
    );
  });
}

function intersectsStructuralSyntax(
  state: EditorState,
  from: number,
  to: number,
) {
  const structuralNames = new Set([
    'HeaderMark',
    'ListMark',
    'QuoteMark',
    'CodeMark',
    'LinkMark',
    'LinkLabel',
    'LinkReference',
    'LinkTitle',
    'URL',
    'InlineCode',
    'CodeText',
    'FencedCode',
    'CodeBlock',
    'Escape',
  ]);
  let intersects = false;
  syntaxTree(state).iterate({
    from,
    to,
    enter(node) {
      if (
        structuralNames.has(node.name) &&
        (from === to
          ? node.from <= from && from < node.to
          : node.from < to && node.to > from)
      ) {
        intersects = true;
      }
    },
  });
  return intersects;
}

export function toggleMarkdownMarker(marker: string) {
  return (view: EditorView) => {
    if (view.state.readOnly) return false;
    const transaction = view.state.changeByRange((range) => {
      const kind = marker.length === 2 ? 'bold' : 'italic';
      const delimiters = kind === 'bold' ? ['**', '__'] : ['*', '_'];
      const layers = emphasisLayers(view.state, range.from, range.to);
      const layer = layers.find(
        (candidate) =>
          candidate.kind === kind &&
          layerMatches(candidate, range.from, range.to),
      );
      let result;
      if (layer) {
        result = removeLayer(range, layer);
      } else if (intersectsStructuralSyntax(view.state, range.from, range.to)) {
        result = {
          changes: [],
          anchor: range.anchor,
          head: range.head,
        };
      } else if (range.empty) {
        result =
          unparsedCursorPair(view.state, range.from, delimiters) ??
          markdownToggle(
            view.state.sliceDoc(),
            range.anchor,
            range.head,
            marker,
          );
      } else if (
        !staysInOneInlineBlock(view.state, range.from, range.to) ||
        escapedAdjacentPair(view.state, range.from, range.to, delimiters) ||
        (markerBoundaryWithoutLayer(view.state, range.from, range.to) &&
          !layers.some((candidate) =>
            layerMatches(candidate, range.from, range.to),
          ))
      ) {
        result = {
          changes: [],
          anchor: range.anchor,
          head: range.head,
        };
      } else {
        result = markdownToggle(
          view.state.sliceDoc(),
          range.anchor,
          range.head,
          marker,
        );
      }
      return {
        changes: result.changes,
        range: EditorSelection.range(result.anchor, result.head),
      };
    });
    view.dispatch({
      ...transaction,
      annotations: isolateHistory.of('full'),
    });
    return true;
  };
}

export const toggleBold = toggleMarkdownMarker('**');
export const toggleItalic = toggleMarkdownMarker('*');

export const richMarkdownEditing: Extension = [
  EditorView.baseTheme({
    '&.cm-focused': { outline: 'none' },
  }),
  richMarkdownPlugin,
  keymap.of([
    { key: 'Mod-b', run: toggleBold },
    { key: 'Mod-i', run: toggleItalic },
  ]),
];
