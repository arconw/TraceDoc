<section class="empty-workspace" aria-labelledby="empty-workspace-title">
  <div class="empty-workspace__mark" aria-hidden="true"></div>
  <p id="empty-workspace-title">No folder open</p>
</section>

<style>
  .empty-workspace {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    color: var(--color-muted);
  }

  .empty-workspace__mark {
    width: var(--space-2);
    height: var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-full);
    background: var(--color-surface-raised);
  }

  p {
    margin: 0;
    font-size: var(--font-size-sm);
  }
</style>
