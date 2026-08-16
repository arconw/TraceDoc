<script lang="ts">
  import type { DocumentId, ProjectModel } from '../types/workspace';

  export let project: ProjectModel;
  export let selectedDocumentId: DocumentId | null;

  $: selectedDocument = selectedDocumentId
    ? project.documents[selectedDocumentId]
    : null;
  $: outgoingLinks = selectedDocumentId
    ? project.links.filter(
        (link) => link.sourceDocumentId === selectedDocumentId,
      )
    : [];
  $: resolvedLinks = outgoingLinks.filter((link) => link.resolved);
  $: unresolvedLinks = outgoingLinks.filter((link) => !link.resolved);

  function targetLabel(targetDocumentId: DocumentId | null) {
    if (!targetDocumentId) return 'Unknown target';
    return project.documents[targetDocumentId]?.path ?? targetDocumentId;
  }
</script>

<aside class="inspector" aria-label="Markdown index debug panel">
  <header>
    <span>Markdown index</span>
    {#if selectedDocument}
      <strong>{selectedDocument.title ?? selectedDocument.name}</strong>
    {/if}
  </header>

  {#if selectedDocument}
    <div class="inspector__content">
      <section>
        <h2>Title</h2>
        <p class="inspector__title">
          {selectedDocument.title ?? selectedDocument.name}
        </p>
      </section>

      <section>
        <h2>Headings</h2>
        {#if selectedDocument.headings.length > 0}
          <ul>
            {#each selectedDocument.headings as heading, index (`heading:${index}`)}
              <li><span>H{heading.level}</span>{heading.text || '(empty)'}</li>
            {/each}
          </ul>
        {:else}
          <p>None</p>
        {/if}
      </section>

      <section>
        <h2>Outgoing links</h2>
        {#if outgoingLinks.length > 0}
          <ul>
            {#each outgoingLinks as link (link.id)}
              <li><code>{link.rawTarget}</code></li>
            {/each}
          </ul>
        {:else}
          <p>None</p>
        {/if}
      </section>

      <section>
        <h2>Resolved targets</h2>
        {#if resolvedLinks.length > 0}
          <ul>
            {#each resolvedLinks as link (link.id)}
              <li>
                <code>{link.rawTarget}</code>
                <span>→ {targetLabel(link.targetDocumentId)}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p>None</p>
        {/if}
      </section>

      <section>
        <h2>Unresolved links</h2>
        {#if unresolvedLinks.length > 0}
          <ul>
            {#each unresolvedLinks as link (link.id)}
              <li>
                <code>{link.rawTarget}</code>
                <span>{link.unresolvedReason ?? 'Unresolved'}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p>None</p>
        {/if}
      </section>
    </div>
  {:else}
    <p class="inspector__empty">Select a document to inspect its metadata.</p>
  {/if}
</aside>

<style>
  .inspector {
    display: grid;
    min-width: 0;
    min-height: 0;
    grid-template-rows: auto minmax(0, 1fr);
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  header {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    color: var(--color-muted);
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  header strong {
    overflow: hidden;
    color: var(--color-foreground-subtle);
    font-size: 0.75rem;
    letter-spacing: 0;
    text-overflow: ellipsis;
    text-transform: none;
    white-space: nowrap;
  }

  .inspector__content {
    display: grid;
    min-height: 0;
    grid-template-columns: repeat(5, minmax(10rem, 1fr));
    gap: var(--space-4);
    overflow: auto;
    padding: 0 var(--space-4) var(--space-3);
    scrollbar-color: var(--color-border-strong) transparent;
    scrollbar-width: thin;
  }

  section {
    min-width: 0;
  }

  h2 {
    margin: 0 0 var(--space-2);
    color: var(--color-muted-dim);
    font-size: 0.6875rem;
    font-weight: 600;
  }

  ul {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    min-width: 0;
    gap: var(--space-2);
    color: var(--color-foreground-subtle);
    font-size: 0.6875rem;
    line-height: 1.35;
  }

  li span,
  code {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  li > span:first-child {
    color: var(--color-muted-dim);
  }

  code {
    color: var(--color-focus);
    font-family: var(--font-family-mono);
  }

  p {
    margin: 0;
    color: var(--color-muted-dim);
    font-size: 0.6875rem;
  }

  .inspector__title {
    color: var(--color-foreground-subtle);
  }

  .inspector__empty {
    padding: 0 var(--space-4) var(--space-3);
  }
</style>
