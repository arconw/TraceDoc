<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import {
    defaultKeymap,
    history,
    historyKeymap,
    indentWithTab,
  } from '@codemirror/commands';
  import {
    bracketMatching,
    defaultHighlightStyle,
    indentOnInput,
    syntaxHighlighting,
  } from '@codemirror/language';
  import { markdown } from '@codemirror/lang-markdown';
  import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
  import { EditorState } from '@codemirror/state';
  import {
    crosshairCursor,
    drawSelection,
    dropCursor,
    EditorView,
    highlightActiveLine,
    highlightActiveLineGutter,
    highlightSpecialChars,
    keymap,
    lineNumbers,
    rectangularSelection,
  } from '@codemirror/view';
  import { onMount } from 'svelte';
  import type { Document as WorkspaceDocument } from '../types/workspace';

  export let document: WorkspaceDocument | null;

  let editorHost: HTMLDivElement;
  let editorView: EditorView | null = null;
  let activeDocument: WorkspaceDocument | null = null;
  let requestedDocumentId: string | null | undefined = undefined;
  let savedContent = '';
  let dirty = false;
  let requestVersion = 0;
  let status: 'empty' | 'loading' | 'ready' | 'saving' | 'error' = 'empty';
  let message: string | null = null;

  $: if (editorView && (document?.id ?? null) !== requestedDocumentId) {
    requestedDocumentId = document?.id ?? null;
    void loadDocument(document);
  }

  $: statusLabel = (() => {
    if (status === 'loading') return 'Loading…';
    if (status === 'saving') return 'Saving…';
    if (status === 'error') return 'Load failed';
    if (message) return 'Save failed';
    if (dirty) return 'Unsaved';
    return activeDocument ? 'Saved' : '';
  })();

  onMount(() => {
    editorView = new EditorView({
      state: createEditorState('', false),
      parent: editorHost,
    });

    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      if (!dirty) return;
      event.preventDefault();
      event.returnValue = '';
    };

    const handleWindowSave = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.altKey ||
        (!event.ctrlKey && !event.metaKey) ||
        event.key.toLowerCase() !== 's'
      ) {
        return;
      }

      event.preventDefault();
      void save();
    };

    window.addEventListener('beforeunload', handleBeforeUnload);
    window.addEventListener('keydown', handleWindowSave);

    return () => {
      requestVersion += 1;
      window.removeEventListener('beforeunload', handleBeforeUnload);
      window.removeEventListener('keydown', handleWindowSave);
      editorView?.destroy();
      editorView = null;
    };
  });

  export function isDirty() {
    return dirty;
  }

  export function getSaveError() {
    return message;
  }

  export async function save(): Promise<boolean> {
    if (!editorView || !activeDocument) {
      message = 'No document is ready to save.';
      return false;
    }

    if (status !== 'ready') {
      message =
        status === 'saving'
          ? 'The document is already being saved.'
          : 'The document is not ready to save.';
      return false;
    }

    const documentId = activeDocument.id;
    const documentPath = activeDocument.path;
    const content = editorView.state.sliceDoc();
    status = 'saving';
    message = null;

    try {
      await invoke('write_document', { documentPath, content });

      if (activeDocument?.id !== documentId || !editorView) return false;
      savedContent = content;
      dirty = editorView.state.sliceDoc() !== savedContent;
      status = 'ready';
      return true;
    } catch (error) {
      if (activeDocument?.id === documentId) {
        message = errorMessage(error);
        status = 'ready';
      }
      return false;
    }
  }

  function createEditorState(content: string, editable: boolean) {
    return EditorState.create({
      doc: content,
      extensions: [
        EditorState.lineSeparator.of(detectLineSeparator(content)),
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightSpecialChars(),
        history(),
        drawSelection(),
        dropCursor(),
        EditorState.allowMultipleSelections.of(true),
        indentOnInput(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        bracketMatching(),
        rectangularSelection(),
        crosshairCursor(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        markdown(),
        EditorState.readOnly.of(!editable),
        EditorView.editable.of(editable),
        EditorView.contentAttributes.of({
          'aria-label': 'Markdown source editor',
          spellcheck: 'false',
        }),
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (!update.docChanged || !activeDocument) return;
          dirty = update.state.sliceDoc() !== savedContent;
          message = null;
        }),
        keymap.of([
          {
            key: 'Mod-s',
            preventDefault: true,
            run: () => {
              void save();
              return true;
            },
          },
          indentWithTab,
          ...defaultKeymap,
          ...searchKeymap,
          ...historyKeymap,
        ]),
      ],
    });
  }

  async function loadDocument(target: WorkspaceDocument | null) {
    const version = ++requestVersion;
    activeDocument = null;
    savedContent = '';
    dirty = false;
    message = null;

    if (!target) {
      status = 'empty';
      editorView?.setState(createEditorState('', false));
      return;
    }

    status = 'loading';
    editorView?.setState(createEditorState('', false));

    try {
      const content = await invoke<string>('read_document', {
        documentPath: target.path,
      });

      if (version !== requestVersion || !editorView) return;
      activeDocument = target;
      savedContent = content;
      dirty = false;
      status = 'ready';
      editorView.setState(createEditorState(content, true));
      editorView.focus();
    } catch (error) {
      if (version !== requestVersion) return;
      status = 'error';
      message = errorMessage(error);
    }
  }

  function retryLoad() {
    if (document) void loadDocument(document);
  }

  function errorMessage(error: unknown) {
    return error instanceof Error ? error.message : String(error);
  }

  function detectLineSeparator(content: string) {
    return /\r\n|\r|\n/.exec(content)?.[0] ?? '\n';
  }
</script>

<section class="editor-pane" aria-label="Document editor">
  {#if document}
    <header class="editor-header">
      <div class="editor-title">
        <span class="editor-name">{document.name}</span>
        {#if dirty}
          <span
            class="dirty-indicator"
            title="Unsaved changes"
            aria-label="Unsaved changes"
          ></span>
        {/if}
        <span
          class:error={Boolean(message)}
          class="editor-status"
          title={message ?? undefined}
        >
          {statusLabel}
        </span>
      </div>
      <button
        type="button"
        class="save-button"
        disabled={!dirty || status !== 'ready'}
        title="Save document (Ctrl/Cmd+S)"
        onclick={() => save()}
      >
        Save
      </button>
    </header>
  {/if}

  <div class="editor-body">
    <div class="editor-host" bind:this={editorHost}></div>

    {#if !document}
      <div class="editor-empty">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M6 3.5h8l4 4v13H6z" />
          <path d="M14 3.5v4h4M9 12h6M9 15.5h6" />
        </svg>
        <p>Select a Markdown document</p>
        <span>Its source will open here.</span>
      </div>
    {:else if status === 'loading'}
      <div class="editor-empty">
        <p>Loading document…</p>
      </div>
    {:else if status === 'error'}
      <div class="editor-empty editor-empty--error">
        <p>Unable to open document</p>
        <span>{message}</span>
        <button type="button" onclick={retryLoad}>Retry</button>
      </div>
    {/if}
  </div>
</section>

<style>
  .editor-pane {
    display: grid;
    min-width: 0;
    min-height: 0;
    grid-template-rows: auto minmax(0, 1fr);
    background: var(--color-background);
  }

  .editor-header {
    display: flex;
    min-width: 0;
    height: 2.75rem;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .editor-title {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: var(--space-2);
  }

  .editor-name {
    overflow: hidden;
    color: var(--color-foreground);
    font-size: var(--font-size-sm);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dirty-indicator {
    width: 0.4375rem;
    min-width: 0.4375rem;
    height: 0.4375rem;
    border-radius: var(--radius-full);
    background: var(--color-focus);
  }

  .editor-status {
    overflow: hidden;
    color: var(--color-muted-dim);
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .editor-status.error {
    color: var(--color-error);
  }

  .save-button,
  .editor-empty button {
    padding: 0.35rem var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-foreground);
    cursor: pointer;
    font-size: 0.75rem;
  }

  .save-button:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .save-button:focus-visible,
  .editor-empty button:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }

  .editor-body {
    position: relative;
    min-width: 0;
    min-height: 0;
  }

  .editor-host {
    width: 100%;
    height: 100%;
  }

  .editor-host :global(.cm-editor) {
    height: 100%;
    background: var(--color-background);
    color: var(--color-foreground-subtle);
    font-family: var(--font-family-mono);
    font-size: 0.875rem;
  }

  .editor-host :global(.cm-scroller) {
    overflow: auto;
    font-family: inherit;
    line-height: 1.65;
    scrollbar-color: var(--color-border-strong) transparent;
    scrollbar-width: thin;
  }

  .editor-host :global(.cm-content) {
    max-width: 72rem;
    min-height: 100%;
    padding: var(--space-5) var(--space-6) 30vh;
    caret-color: var(--color-foreground);
  }

  .editor-host :global(.cm-line) {
    padding: 0;
  }

  .editor-host :global(.cm-gutters) {
    border-right: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-muted-dim);
  }

  .editor-host :global(.cm-lineNumbers .cm-gutterElement) {
    min-width: 2.75rem;
    padding: 0 var(--space-3) 0 var(--space-2);
  }

  .editor-host :global(.cm-activeLine),
  .editor-host :global(.cm-activeLineGutter) {
    background: color-mix(
      in srgb,
      var(--color-surface-raised) 42%,
      transparent
    );
  }

  .editor-host :global(.cm-selectionBackground),
  .editor-host :global(.cm-content ::selection) {
    background: var(--color-selection) !important;
  }

  .editor-host :global(.cm-cursor) {
    border-left-color: var(--color-foreground);
  }

  .editor-host :global(.cm-focused) {
    outline: none;
  }

  .editor-host :global(.cm-panels) {
    border-color: var(--color-border);
    background: var(--color-surface);
    color: var(--color-foreground);
  }

  .editor-host :global(.cm-search) {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    align-items: center;
  }

  .editor-host :global(.cm-search input),
  .editor-host :global(.cm-search button) {
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-foreground);
    font-family: var(--font-family-mono);
    font-size: 0.75rem;
  }

  .editor-empty {
    position: absolute;
    inset: 0;
    display: grid;
    align-content: center;
    justify-items: center;
    gap: var(--space-2);
    padding: var(--space-6);
    background: var(--color-background);
    color: var(--color-muted);
    text-align: center;
  }

  .editor-empty svg {
    width: 2rem;
    height: 2rem;
    margin-bottom: var(--space-1);
    fill: none;
    stroke: var(--color-border-strong);
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-width: 1.2;
  }

  .editor-empty p {
    margin: 0;
    color: var(--color-foreground-subtle);
    font-size: var(--font-size-sm);
    font-weight: 500;
  }

  .editor-empty span {
    max-width: 34rem;
    color: var(--color-muted-dim);
    font-size: 0.75rem;
  }

  .editor-empty--error span {
    color: var(--color-error);
  }
</style>
