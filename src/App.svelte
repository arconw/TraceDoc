<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { projectStore } from './lib/stores/project';
  import type { AppInfo } from './lib/types/app';
  import WorkspaceStatus from './lib/views/WorkspaceStatus.svelte';

  let appName = 'Simple Docs';

  onMount(async () => {
    try {
      const appInfo = await invoke<AppInfo>('get_app_info');
      appName = appInfo.name;
      document.title = `${appInfo.name} ${appInfo.version}`;
    } catch (error) {
      console.error('Unable to load application information', error);
    }
  });
</script>

<main class="app-shell">
  <header class="app-header">
    <h1>{appName}</h1>
    <button
      type="button"
      disabled={$projectStore.status === 'loading'}
      onclick={() => projectStore.openFolder()}
    >
      {$projectStore.status === 'loading' ? 'Opening…' : 'Open Folder'}
    </button>
  </header>
  <WorkspaceStatus />
</main>

<style>
  .app-shell {
    display: grid;
    min-height: 100vh;
    grid-template-rows: var(--header-height) 1fr;
    background: var(--color-background);
  }

  .app-header {
    display: flex;
    align-items: center;
    padding: 0 var(--space-5);
    justify-content: space-between;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  h1 {
    margin: 0;
    color: var(--color-foreground);
    font-size: var(--font-size-md);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  button {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-foreground);
    cursor: pointer;
    font-size: var(--font-size-sm);
  }

  button:hover:not(:disabled) {
    border-color: var(--color-foreground);
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
