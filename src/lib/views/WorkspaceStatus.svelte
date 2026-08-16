<script lang="ts">
  import { projectStore } from '../stores/project';
</script>

<section class="workspace-status" aria-live="polite">
  {#if $projectStore.status === 'empty'}
    <div class="workspace-status__empty">
      <span class="workspace-status__mark" aria-hidden="true"></span>
      <p>No folder open</p>
    </div>
  {:else if $projectStore.status === 'loading'}
    <p>Opening workspace…</p>
  {:else if $projectStore.status === 'error'}
    <div class="workspace-status__error">
      <p>Unable to open workspace</p>
      <span>{$projectStore.message}</span>
    </div>
  {:else}
    <dl>
      <div>
        <dt>Workspace</dt>
        <dd>{$projectStore.project.rootPath}</dd>
      </div>
      <div>
        <dt>Folders</dt>
        <dd>{Object.keys($projectStore.project.folders).length}</dd>
      </div>
      <div>
        <dt>Documents</dt>
        <dd>{Object.keys($projectStore.project.documents).length}</dd>
      </div>
    </dl>
  {/if}
</section>

<style>
  .workspace-status {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-6);
    color: var(--color-muted);
  }

  .workspace-status__empty {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .workspace-status__mark {
    width: var(--space-2);
    height: var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-full);
    background: var(--color-surface-raised);
  }

  .workspace-status__error {
    display: grid;
    max-width: 36rem;
    gap: var(--space-2);
    text-align: center;
  }

  p,
  dl,
  dd {
    margin: 0;
  }

  p,
  dt {
    font-size: var(--font-size-sm);
  }

  .workspace-status__error p,
  dt {
    color: var(--color-foreground);
    font-weight: 600;
  }

  .workspace-status__error span {
    color: var(--color-error);
    font-size: var(--font-size-sm);
  }

  dl {
    display: grid;
    min-width: min(32rem, 100%);
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  dl div {
    display: grid;
    grid-template-columns: 7rem minmax(0, 1fr);
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
  }

  dl div + div {
    border-top: 1px solid var(--color-border);
  }

  dd {
    overflow: hidden;
    color: var(--color-foreground);
    font-size: var(--font-size-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
