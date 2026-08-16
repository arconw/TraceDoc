import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { writable } from 'svelte/store';
import type {
  DocumentId,
  DocumentIndexUpdate,
  WorkspaceSnapshot,
} from '../types/workspace';
import { applyDocumentIndex, type ProjectState } from './project-state';

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
        const snapshot = await invoke<WorkspaceSnapshot>('open_workspace', {
          rootPath: selectedPath,
        });
        set({
          status: 'loaded',
          project: snapshot.project,
          workspaceGeneration: snapshot.workspaceGeneration,
          selectedDocumentId: null,
        });
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
    applyDocumentIndex(updateValue: DocumentIndexUpdate) {
      update((state) => applyDocumentIndex(state, updateValue));
    },
  };
}

export const projectStore = createProjectStore();
