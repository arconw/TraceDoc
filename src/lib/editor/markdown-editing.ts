export interface MarkdownToggleEdit {
  from: number;
  to: number;
  insert: string;
}

export interface MarkdownToggleResult {
  changes: MarkdownToggleEdit[];
  anchor: number;
  head: number;
}

function exactMarkerAt(content: string, from: number, marker: string) {
  if (content.slice(from, from + marker.length) !== marker) return false;
  if (marker !== '*') return true;
  return content[from - 1] !== '*' && content[from + 1] !== '*';
}

function unchanged(anchor: number, head: number): MarkdownToggleResult {
  return { changes: [], anchor, head };
}

function directedRange(from: number, to: number, reversed: boolean) {
  return {
    anchor: reversed ? to : from,
    head: reversed ? from : to,
  };
}

export function markdownToggle(
  content: string,
  anchor: number,
  head: number,
  marker: string,
): MarkdownToggleResult {
  const from = Math.min(anchor, head);
  const to = Math.max(anchor, head);
  const reversed = anchor > head;
  const selected = content.slice(from, to);

  if (
    selected.length > 0 &&
    (/^\s|\s$/.test(selected) || /\n\s*\n/.test(selected))
  ) {
    return unchanged(anchor, head);
  }

  const outsideStart = from - marker.length;
  const hasMarkerBefore =
    outsideStart >= 0 && exactMarkerAt(content, outsideStart, marker);
  const hasMarkerAfter = exactMarkerAt(content, to, marker);

  if (hasMarkerBefore !== hasMarkerAfter) return unchanged(anchor, head);
  if (hasMarkerBefore && hasMarkerAfter) {
    const nextFrom = from - marker.length;
    const nextTo = to - marker.length;
    return {
      changes: [
        { from: outsideStart, to: from, insert: '' },
        { from: to, to: to + marker.length, insert: '' },
      ],
      ...directedRange(nextFrom, nextTo, reversed),
    };
  }

  const insideEnd = to - marker.length;
  const hasMarkerInsideStart =
    selected.length >= marker.length * 2 &&
    exactMarkerAt(content, from, marker);
  const hasMarkerInsideEnd =
    selected.length >= marker.length * 2 &&
    exactMarkerAt(content, insideEnd, marker);

  if (hasMarkerInsideStart !== hasMarkerInsideEnd) {
    return unchanged(anchor, head);
  }
  if (hasMarkerInsideStart && hasMarkerInsideEnd) {
    const nextTo = to - marker.length * 2;
    return {
      changes: [
        { from, to: from + marker.length, insert: '' },
        { from: insideEnd, to, insert: '' },
      ],
      ...directedRange(from, nextTo, reversed),
    };
  }

  const nextFrom = from + marker.length;
  const nextTo = to + marker.length;
  return {
    changes: [
      { from, to: from, insert: marker },
      { from: to, to, insert: marker },
    ],
    ...directedRange(nextFrom, nextTo, reversed),
  };
}
