<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity';
  import type { DocumentId, FolderId, ProjectModel } from '../types/workspace';
  import FolderNode from './FolderNode.svelte';

  export let project: ProjectModel;
  export let selectedDocumentId: DocumentId | null;
  export let onSelectDocument: (documentId: DocumentId) => void;

  let initializedRootPath: string | null = null;
  const expandedFolderIds = new SvelteSet<FolderId>();

  $: rootFolder = Object.values(project.folders).find(
    (folder) => folder.parentId === null,
  );
  $: if (project.rootPath !== initializedRootPath) {
    initializedRootPath = project.rootPath;
    expandedFolderIds.clear();

    if (rootFolder) {
      expandedFolderIds.add(rootFolder.id);
    }
  }

  function toggleFolder(folderId: FolderId) {
    if (expandedFolderIds.has(folderId)) {
      expandedFolderIds.delete(folderId);
    } else {
      expandedFolderIds.add(folderId);
    }
  }
</script>

<aside class="sidebar" aria-label="Project explorer">
  <header>
    <span class="sidebar__label">Project</span>
    <span class="sidebar__path" title={project.rootPath}
      >{project.rootPath}</span
    >
  </header>

  <div class="sidebar__tree-scroll">
    {#if rootFolder}
      <ul class="sidebar__tree">
        <FolderNode
          {project}
          folderId={rootFolder.id}
          depth={0}
          {expandedFolderIds}
          {selectedDocumentId}
          onToggle={toggleFolder}
          {onSelectDocument}
        />
      </ul>

      {#if Object.keys(project.documents).length === 0}
        <p class="sidebar__empty">No Markdown documents</p>
      {/if}
    {:else}
      <p class="sidebar__empty">Workspace tree unavailable</p>
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    display: grid;
    min-width: 0;
    min-height: 0;
    grid-template-rows: auto minmax(0, 1fr);
    border-right: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  header {
    display: grid;
    gap: 0.1875rem;
    padding: var(--space-3) var(--space-3) var(--space-2);
  }

  .sidebar__label {
    color: var(--color-muted);
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.09em;
    text-transform: uppercase;
  }

  .sidebar__path {
    overflow: hidden;
    color: var(--color-muted-dim);
    font-family: var(--font-family-mono);
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sidebar__tree-scroll {
    min-height: 0;
    overflow: auto;
    padding: var(--space-1) var(--space-2) var(--space-3);
    scrollbar-color: var(--color-border-strong) transparent;
    scrollbar-width: thin;
  }

  .sidebar__tree {
    min-width: max-content;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .sidebar__empty {
    margin: var(--space-3) var(--space-2);
    color: var(--color-muted-dim);
    font-size: 0.75rem;
    line-height: 1.4;
  }
</style>
