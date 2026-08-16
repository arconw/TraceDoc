import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';
import { after, test } from 'node:test';
import { history, redo, undo } from '@codemirror/commands';
import { markdown } from '@codemirror/lang-markdown';
import { EditorSelection, EditorState } from '@codemirror/state';
import { createServer } from 'vite';

const vite = await createServer({
  configFile: false,
  root: new URL('../../..', import.meta.url).pathname,
  server: { middlewareMode: true },
});
const rich = await vite.ssrLoadModule('/src/lib/editor/rich-markdown.ts');
const {
  buildRichMarkdownDecorations,
  richMarkdownEditing,
  toggleBold,
  toggleItalic,
} = rich;

after(async () => {
  await vite.close();
});

function state(content, selection) {
  return EditorState.create({
    doc: content,
    selection,
    extensions: [
      EditorState.allowMultipleSelections.of(true),
      history(),
      markdown(),
      richMarkdownEditing,
    ],
  });
}

function decorations(editorState, visibleRanges, treeProvider) {
  const found = [];
  const set = buildRichMarkdownDecorations(
    editorState,
    visibleRanges ?? [{ from: 0, to: editorState.doc.length }],
    treeProvider,
  );
  set.between(0, editorState.doc.length, (from, to, value) => {
    found.push({ from, to, class: value.spec.class });
  });
  return found;
}

function hasClass(records, className, text, content) {
  return records.some(
    (record) =>
      record.class?.split(' ').includes(className) &&
      (text === undefined || content.slice(record.from, record.to) === text),
  );
}

function commandTarget(initialState) {
  let current = initialState;
  return {
    get state() {
      return current;
    },
    dispatch(transaction) {
      current = transaction.state ?? current.update(transaction).state;
    },
  };
}

test('decorates ATX and Setext headings, emphasis, code, and fences', () => {
  const content =
    '# ATX\n\nSetext\n------\n\n**bold *nested*** and `code`\n\n```ts\nvalue\n```\n';
  const editorState = state(content, { anchor: content.length });
  const records = decorations(editorState);

  assert.equal(hasClass(records, 'cm-md-heading-1', '# ATX', content), true);
  assert.equal(
    hasClass(records, 'cm-md-heading-2', 'Setext\n------', content),
    true,
  );
  assert.equal(
    hasClass(records, 'cm-md-strong', '**bold *nested***', content),
    true,
  );
  assert.equal(hasClass(records, 'cm-md-emphasis', '*nested*', content), true);
  assert.equal(hasClass(records, 'cm-md-inline-code', '`code`', content), true);
  assert.equal(
    records.filter((record) => record.class === 'cm-md-code-block-line').length,
    3,
  );
  assert.equal(editorState.sliceDoc(), content);
});

test('styles ordinary links and CommonMark autolinks without inventing bare URL syntax', () => {
  const content =
    '[label](https://example.com) <https://openai.com/a> <user@example.com> https://bare.example';
  const records = decorations(state(content, { anchor: content.length }));

  for (const text of [
    'label',
    'https://example.com',
    'https://openai.com/a',
    'user@example.com',
  ]) {
    assert.equal(hasClass(records, 'cm-md-link', text, content), true, text);
  }
  assert.equal(
    records.some(
      (record) =>
        record.class?.includes('cm-md-link') &&
        Array.from(content.slice(record.from, record.to)).some((character) =>
          '[]()<>'.includes(character),
        ),
    ),
    false,
  );
  assert.equal(
    records.filter((record) => record.class === 'cm-md-syntax').length >= 8,
    true,
  );

  const bare = 'https://bare.example';
  const bareRecords = decorations(state(bare, { anchor: bare.length }));
  assert.equal(hasClass(bareRecords, 'cm-md-link', bare, bare), false);
});

test('splits bracketed destinations into link content and cursor-aware syntax angles', () => {
  const content = '[label](<foo bar>) and <https://example.com>';
  const resting = decorations(state(content, { anchor: content.length }));
  const active = decorations(
    state(content, { anchor: content.indexOf('foo') }),
  );

  assert.equal(hasClass(resting, 'cm-md-link', 'foo bar', content), true);
  assert.equal(hasClass(resting, 'cm-md-link', '<foo bar>', content), false);
  assert.equal(hasClass(resting, 'cm-md-syntax', '<', content), true);
  assert.equal(hasClass(resting, 'cm-md-syntax', '>', content), true);
  assert.equal(hasClass(active, 'cm-md-syntax-active', '<', content), true);
  assert.equal(
    hasClass(resting, 'cm-md-link', 'https://example.com', content),
    true,
  );
});

test('keeps empty bracketed destinations as nonzero syntax decorations', () => {
  for (const content of ['[label](<>)', '[](<>)']) {
    const records = decorations(state(content, { anchor: content.length }));
    assert.equal(records.length > 0, true);
    assert.equal(
      records.every((record) => record.from < record.to),
      true,
    );
    assert.equal(hasClass(records, 'cm-md-syntax', '<', content), true);
    assert.equal(hasClass(records, 'cm-md-syntax', '>', content), true);
    assert.equal(hasClass(records, 'cm-md-link', '<>', content), false);
  }
});

test('makes syntax active only when selection touches its Markdown construct', () => {
  const content = 'before **bold** after';
  const resting = decorations(state(content, { anchor: 0 }));
  const active = decorations(
    state(content, { anchor: content.indexOf('bold') + 1 }),
  );

  assert.equal(
    hasClass(resting, 'cm-md-syntax-active', undefined, content),
    false,
  );
  assert.equal(hasClass(active, 'cm-md-syntax-active', '**', content), true);
  assert.equal(
    active.filter((record) =>
      record.class?.split(' ').includes('cm-md-syntax-active'),
    ).length,
    2,
  );
});

test('keeps malformed Markdown editable and falls back to no decorations on parser failure', () => {
  const content = '[broken](url and **unfinished';
  const editorState = state(content, { anchor: 4 });
  assert.doesNotThrow(() => decorations(editorState));
  assert.deepEqual(
    decorations(editorState, undefined, () => {
      throw new Error('synthetic parser failure');
    }),
    [],
  );
  assert.equal(editorState.sliceDoc(), content);
});

test('bounds large-document decoration work to visible ranges and updates after edits', () => {
  const content = Array.from(
    { length: 4000 },
    (_, index) => `## Heading ${index}\n\nText **${index}** [link](doc.md).\n`,
  ).join('\n');
  const editorState = state(content, { anchor: 0 });
  const started = performance.now();
  const first = decorations(editorState, [{ from: 0, to: 600 }]);
  const elapsed = performance.now() - started;
  const updated = editorState.update({
    changes: { from: 0, insert: '# Top\n' },
  }).state;
  const second = decorations(updated, [{ from: 0, to: 600 }]);

  assert.equal(content.length > 100_000, true);
  assert.equal(first.length < 200, true);
  assert.equal(second.length < 200, true);
  assert.equal(
    hasClass(second, 'cm-md-heading-1', '# Top', updated.sliceDoc()),
    true,
  );
  assert.equal(elapsed < 2000, true, `visible decorations took ${elapsed}ms`);
  assert.equal(editorState.sliceDoc(), content);
});

test('runs actual bold and italic commands as atomic undoable transactions', () => {
  const boldTarget = commandTarget(
    state('**plain**', EditorSelection.single(0, 9)),
  );
  assert.equal(toggleBold(boldTarget), true);
  assert.equal(boldTarget.state.sliceDoc(), 'plain');
  assert.deepEqual(boldTarget.state.selection.main.toJSON(), {
    anchor: 0,
    head: 5,
  });
  assert.equal(undo(boldTarget), true);
  assert.equal(boldTarget.state.sliceDoc(), '**plain**');
  assert.equal(redo(boldTarget), true);
  assert.equal(boldTarget.state.sliceDoc(), 'plain');

  const italicTarget = commandTarget(
    state('*plain*', EditorSelection.single(7, 0)),
  );
  assert.equal(toggleItalic(italicTarget), true);
  assert.equal(italicTarget.state.sliceDoc(), 'plain');
  assert.deepEqual(italicTarget.state.selection.main.toJSON(), {
    anchor: 5,
    head: 0,
  });
});

test('supports paired cursors and multiple selections in one transaction', () => {
  const cursorTarget = commandTarget(state('text', { anchor: 2 }));
  assert.equal(toggleItalic(cursorTarget), true);
  assert.equal(cursorTarget.state.sliceDoc(), 'te**xt');
  assert.equal(cursorTarget.state.selection.main.head, 3);

  const multipleTarget = commandTarget(
    state(
      'one two',
      EditorSelection.create([
        EditorSelection.range(0, 3),
        EditorSelection.range(4, 7),
      ]),
    ),
  );
  assert.equal(toggleBold(multipleTarget), true);
  assert.equal(multipleTarget.state.sliceDoc(), '**one** **two**');
  assert.equal(multipleTarget.state.selection.ranges.length, 2);
  assert.equal(undo(multipleTarget), true);
  assert.equal(multipleTarget.state.sliceDoc(), 'one two');
});

test('keeps italic toggling involutive across paired and nested stars', () => {
  const pairTarget = commandTarget(state('te**xt', { anchor: 3 }));
  assert.equal(toggleItalic(pairTarget), true);
  assert.equal(pairTarget.state.sliceDoc(), 'text');
  assert.equal(pairTarget.state.selection.main.head, 2);

  const nestedTarget = commandTarget(
    state('**plain**', EditorSelection.single(0, 9)),
  );
  assert.equal(toggleItalic(nestedTarget), true);
  assert.equal(nestedTarget.state.sliceDoc(), '***plain***');
  assert.deepEqual(nestedTarget.state.selection.main.toJSON(), {
    anchor: 1,
    head: 10,
  });
  assert.equal(toggleItalic(nestedTarget), true);
  assert.equal(nestedTarget.state.sliceDoc(), '**plain**');
  assert.deepEqual(nestedTarget.state.selection.main.toJSON(), {
    anchor: 0,
    head: 9,
  });

  const fullTripleTarget = commandTarget(
    state('***plain***', EditorSelection.single(0, 11)),
  );
  assert.equal(toggleItalic(fullTripleTarget), true);
  assert.equal(fullTripleTarget.state.sliceDoc(), '**plain**');
});

test('removes parsed underscore layers and nests the other emphasis kind predictably', () => {
  const italicTarget = commandTarget(
    state('_plain_', EditorSelection.single(0, 7)),
  );
  assert.equal(toggleItalic(italicTarget), true);
  assert.equal(italicTarget.state.sliceDoc(), 'plain');

  const boldTarget = commandTarget(
    state('__plain__', EditorSelection.single(0, 9)),
  );
  assert.equal(toggleBold(boldTarget), true);
  assert.equal(boldTarget.state.sliceDoc(), 'plain');

  const nestedTarget = commandTarget(
    state('_plain_', EditorSelection.single(0, 7)),
  );
  assert.equal(toggleBold(nestedTarget), true);
  assert.equal(nestedTarget.state.sliceDoc(), '**_plain_**');
});

test('does not remove escaped pseudo-markers or cross Markdown block boundaries', () => {
  const escapedTarget = commandTarget(
    state('\\**plain**', EditorSelection.single(1, 10)),
  );
  assert.equal(toggleBold(escapedTarget), true);
  assert.equal(escapedTarget.state.sliceDoc(), '\\**plain**');

  for (const content of ['one\n# two', 'one\n- two']) {
    const target = commandTarget(
      state(content, EditorSelection.single(0, content.length)),
    );
    assert.equal(toggleItalic(target), true);
    assert.equal(target.state.sliceDoc(), content);
  }

  const softBreakTarget = commandTarget(
    state('one\nsoft', EditorSelection.single(0, 8)),
  );
  assert.equal(toggleItalic(softBreakTarget), true);
  assert.equal(softBreakTarget.state.sliceDoc(), '*one\nsoft*');
});

test('does not toggle escaped adjacent pseudo-pairs selected by content only', () => {
  const escapedBold = commandTarget(
    state('\\**plain**', EditorSelection.single(3, 8)),
  );
  assert.equal(toggleBold(escapedBold), true);
  assert.equal(escapedBold.state.sliceDoc(), '\\**plain**');

  const escapedItalic = commandTarget(
    state('\\*plain*', EditorSelection.single(2, 7)),
  );
  assert.equal(toggleItalic(escapedItalic), true);
  assert.equal(escapedItalic.state.sliceDoc(), '\\*plain*');

  const evenParity = commandTarget(
    state('\\\\**plain**', EditorSelection.single(4, 9)),
  );
  assert.equal(toggleBold(evenParity), true);
  assert.equal(evenParity.state.sliceDoc(), '\\\\plain');
});

test('isolates successive formatting commands and paired edits in history', () => {
  const layered = commandTarget(state('plain', EditorSelection.single(0, 5)));
  assert.equal(toggleBold(layered), true);
  assert.equal(layered.state.sliceDoc(), '**plain**');
  assert.equal(toggleItalic(layered), true);
  assert.equal(layered.state.sliceDoc(), '***plain***');
  assert.equal(undo(layered), true);
  assert.equal(layered.state.sliceDoc(), '**plain**');
  assert.equal(undo(layered), true);
  assert.equal(layered.state.sliceDoc(), 'plain');

  const paired = commandTarget(state('text', { anchor: 2 }));
  assert.equal(toggleItalic(paired), true);
  assert.equal(paired.state.sliceDoc(), 'te**xt');
  assert.equal(toggleItalic(paired), true);
  assert.equal(paired.state.sliceDoc(), 'text');
  assert.equal(undo(paired), true);
  assert.equal(paired.state.sliceDoc(), 'te**xt');
  assert.equal(undo(paired), true);
  assert.equal(paired.state.sliceDoc(), 'text');
});

test('rejects structural marker intersections while allowing text-only formatting', () => {
  const cases = [
    { content: '# Heading', textFrom: 2, textTo: 9 },
    { content: 'Heading\n=======', textFrom: 0, textTo: 7 },
    { content: '- Item', textFrom: 2, textTo: 6 },
  ];

  for (const { content, textFrom, textTo } of cases) {
    const whole = commandTarget(
      state(content, EditorSelection.single(0, content.length)),
    );
    assert.equal(toggleBold(whole), true);
    assert.equal(whole.state.sliceDoc(), content);

    const textOnly = commandTarget(
      state(content, EditorSelection.single(textFrom, textTo)),
    );
    assert.equal(toggleBold(textOnly), true);
    assert.equal(textOnly.state.sliceDoc().includes('**'), true);
  }

  const fenced = '```\ncode\n```';
  const fencedTarget = commandTarget(
    state(fenced, EditorSelection.single(0, fenced.length)),
  );
  assert.equal(toggleItalic(fencedTarget), true);
  assert.equal(fencedTarget.state.sliceDoc(), fenced);
});

test('protects link destinations, titles, reference structures, and autolink content', () => {
  const protectedRanges = [
    { content: '[label](url.md)', from: 8, to: 14 },
    { content: '<https://x.dev>', from: 1, to: 14 },
    { content: '[label](url.md "title")', from: 15, to: 22 },
    {
      content: '[label][ref]\n\n[ref]: target.md "title"',
      from: 21,
      to: 30,
    },
    {
      content: '[label][ref]\n\n[ref]: target.md "title"',
      from: 31,
      to: 38,
    },
    {
      content: '[label][ref]\n\n[ref]: target.md',
      from: 14,
      to: 19,
    },
  ];

  for (const { content, from, to } of protectedRanges) {
    for (const selection of [
      EditorSelection.single(from, to),
      EditorSelection.cursor(from + Math.floor((to - from) / 2)),
    ]) {
      const target = commandTarget(state(content, selection));
      assert.equal(toggleBold(target), true);
      assert.equal(target.state.sliceDoc(), content);
    }
  }
});

test('formats visible link labels and image alt text but never image destinations', () => {
  const link = '[label](url.md)';
  const linkTarget = commandTarget(state(link, EditorSelection.single(1, 6)));
  assert.equal(toggleBold(linkTarget), true);
  assert.equal(linkTarget.state.sliceDoc(), '[**label**](url.md)');

  const image = '![alt](image.png "title")';
  const altTarget = commandTarget(state(image, EditorSelection.single(2, 5)));
  assert.equal(toggleItalic(altTarget), true);
  assert.equal(altTarget.state.sliceDoc(), '![*alt*](image.png "title")');

  for (const [from, to] of [
    [7, 16],
    [17, 24],
  ]) {
    const destinationTarget = commandTarget(
      state(image, EditorSelection.single(from, to)),
    );
    assert.equal(toggleBold(destinationTarget), true);
    assert.equal(destinationTarget.state.sliceDoc(), image);
  }

  const referenceImage = '![alt][img]\n\n[img]: image.png';
  const referenceLabel = commandTarget(
    state(referenceImage, EditorSelection.single(6, 11)),
  );
  assert.equal(toggleItalic(referenceLabel), true);
  assert.equal(referenceLabel.state.sliceDoc(), referenceImage);
  const referenceDestination = commandTarget(
    state(referenceImage, EditorSelection.single(20, 29)),
  );
  assert.equal(toggleItalic(referenceDestination), true);
  assert.equal(referenceDestination.state.sliceDoc(), referenceImage);
});

test('removes a matching outer strong layer around structurally protected inline content', () => {
  for (const content of [
    '**[label](url.md)**',
    '**use `code`**',
    '**a\\*b**',
  ]) {
    const expected = content.slice(2, -2);
    for (const selection of [
      EditorSelection.single(0, content.length),
      EditorSelection.single(2, content.length - 2),
    ]) {
      const target = commandTarget(state(content, selection));
      assert.equal(toggleBold(target), true);
      assert.equal(target.state.sliceDoc(), expected);
    }
  }
});

test('uses half-open structural cursor membership at link and code boundaries', () => {
  const link = '[label](url.md)';
  const afterLink = commandTarget(state(link, { anchor: link.length }));
  assert.equal(toggleItalic(afterLink), true);
  assert.equal(afterLink.state.sliceDoc(), `${link}**`);

  const insideDestination = commandTarget(
    state(link, { anchor: link.indexOf('url') + 1 }),
  );
  assert.equal(toggleItalic(insideDestination), true);
  assert.equal(insideDestination.state.sliceDoc(), link);

  const code = '`code`';
  const afterCode = commandTarget(state(code, { anchor: code.length }));
  assert.equal(toggleItalic(afterCode), true);
  assert.equal(afterCode.state.sliceDoc(), `${code}**`);

  const insideCode = commandTarget(state(code, { anchor: 2 }));
  assert.equal(toggleItalic(insideCode), true);
  assert.equal(insideCode.state.sliceDoc(), code);
});
