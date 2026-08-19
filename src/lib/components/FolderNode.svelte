<script lang="ts">
  import type { DocumentId, FolderId, ProjectModel } from '../types/workspace';
  import DocumentRow from './DocumentRow.svelte';
  import FolderNode from './FolderNode.svelte';

  let {
    project,
    folderId,
    depth,
    expandedFolderIds,
    selectedDocumentId,
    onToggle,
    onSelectDocument,
  }: {
    project: ProjectModel;
    folderId: FolderId;
    depth: number;
    expandedFolderIds: ReadonlySet<FolderId>;
    selectedDocumentId: DocumentId | null;
    onToggle: (folderId: FolderId) => void;
    onSelectDocument: (documentId: DocumentId) => void;
  } = $props();

  let folder = $derived(project.folders[folderId]);
  let expanded = $derived(expandedFolderIds.has(folderId));

  function handleKeydown(event: KeyboardEvent) {
    if (
      (event.key === 'ArrowRight' && !expanded) ||
      (event.key === 'ArrowLeft' && expanded)
    ) {
      event.preventDefault();
      onToggle(folderId);
    }
  }
</script>

{#if folder}
  <li>
    <button
      type="button"
      class="folder-row"
      style:padding-left={`${8 + depth * 14}px`}
      title={folder.path || project.rootPath}
      aria-expanded={expanded}
      onclick={() => onToggle(folderId)}
      onkeydown={handleKeydown}
    >
      <span class:expanded class="chevron" aria-hidden="true">›</span>
      <svg class="folder-icon" viewBox="0 0 16 16" aria-hidden="true">
        <path d="M1.75 4.25h4l1.25 1.5h7.25v7.5H1.75z" />
        <path d="M1.75 4.25V2.75h4l1.25 1.5h7.25v1.5" />
      </svg>
      <span class="folder-name">{folder.name}</span>
    </button>

    {#if expanded}
      <ul>
        {#each folder.childFolderIds as childFolderId (childFolderId)}
          <FolderNode
            {project}
            folderId={childFolderId}
            depth={depth + 1}
            {expandedFolderIds}
            {selectedDocumentId}
            {onToggle}
            {onSelectDocument}
          />
        {/each}
        {#each folder.documentIds as documentId (documentId)}
          {@const document = project.documents[documentId]}
          {#if document}
            <DocumentRow
              {document}
              depth={depth + 1}
              selected={selectedDocumentId === documentId}
              onSelect={onSelectDocument}
            />
          {/if}
        {/each}
      </ul>
    {/if}
  </li>
{/if}

<style>
  li,
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .folder-row {
    display: flex;
    width: 100%;
    height: 1.75rem;
    align-items: center;
    gap: 0.3125rem;
    overflow: hidden;
    padding-top: 0;
    padding-right: var(--space-2);
    padding-bottom: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-foreground-subtle);
    cursor: default;
    font-size: 0.8125rem;
    font-weight: 500;
    line-height: 1;
    text-align: left;
  }

  .folder-row:hover {
    background: var(--color-surface-hover);
    color: var(--color-foreground);
  }

  .folder-row:focus-visible {
    outline: 1px solid var(--color-focus);
    outline-offset: -1px;
  }

  .chevron {
    width: 0.625rem;
    min-width: 0.625rem;
    color: var(--color-muted);
    font-size: 1rem;
    transform: rotate(0deg);
    transition: transform 100ms ease;
  }

  .chevron.expanded {
    transform: rotate(90deg);
  }

  .folder-icon {
    width: 0.875rem;
    min-width: 0.875rem;
    height: 0.875rem;
    fill: none;
    stroke: currentColor;
    stroke-linejoin: round;
    stroke-width: 1.1;
  }

  .folder-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (prefers-reduced-motion: reduce) {
    .chevron {
      transition: none;
    }
  }
</style>
