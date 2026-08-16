<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { AppInfo } from './lib/types/app';
  import EmptyWorkspace from './lib/views/EmptyWorkspace.svelte';

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
  </header>
  <EmptyWorkspace />
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
</style>
