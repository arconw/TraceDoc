import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { writable } from 'svelte/store';
import type { ProjectModel } from '../types/workspace';

export type ProjectState =
  | { status: 'empty' }
  | { status: 'loading' }
  | { status: 'loaded'; project: ProjectModel }
  | { status: 'error'; message: string };

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function createProjectStore() {
  const { subscribe, set } = writable<ProjectState>({ status: 'empty' });

  return {
    subscribe,
    async openFolder() {
      let selectedPath: string | null;

      try {
        selectedPath = await open({
          directory: true,
          multiple: false,
          title: 'Open documentation folder',
        });
      } catch (error) {
        set({ status: 'error', message: errorMessage(error) });
        return;
      }

      if (selectedPath === null) {
        return;
      }

      set({ status: 'loading' });

      try {
        const project = await invoke<ProjectModel>('open_workspace', {
          rootPath: selectedPath,
        });
        set({ status: 'loaded', project });
      } catch (error) {
        set({ status: 'error', message: errorMessage(error) });
      }
    },
  };
}

export const projectStore = createProjectStore();
