<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import LinkInspector from '../components/LinkInspector.svelte';
  import MarkdownEditor from '../components/MarkdownEditor.svelte';
  import Sidebar from '../components/Sidebar.svelte';
  import { projectStore } from '../stores/project';
  import type { DocumentId, ProjectModel } from '../types/workspace';
  import MapView from './MapView.svelte';

  type PendingAction =
    | { kind: 'document'; documentId: DocumentId; openEditor: boolean }
    | {
        kind: 'workspace';
      };

  export let project: ProjectModel;
  export let workspaceGeneration: number;
  export let selectedDocumentId: DocumentId | null;

  let editor: MarkdownEditor | undefined;
  let decisionDialog: HTMLDialogElement;
  let saveButton: HTMLButtonElement;
  let pendingAction: PendingAction | null = null;
  let decisionBusy = false;
  let decisionError: string | null = null;
  let restoreFocus: HTMLElement | null = null;
  let workspaceResolver: ((proceed: boolean) => void) | null = null;
  let activeView: 'editor' | 'map' = 'editor';
  let mapInitialized = false;

  $: selectedDocument = selectedDocumentId
    ? project.documents[selectedDocumentId]
    : null;
  $: pendingDocument =
    pendingAction?.kind === 'document'
      ? project.documents[pendingAction.documentId]
      : null;

  onDestroy(() => {
    workspaceResolver?.(false);
  });

  export function isDirty() {
    return editor?.isDirty() ?? false;
  }

  export function prepareWorkspaceChange(): Promise<boolean> {
    if (!editor?.isDirty()) return Promise.resolve(true);
    if (pendingAction) return Promise.resolve(false);

    return new Promise((resolve) => {
      workspaceResolver = resolve;
      void openDecision({ kind: 'workspace' });
    });
  }

  function requestDocument(documentId: DocumentId, openEditor = false) {
    if (pendingAction) return;

    if (documentId === selectedDocumentId) {
      if (openEditor) activeView = 'editor';
      return;
    }

    if (editor?.isDirty()) {
      void openDecision({ kind: 'document', documentId, openEditor });
      return;
    }

    projectStore.selectDocument(documentId);
    if (openEditor) activeView = 'editor';
  }

  function showView(view: 'editor' | 'map') {
    activeView = view;
    if (view === 'map') mapInitialized = true;
  }

  async function openDecision(action: PendingAction) {
    pendingAction = action;
    decisionError = null;
    restoreFocus =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    await tick();

    if (!pendingAction || decisionDialog.open) return;
    decisionDialog.showModal();
    saveButton.focus();
  }

  async function saveAndContinue() {
    if (!pendingAction || !editor) return;
    decisionBusy = true;
    decisionError = null;
    const saved = await editor.save();
    decisionBusy = false;

    if (!saved) {
      decisionError =
        editor.getSaveError() ??
        'The document could not be saved. Your changes remain open.';
      return;
    }

    if (editor.isDirty()) {
      decisionError =
        'The document changed while saving. Save the remaining changes before continuing.';
      return;
    }

    finishDecision(true);
  }

  function discardAndContinue() {
    if (decisionBusy) return;
    finishDecision(true);
  }

  function cancelDecision() {
    if (decisionBusy) return;
    finishDecision(false);
  }

  function handleDialogCancel(event: Event) {
    event.preventDefault();
    cancelDecision();
  }

  function finishDecision(proceed: boolean) {
    const action = pendingAction;
    const resolver = workspaceResolver;
    const focusTarget = restoreFocus;
    pendingAction = null;
    workspaceResolver = null;
    restoreFocus = null;
    decisionError = null;

    if (decisionDialog.open) decisionDialog.close();

    if (proceed && action?.kind === 'document') {
      projectStore.selectDocument(action.documentId);
      if (action.openEditor) activeView = 'editor';
    }

    if (action?.kind === 'workspace') resolver?.(proceed);

    requestAnimationFrame(() => {
      if (focusTarget?.isConnected) focusTarget.focus();
    });
  }
</script>

<div class="workspace">
  <Sidebar {project} {selectedDocumentId} onSelectDocument={requestDocument} />
  <div class="workspace__content">
    <nav class="view-switcher" aria-label="Workspace view">
      <button
        type="button"
        class:active={activeView === 'editor'}
        aria-pressed={activeView === 'editor'}
        onclick={() => showView('editor')}
      >
        Editor
      </button>
      <button
        type="button"
        class:active={activeView === 'map'}
        aria-pressed={activeView === 'map'}
        onclick={() => showView('map')}
      >
        Map
      </button>
    </nav>
    <div
      class="workspace__panel workspace__editor"
      hidden={activeView !== 'editor'}
    >
      <MarkdownEditor
        bind:this={editor}
        document={selectedDocument}
        {workspaceGeneration}
      />
      <LinkInspector {project} {selectedDocumentId} />
    </div>
    {#if mapInitialized}
      <div class="workspace__panel" hidden={activeView !== 'map'}>
        <MapView
          {project}
          {selectedDocumentId}
          visible={activeView === 'map'}
          onOpenDocument={(documentId) => requestDocument(documentId, true)}
        />
      </div>
    {/if}
  </div>
</div>

<dialog
  bind:this={decisionDialog}
  class="switch-dialog"
  aria-labelledby="switch-dialog-title"
  aria-describedby="switch-dialog-description"
  oncancel={handleDialogCancel}
>
  {#if pendingAction}
    <h2 id="switch-dialog-title">Save changes?</h2>
    <p id="switch-dialog-description">
      {#if pendingDocument}
        Save changes before opening <strong>{pendingDocument.name}</strong>?
      {:else}
        Save changes before opening another folder?
      {/if}
    </p>
    {#if decisionError}
      <p class="switch-dialog__error" role="alert">{decisionError}</p>
    {/if}
    <div class="switch-dialog__actions">
      <button type="button" disabled={decisionBusy} onclick={cancelDecision}>
        Cancel
      </button>
      <button
        type="button"
        disabled={decisionBusy}
        onclick={discardAndContinue}
      >
        Discard
      </button>
      <button
        type="button"
        class="primary"
        bind:this={saveButton}
        disabled={decisionBusy}
        onclick={saveAndContinue}
      >
        {decisionBusy ? 'Saving…' : 'Save'}
      </button>
    </div>
  {/if}
</dialog>

<style>
  .workspace {
    display: grid;
    min-width: 0;
    min-height: 0;
    grid-template-columns: clamp(13rem, 25vw, 18rem) minmax(0, 1fr);
  }

  .workspace__content {
    display: grid;
    min-width: 0;
    min-height: 0;
    grid-template-rows: 2.5rem minmax(0, 1fr);
  }

  .view-switcher {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .view-switcher button {
    min-width: 0;
    padding: 0.3rem var(--space-3);
    border-color: transparent;
    background: transparent;
    color: var(--color-muted);
    font-weight: 550;
  }

  .view-switcher button:hover {
    border-color: var(--color-border);
    background: var(--color-surface-hover);
    color: var(--color-foreground-subtle);
  }

  .view-switcher button.active {
    border-color: var(--color-border-strong);
    background: var(--color-surface-raised);
    color: var(--color-foreground);
  }

  .workspace__panel {
    min-width: 0;
    min-height: 0;
  }

  .workspace__panel[hidden] {
    display: none;
  }

  .workspace__editor {
    display: grid;
    grid-template-rows: minmax(0, 1fr) 10.5rem;
  }

  .switch-dialog {
    width: min(25rem, calc(100% - 2.5rem));
    max-width: none;
    padding: var(--space-5);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-foreground);
    box-shadow: 0 1rem 3rem rgb(0 0 0 / 35%);
  }

  .switch-dialog[open] {
    display: grid;
    gap: var(--space-3);
  }

  .switch-dialog::backdrop {
    background: rgb(5 7 10 / 68%);
  }

  h2,
  p {
    margin: 0;
  }

  h2 {
    color: var(--color-foreground);
    font-size: var(--font-size-md);
  }

  p {
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }

  strong {
    color: var(--color-foreground-subtle);
  }

  .switch-dialog__error {
    padding: var(--space-2) var(--space-3);
    border: 1px solid color-mix(in srgb, var(--color-error) 45%, transparent);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    color: var(--color-error);
  }

  .switch-dialog__actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  button {
    min-width: 4.5rem;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-foreground);
    cursor: pointer;
    font-size: 0.75rem;
  }

  button.primary {
    border-color: var(--color-focus);
    background: var(--color-selection);
  }

  button:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }

  button:disabled {
    cursor: wait;
    opacity: 0.65;
  }
</style>
