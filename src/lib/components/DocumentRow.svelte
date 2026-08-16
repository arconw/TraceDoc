<script lang="ts">
  import type {
    Document as WorkspaceDocument,
    DocumentId,
  } from '../types/workspace';

  export let document: WorkspaceDocument;
  export let depth: number;
  export let selected: boolean;
  export let onSelect: (documentId: DocumentId) => void;
</script>

<li>
  <button
    type="button"
    class:selected
    style:padding-left={`${8 + depth * 14}px`}
    title={document.path}
    aria-current={selected ? 'page' : undefined}
    onclick={() => onSelect(document.id)}
  >
    <svg class="document-icon" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M4 1.75h5l3 3V14.25H4z" />
      <path d="M9 1.75v3h3" />
    </svg>
    <span>{document.name}</span>
  </button>
</li>

<style>
  li {
    margin: 0;
  }

  button {
    display: flex;
    width: 100%;
    height: 1.75rem;
    align-items: center;
    gap: 0.375rem;
    overflow: hidden;
    padding-top: 0;
    padding-right: var(--space-2);
    padding-bottom: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-muted);
    cursor: default;
    font-size: 0.8125rem;
    line-height: 1;
    text-align: left;
  }

  button:hover {
    background: var(--color-surface-hover);
    color: var(--color-foreground);
  }

  button.selected {
    background: var(--color-selection);
    color: var(--color-selection-foreground);
  }

  button:focus-visible {
    outline: 1px solid var(--color-focus);
    outline-offset: -1px;
  }

  span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .document-icon {
    width: 0.875rem;
    min-width: 0.875rem;
    height: 0.875rem;
    fill: none;
    stroke: currentColor;
    stroke-linejoin: round;
    stroke-width: 1.1;
  }
</style>
