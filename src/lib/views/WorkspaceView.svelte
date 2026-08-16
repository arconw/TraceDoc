<script lang="ts">
  import Sidebar from '../components/Sidebar.svelte';
  import { projectStore } from '../stores/project';
  import type { DocumentId, ProjectModel } from '../types/workspace';

  export let project: ProjectModel;
  export let selectedDocumentId: DocumentId | null;

  $: selectedDocument = selectedDocumentId
    ? project.documents[selectedDocumentId]
    : null;
</script>

<div class="workspace">
  <Sidebar
    {project}
    {selectedDocumentId}
    onSelectDocument={(documentId) => projectStore.selectDocument(documentId)}
  />

  <section class="document-placeholder" aria-live="polite">
    {#if selectedDocument}
      <div class="document-placeholder__selection">
        <span>Selected document</span>
        <h2>{selectedDocument.name}</h2>
        <code>{selectedDocument.path}</code>
      </div>
    {:else}
      <div class="document-placeholder__empty">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M6 3.5h8l4 4v13H6z" />
          <path d="M14 3.5v4h4M9 12h6M9 15.5h6" />
        </svg>
        <p>Select a Markdown document</p>
        <span>Its path will appear here.</span>
      </div>
    {/if}
  </section>
</div>

<style>
  .workspace {
    display: grid;
    min-width: 0;
    min-height: 0;
    grid-template-columns: clamp(13rem, 25vw, 18rem) minmax(0, 1fr);
  }

  .document-placeholder {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: center;
    padding: var(--space-6);
  }

  .document-placeholder__selection,
  .document-placeholder__empty {
    display: grid;
    max-width: min(38rem, 100%);
    justify-items: center;
    gap: var(--space-2);
    text-align: center;
  }

  .document-placeholder__selection > span {
    color: var(--color-muted);
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.09em;
    text-transform: uppercase;
  }

  h2,
  p {
    margin: 0;
  }

  h2 {
    color: var(--color-foreground);
    font-size: var(--font-size-lg);
    font-weight: 600;
  }

  code {
    max-width: 100%;
    overflow-wrap: anywhere;
    color: var(--color-muted);
    font-family: var(--font-family-mono);
    font-size: 0.8125rem;
  }

  .document-placeholder__empty {
    color: var(--color-muted);
  }

  .document-placeholder__empty svg {
    width: 2rem;
    height: 2rem;
    margin-bottom: var(--space-1);
    fill: none;
    stroke: var(--color-border-strong);
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-width: 1.2;
  }

  .document-placeholder__empty p {
    color: var(--color-foreground-subtle);
    font-size: var(--font-size-sm);
    font-weight: 500;
  }

  .document-placeholder__empty span {
    color: var(--color-muted-dim);
    font-size: 0.75rem;
  }
</style>
