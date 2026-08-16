<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import LinkInspector from '../components/LinkInspector.svelte';
  import MarkdownEditor from '../components/MarkdownEditor.svelte';
  import Sidebar from '../components/Sidebar.svelte';
  import { projectStore } from '../stores/project';
  import type { DocumentId, ProjectModel } from '../types/workspace';
  import {
    adjustSidebarWidth,
    clampSidebarWidth,
    defaultUiPreferences,
    loadUiPreferences,
    saveUiPreferences,
    sidebarMaximumWidth,
    type WorkspaceView as WorkspaceViewName,
  } from '../utils/ui-preferences';
  import {
    beginLazyViewLoad,
    completeLazyViewLoad,
    createLazyViewState,
    failLazyViewLoad,
    lazyViewPresentation,
    lazyViewShouldLoad,
  } from '../utils/ui-behavior';

  type MapViewHandle = {
    fit: () => void;
  };

  type PendingAction =
    | { kind: 'document'; documentId: DocumentId; openEditor: boolean }
    | {
        kind: 'workspace';
      };

  export let project: ProjectModel;
  export let workspaceGeneration: number;
  export let workspaceRevision: number;
  export let selectedDocumentId: DocumentId | null;
  export let documentChangeVersions: Record<DocumentId, number>;
  export let watchError: string | null;

  let editor: MarkdownEditor | undefined;
  let mapView: MapViewHandle | undefined;
  let LoadedMapView: typeof import('./MapView.svelte').default | undefined;
  let mapLoadState = createLazyViewState();
  let decisionDialog: HTMLDialogElement;
  let saveButton: HTMLButtonElement;
  let pendingAction: PendingAction | null = null;
  let decisionBusy = false;
  let decisionError: string | null = null;
  let restoreFocus: HTMLElement | null = null;
  let workspaceResolver: ((proceed: boolean) => void) | null = null;
  let activeView: WorkspaceViewName = 'editor';
  let sidebarWidth = defaultUiPreferences.sidebarWidth;
  let sidebarMaxWidth = 384;
  let workspaceWidth = 1000;
  let resizeStartX = 0;
  let resizeStartWidth = 0;
  let workspaceHost: HTMLDivElement;
  let workspaceSizeObserver: ResizeObserver | null = null;

  $: selectedDocument = selectedDocumentId
    ? project.documents[selectedDocumentId]
    : null;
  $: pendingDocument =
    pendingAction?.kind === 'document'
      ? project.documents[pendingAction.documentId]
      : null;
  $: mapPresentation = lazyViewPresentation(activeView === 'map', mapLoadState);

  onDestroy(() => {
    workspaceResolver?.(false);
    stopSidebarResize();
    workspaceSizeObserver?.disconnect();
  });

  onMount(() => {
    const preferences = loadUiPreferences();
    activeView = preferences.activeView;
    updateWorkspaceWidth(workspaceHost.clientWidth || window.innerWidth);
    sidebarWidth = clampSidebarWidth(preferences.sidebarWidth, workspaceWidth);
    if (sidebarWidth !== preferences.sidebarWidth) persistPreferences();
    if (activeView === 'map') initializeMap();

    workspaceSizeObserver = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) updateWorkspaceWidth(entry.contentRect.width);
    });
    workspaceSizeObserver.observe(workspaceHost);

    const handleShortcut = (event: KeyboardEvent) => {
      if (
        pendingAction ||
        event.defaultPrevented ||
        event.altKey ||
        event.shiftKey ||
        (!event.ctrlKey && !event.metaKey)
      ) {
        return;
      }

      if (event.key === '1') {
        event.preventDefault();
        showView('editor');
      } else if (event.key === '2') {
        event.preventDefault();
        showView('map');
      } else if (event.key === '0') {
        event.preventDefault();
        void showMapAndFit();
      } else if (event.key.toLowerCase() === 'f' && activeView === 'editor') {
        event.preventDefault();
        editor?.find();
      }
    };

    window.addEventListener('keydown', handleShortcut);
    return () => window.removeEventListener('keydown', handleShortcut);
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
      if (openEditor) showView('editor');
      return;
    }

    if (editor?.isDirty()) {
      void openDecision({ kind: 'document', documentId, openEditor });
      return;
    }

    projectStore.selectDocument(documentId);
    if (openEditor) showView('editor');
  }

  function showView(view: WorkspaceViewName) {
    activeView = view;
    if (view === 'map') initializeMap();
    persistPreferences();
  }

  function initializeMap() {
    if (!lazyViewShouldLoad(mapLoadState)) return;
    startMapLoad();
  }

  function retryMapModule() {
    if (activeView !== 'map') return;
    startMapLoad();
  }

  function startMapLoad() {
    mapLoadState = beginLazyViewLoad(mapLoadState);
    const requestId = mapLoadState.requestId;
    void import('./MapView.svelte')
      .then((module) => {
        const next = completeLazyViewLoad(mapLoadState, requestId);
        if (next === mapLoadState) return;
        LoadedMapView = module.default;
        mapLoadState = next;
      })
      .catch((error) => {
        mapLoadState = failLazyViewLoad(
          mapLoadState,
          requestId,
          error instanceof Error ? error.message : String(error),
        );
      });
  }

  async function showMapAndFit() {
    showView('map');
    await tick();
    requestAnimationFrame(() => mapView?.fit());
  }

  function persistPreferences() {
    saveUiPreferences({ activeView, sidebarWidth });
  }

  function updateWorkspaceWidth(width: number) {
    workspaceWidth = width;
    sidebarMaxWidth = sidebarMaximumWidth(width);
    const nextWidth = clampSidebarWidth(sidebarWidth, width);
    if (nextWidth === sidebarWidth) return;
    sidebarWidth = nextWidth;
    persistPreferences();
  }

  function startSidebarResize(event: PointerEvent) {
    if (event.button !== 0) return;
    resizeStartX = event.clientX;
    resizeStartWidth = sidebarWidth;
    window.addEventListener('pointermove', resizeSidebar);
    window.addEventListener('pointerup', finishSidebarResize, { once: true });
    window.addEventListener('pointercancel', finishSidebarResize, {
      once: true,
    });
    document.body.classList.add('resizing-sidebar');
    event.preventDefault();
  }

  function resizeSidebar(event: PointerEvent) {
    sidebarWidth = clampSidebarWidth(
      resizeStartWidth + event.clientX - resizeStartX,
      workspaceWidth,
    );
  }

  function finishSidebarResize() {
    stopSidebarResize();
    persistPreferences();
  }

  function stopSidebarResize() {
    window.removeEventListener('pointermove', resizeSidebar);
    window.removeEventListener('pointerup', finishSidebarResize);
    window.removeEventListener('pointercancel', finishSidebarResize);
    document.body.classList.remove('resizing-sidebar');
  }

  function resizeSidebarWithKeyboard(event: KeyboardEvent) {
    const adjustment =
      event.key === 'ArrowLeft'
        ? 'decrease'
        : event.key === 'ArrowRight'
          ? 'increase'
          : event.key === 'Home'
            ? 'minimum'
            : event.key === 'End'
              ? 'maximum'
              : null;
    if (!adjustment) return;

    event.preventDefault();
    sidebarWidth = adjustSidebarWidth(sidebarWidth, adjustment, workspaceWidth);
    persistPreferences();
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
      if (action.openEditor) showView('editor');
    }

    if (action?.kind === 'workspace') resolver?.(proceed);

    requestAnimationFrame(() => {
      if (focusTarget?.isConnected) focusTarget.focus();
    });
  }
</script>

<div
  class="workspace"
  bind:this={workspaceHost}
  style={`--sidebar-width: ${sidebarWidth}px`}
>
  <Sidebar {project} {selectedDocumentId} onSelectDocument={requestDocument} />
  <div
    class="sidebar-resizer"
    role="slider"
    aria-label="Resize project explorer"
    aria-orientation="horizontal"
    aria-valuemin="192"
    aria-valuemax={sidebarMaxWidth}
    aria-valuenow={sidebarWidth}
    tabindex="0"
    onpointerdown={startSidebarResize}
    onkeydown={resizeSidebarWithKeyboard}
  ></div>
  <div class="workspace__content">
    <nav class="view-switcher" aria-label="Workspace view">
      <button
        type="button"
        class:active={activeView === 'editor'}
        aria-pressed={activeView === 'editor'}
        aria-keyshortcuts="Control+1 Meta+1"
        title="Editor (Ctrl/Cmd+1)"
        onclick={() => showView('editor')}
      >
        Editor
      </button>
      {#if watchError}
        <span class="watch-error" role="status" title={watchError}>
          Live updates paused · use Refresh Workspace
        </span>
      {/if}
      <button
        type="button"
        class:active={activeView === 'map'}
        aria-pressed={activeView === 'map'}
        aria-keyshortcuts="Control+2 Meta+2"
        title="Map (Ctrl/Cmd+2)"
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
        {selectedDocumentId}
        externalChangeVersion={selectedDocumentId
          ? (documentChangeVersions[selectedDocumentId] ?? 0)
          : 0}
        {workspaceGeneration}
        {workspaceRevision}
        saveShortcutEnabled={!pendingAction}
      />
      <LinkInspector {project} {selectedDocumentId} />
    </div>
    {#if mapLoadState.status === 'ready' && LoadedMapView}
      <div class="workspace__panel" hidden={activeView !== 'map'}>
        <LoadedMapView
          bind:this={mapView}
          {project}
          {selectedDocumentId}
          visible={activeView === 'map'}
          onOpenDocument={(documentId) => requestDocument(documentId, true)}
        />
      </div>
    {:else if mapPresentation === 'loading'}
      <div class="workspace__panel map-module-state" role="status">
        Loading map…
      </div>
    {:else if mapPresentation === 'error'}
      <div class="workspace__panel map-module-state" role="alert">
        <span>Unable to load map: {mapLoadState.message}</span>
        <button type="button" onclick={retryMapModule}>Retry</button>
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
    grid-template-columns: var(--sidebar-width) 0.3rem minmax(0, 1fr);
  }

  .sidebar-resizer {
    position: relative;
    z-index: 2;
    min-height: 0;
    padding: 0;
    border: 0;
    background: var(--color-surface);
    cursor: col-resize;
    touch-action: none;
  }

  .sidebar-resizer::after {
    position: absolute;
    inset: 0 auto 0 0.125rem;
    width: 1px;
    background: var(--color-border);
    content: '';
  }

  .sidebar-resizer:hover::after,
  .sidebar-resizer:focus-visible::after {
    background: var(--color-focus);
  }

  .sidebar-resizer:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: -2px;
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

  .watch-error {
    overflow: hidden;
    margin-left: auto;
    color: var(--color-error);
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .workspace__panel {
    min-width: 0;
    min-height: 0;
  }

  .workspace__panel[hidden] {
    display: none;
  }

  .map-module-state {
    display: grid;
    place-items: center;
    align-content: center;
    gap: var(--space-3);
    padding: var(--space-6);
    background: var(--color-map-background);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    text-align: center;
  }

  .map-module-state button {
    padding: 0.35rem var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-foreground);
    cursor: pointer;
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
