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
  import {
    highlightSelectionMatches,
    openSearchPanel,
    searchKeymap,
  } from '@codemirror/search';
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
  import {
    closesDeletedBufferWithoutDiskAccess,
    editorReadIsCurrent,
    retainedLocalBaseline,
    writeResultIsStale,
  } from '../editor/request-state';
  import { richMarkdownEditing } from '../editor/rich-markdown';
  import { projectStore } from '../stores/project';
  import type {
    Document as WorkspaceDocument,
    DocumentIndexUpdate,
    DocumentReadResult,
  } from '../types/workspace';
  import { saveShortcutAction } from '../utils/ui-behavior';

  export let document: WorkspaceDocument | null;
  export let selectedDocumentId: string | null;
  export let externalChangeVersion: number;
  export let workspaceGeneration: number;
  export let workspaceRevision: number;
  export let saveShortcutEnabled = true;

  let editorHost: HTMLDivElement;
  let editorView: EditorView | null = null;
  let activeDocument: WorkspaceDocument | null = null;
  let requestedDocumentId: string | null | undefined = undefined;
  let requestedWorkspaceGeneration: number | undefined = undefined;
  let handledExternalChangeVersion = 0;
  let savedContent = '';
  let savedContentToken = '';
  let dirty = false;
  let requestVersion = 0;
  let status: 'empty' | 'loading' | 'ready' | 'saving' | 'error' = 'empty';
  let message: string | null = null;
  let saveWarning: string | null = null;
  let conflict: 'modified' | 'deleted' | null = null;

  $: if (
    editorView &&
    (selectedDocumentId !== requestedDocumentId ||
      workspaceGeneration !== requestedWorkspaceGeneration)
  ) {
    requestedDocumentId = selectedDocumentId;
    requestedWorkspaceGeneration = workspaceGeneration;
    handledExternalChangeVersion = externalChangeVersion;
    void loadDocument(document, selectedDocumentId);
  }

  $: if (
    editorView &&
    externalChangeVersion !== handledExternalChangeVersion &&
    selectedDocumentId === requestedDocumentId
  ) {
    handledExternalChangeVersion = externalChangeVersion;
    void handleExternalChange();
  }

  $: statusLabel = (() => {
    if (conflict) return 'External conflict';
    if (status === 'loading') return 'Loading…';
    if (status === 'saving') return 'Saving…';
    if (status === 'error') return 'Load failed';
    if (message) return 'Save failed';
    if (saveWarning) return 'Saved with warning';
    if (dirty) return 'Modified';
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
      if (event.defaultPrevented) return;
      const action = saveShortcutAction(event, saveShortcutEnabled);
      if (action === 'ignore') return;

      event.preventDefault();
      if (action === 'save') void save();
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

  export function find() {
    if (!editorView || !activeDocument || status === 'loading') return;
    editorView.focus();
    openSearchPanel(editorView);
  }

  export async function save(): Promise<boolean> {
    if (!editorView || !activeDocument) {
      message = 'No document is ready to save.';
      return false;
    }

    if (conflict) {
      message = 'Resolve the external change before saving.';
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
    const requestGeneration = workspaceGeneration;
    const requestRevision = workspaceRevision;
    status = 'saving';
    message = null;
    saveWarning = null;

    try {
      const indexUpdate = await invoke<DocumentIndexUpdate>('write_document', {
        documentPath,
        content,
        expectedContentToken: savedContentToken,
        expectedWorkspaceRevision: requestRevision,
        workspaceGeneration: requestGeneration,
      });

      if (writeResultIsStale(indexUpdate.workspaceRevision, requestRevision)) {
        conflict = 'modified';
        status = 'ready';
        return false;
      }
      projectStore.applyDocumentIndex(indexUpdate);
      if (
        activeDocument?.id !== documentId ||
        requestGeneration !== workspaceGeneration ||
        !editorView
      ) {
        return true;
      }
      savedContent = content;
      savedContentToken = indexUpdate.contentToken;
      dirty = editorView.state.sliceDoc() !== savedContent;
      saveWarning = indexUpdate.saveWarning;
      status = 'ready';
      return true;
    } catch (error) {
      if (activeDocument?.id === documentId) {
        const nextMessage = errorMessage(error);
        if (nextMessage.includes('changed externally')) {
          conflict = 'modified';
          message = null;
        } else {
          message = nextMessage;
        }
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
        richMarkdownEditing,
        EditorState.readOnly.of(!editable),
        EditorView.editable.of(editable),
        EditorView.contentAttributes.of({
          'aria-label': 'Markdown source editor',
          'aria-describedby': 'markdown-editor-help',
          spellcheck: 'false',
        }),
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (!update.docChanged || !activeDocument) return;
          dirty = update.state.sliceDoc() !== savedContent;
          message = null;
          saveWarning = null;
        }),
        keymap.of([
          {
            key: 'Mod-Shift-s',
            preventDefault: true,
            run: () => true,
          },
          {
            key: 'Mod-s',
            preventDefault: true,
            run: () => {
              if (saveShortcutEnabled) void save();
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

  async function loadDocument(
    target: WorkspaceDocument | null,
    targetId: string | null,
    discardLocal = false,
  ) {
    if (
      !discardLocal &&
      !target &&
      targetId &&
      activeDocument?.id === targetId &&
      dirty
    ) {
      conflict = 'deleted';
      message = null;
      saveWarning = null;
      status = 'ready';
      return;
    }
    const version = ++requestVersion;
    const requestGeneration = workspaceGeneration;
    activeDocument = null;
    savedContent = '';
    savedContentToken = '';
    dirty = false;
    message = null;
    saveWarning = null;
    conflict = null;

    if (!target) {
      status = 'empty';
      editorView?.setState(createEditorState('', false));
      return;
    }

    status = 'loading';
    editorView?.setState(createEditorState('', false));

    try {
      const result = await invoke<DocumentReadResult>('read_document', {
        documentPath: target.path,
        workspaceGeneration: requestGeneration,
      });

      if (
        !editorReadIsCurrent(
          {
            version,
            documentId: target.id,
            workspaceGeneration: requestGeneration,
          },
          requestVersion,
          selectedDocumentId,
          workspaceGeneration,
          result.workspaceGeneration,
        ) ||
        !editorView
      ) {
        return;
      }
      activeDocument = target;
      savedContent = result.content;
      savedContentToken = result.contentToken;
      dirty = false;
      status = 'ready';
      editorView.setState(createEditorState(result.content, true));
      editorView.focus();
    } catch (error) {
      if (version !== requestVersion) return;
      status = 'error';
      message = errorMessage(error);
    }
  }

  function retryLoad() {
    if (document) void loadDocument(document, selectedDocumentId);
  }

  function handleExternalChange() {
    if (!selectedDocumentId) return;
    requestVersion += 1;
    if (!activeDocument || activeDocument.id !== selectedDocumentId) {
      return loadDocument(document, selectedDocumentId);
    }
    if (dirty || status === 'saving') {
      conflict = document ? 'modified' : 'deleted';
      message = null;
      saveWarning = null;
      status = 'ready';
      return;
    }
    return loadDocument(document, selectedDocumentId);
  }

  async function keepLocalChanges() {
    if (conflict !== 'modified' || !activeDocument || status !== 'ready') {
      return;
    }
    const version = ++requestVersion;
    const documentId = activeDocument.id;
    const documentPath = activeDocument.path;
    const requestGeneration = workspaceGeneration;
    status = 'loading';
    try {
      await invoke('acknowledge_save_conflict', {
        documentPath,
        workspaceGeneration: requestGeneration,
      });
      const result = await invoke<DocumentReadResult>('read_document', {
        documentPath,
        workspaceGeneration: requestGeneration,
      });
      if (
        !editorReadIsCurrent(
          { version, documentId, workspaceGeneration: requestGeneration },
          requestVersion,
          selectedDocumentId,
          workspaceGeneration,
          result.workspaceGeneration,
        ) ||
        activeDocument?.id !== documentId ||
        !editorView
      ) {
        return;
      }
      const retained = retainedLocalBaseline(
        editorView.state.sliceDoc(),
        result.content,
        result.contentToken,
      );
      savedContent = retained.savedContent;
      savedContentToken = retained.savedContentToken;
      dirty = retained.dirty;
      conflict = null;
      status = 'ready';
    } catch (error) {
      if (
        !editorReadIsCurrent(
          { version, documentId, workspaceGeneration: requestGeneration },
          requestVersion,
          selectedDocumentId,
          workspaceGeneration,
          requestGeneration,
        ) ||
        activeDocument?.id !== documentId
      ) {
        return;
      }
      message = errorMessage(error);
      status = 'ready';
    }
  }

  async function reloadExternal() {
    if (!activeDocument || status !== 'ready') return;
    const version = ++requestVersion;
    const documentId = activeDocument.id;
    const documentPath = activeDocument.path;
    const requestGeneration = workspaceGeneration;
    if (closesDeletedBufferWithoutDiskAccess(conflict)) {
      activeDocument = null;
      requestedDocumentId = null;
      savedContent = '';
      savedContentToken = '';
      dirty = false;
      conflict = null;
      message = null;
      saveWarning = null;
      status = 'empty';
      editorView?.setState(createEditorState('', false));
      projectStore.closeDocument(documentId);
      return;
    }
    status = 'loading';
    try {
      await invoke('acknowledge_save_conflict', {
        documentPath,
        workspaceGeneration: requestGeneration,
      });
      if (
        version !== requestVersion ||
        selectedDocumentId !== documentId ||
        workspaceGeneration !== requestGeneration
      ) {
        return;
      }
      dirty = false;
      conflict = null;
      await loadDocument(document, selectedDocumentId, true);
    } catch (error) {
      if (
        version !== requestVersion ||
        selectedDocumentId !== documentId ||
        workspaceGeneration !== requestGeneration
      ) {
        return;
      }
      message = errorMessage(error);
      status = 'ready';
    }
  }

  function errorMessage(error: unknown) {
    return error instanceof Error ? error.message : String(error);
  }

  function detectLineSeparator(content: string) {
    return /\r\n|\r|\n/.exec(content)?.[0] ?? '\n';
  }
</script>

<section
  class:has-conflict={Boolean(conflict)}
  class:has-warning={Boolean(saveWarning)}
  class="editor-pane"
  aria-label="Document editor"
>
  {#if document || activeDocument}
    <header class="editor-header">
      <div class="editor-title">
        <span class="editor-name">{document?.name ?? activeDocument?.name}</span
        >
        {#if dirty}
          <span
            class="dirty-indicator"
            title="Unsaved changes"
            aria-label="Unsaved changes"
          ></span>
        {/if}
        <span
          class:error={Boolean(message)}
          class:warning={Boolean(saveWarning)}
          class="editor-status"
          title={message ?? saveWarning ?? undefined}
        >
          {statusLabel}
        </span>
      </div>
      <span class="editor-mode" aria-hidden="true">Markdown</span>
      <button
        type="button"
        class="save-button"
        disabled={!dirty || status !== 'ready' || Boolean(conflict)}
        aria-keyshortcuts="Control+S Meta+S"
        title="Save document (Ctrl/Cmd+S)"
        onclick={() => save()}
      >
        Save
      </button>
    </header>
    {#if saveWarning}
      <div class="save-warning" role="status" aria-live="polite">
        {saveWarning}
      </div>
    {/if}
    {#if conflict}
      <div class="conflict-banner" role="alert">
        <span>
          {conflict === 'deleted'
            ? 'This file was deleted externally. Your unsaved buffer is still open.'
            : 'This file changed externally. Your unsaved buffer was not overwritten.'}
        </span>
        <div>
          {#if conflict === 'modified'}
            <button type="button" onclick={keepLocalChanges}>Keep mine</button>
          {/if}
          <button type="button" onclick={reloadExternal}>
            {conflict === 'deleted' ? 'Close buffer' : 'Reload file'}
          </button>
        </div>
      </div>
    {/if}
  {/if}

  <div class="editor-body">
    <p id="markdown-editor-help" class="sr-only">
      Markdown source editor. Control or Command B toggles bold. Control or
      Command I toggles italic. Markdown punctuation becomes fully visible when
      the cursor enters its formatted text.
    </p>
    <div class="editor-host" bind:this={editorHost}></div>

    {#if !document && !activeDocument}
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

  .editor-pane.has-conflict {
    grid-template-rows: auto auto minmax(0, 1fr);
  }

  .editor-pane.has-warning {
    grid-template-rows: auto auto minmax(0, 1fr);
  }

  .editor-pane.has-warning.has-conflict {
    grid-template-rows: auto auto auto minmax(0, 1fr);
  }

  .save-warning {
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid color-mix(in srgb, #a66a00 40%, transparent);
    background: color-mix(in srgb, #a66a00 9%, var(--color-surface));
    color: var(--color-foreground-subtle);
    font-size: 0.75rem;
  }

  .conflict-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid
      color-mix(in srgb, var(--color-error) 45%, transparent);
    background: color-mix(
      in srgb,
      var(--color-error) 10%,
      var(--color-surface)
    );
    color: var(--color-foreground-subtle);
    font-size: 0.75rem;
  }

  .conflict-banner div {
    display: flex;
    flex: none;
    gap: var(--space-2);
  }

  .conflict-banner button {
    padding: 0.25rem var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-foreground);
    cursor: pointer;
    font-size: 0.6875rem;
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

  .editor-mode {
    margin-left: auto;
    color: var(--color-muted-dim);
    font-family: var(--font-family-mono);
    font-size: 0.6875rem;
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

  .editor-status.warning {
    color: var(--color-warning, #a66a00);
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

  .editor-host :global(.cm-editor.cm-focused) {
    box-shadow: inset 0 0 0 1px var(--color-focus);
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

  .editor-host :global(.cm-md-heading) {
    color: var(--color-editor-heading);
    font-family:
      Inter,
      ui-sans-serif,
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      sans-serif;
    font-weight: 700;
    letter-spacing: -0.018em;
  }

  .editor-host :global(.cm-md-heading-1) {
    font-size: 1.75em;
  }

  .editor-host :global(.cm-md-heading-2) {
    font-size: 1.45em;
  }

  .editor-host :global(.cm-md-heading-3) {
    font-size: 1.22em;
  }

  .editor-host :global(.cm-md-heading-4),
  .editor-host :global(.cm-md-heading-5),
  .editor-host :global(.cm-md-heading-6) {
    font-size: 1.05em;
  }

  .editor-host :global(.cm-md-heading-line) {
    padding-top: 0.42em;
    padding-bottom: 0.16em;
  }

  .editor-host :global(.cm-md-strong) {
    color: var(--color-foreground);
    font-weight: 700;
  }

  .editor-host :global(.cm-md-emphasis) {
    color: var(--color-foreground);
    font-style: italic;
  }

  .editor-host :global(.cm-md-inline-code) {
    padding: 0.08em 0.24em;
    border: 1px solid var(--color-editor-code-border);
    border-radius: 0.25rem;
    background: var(--color-editor-code);
    color: var(--color-editor-code-foreground);
  }

  .editor-host :global(.cm-md-link) {
    color: var(--color-editor-link);
    text-decoration-color: color-mix(
      in srgb,
      var(--color-editor-link) 48%,
      transparent
    );
    text-decoration-line: underline;
    text-underline-offset: 0.16em;
  }

  .editor-host :global(.cm-md-code-block-line) {
    background: var(--color-editor-code);
    color: var(--color-editor-code-foreground);
  }

  .editor-host :global(.cm-md-syntax) {
    color: var(--color-editor-syntax);
    opacity: 0.38;
    transition:
      color 80ms ease,
      opacity 80ms ease;
  }

  .editor-host :global(.cm-md-syntax-active) {
    color: var(--color-editor-syntax-active);
    opacity: 1;
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

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  @media (prefers-reduced-motion: reduce) {
    .editor-host :global(.cm-md-syntax) {
      transition: none;
    }
  }

  @media (forced-colors: active) {
    .editor-host :global(.cm-editor.cm-focused) {
      outline: 2px solid Highlight;
      outline-offset: -2px;
      box-shadow: none;
    }

    .editor-host :global(.cm-md-inline-code),
    .editor-host :global(.cm-md-code-block-line) {
      border-color: CanvasText;
      background: Canvas;
      color: CanvasText;
    }

    .editor-host :global(.cm-md-link),
    .editor-host :global(.cm-md-syntax-active) {
      color: LinkText;
    }

    .editor-host :global(.cm-md-syntax) {
      color: GrayText;
      opacity: 1;
    }
  }
</style>
