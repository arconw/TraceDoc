import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { writable } from 'svelte/store';
import type { DocumentId, ProjectModel } from '../types/workspace';

export type ProjectState =
  | { status: 'empty' }
  | { status: 'loading' }
  | {
      status: 'loaded';
      project: ProjectModel;
      selectedDocumentId: DocumentId | null;
    }
  | { status: 'error'; message: string };

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function createProjectStore() {
  const { subscribe, set, update } = writable<ProjectState>({
    status: 'empty',
  });

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
        set({ status: 'loaded', project, selectedDocumentId: null });
      } catch (error) {
        set({ status: 'error', message: errorMessage(error) });
      }
    },
    selectDocument(documentId: DocumentId) {
      update((state) => {
        if (state.status !== 'loaded' || !state.project.documents[documentId]) {
          return state;
        }

        return { ...state, selectedDocumentId: documentId };
      });
    },
  };
}

export const projectStore = createProjectStore();
