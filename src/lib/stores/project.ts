import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { writable } from 'svelte/store';
import type {
  DocumentId,
  DocumentIndexUpdate,
  WorkspaceEventError,
  WorkspaceSnapshot,
  WorkspacePatch,
} from '../types/workspace';
import {
  applyDocumentIndex,
  applyWorkspaceError,
  applyWorkspacePatch,
  applyWorkspaceSnapshot,
  closeSelectedDocument,
  completeWorkspaceOpen,
  type ProjectState,
} from './project-state';

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

      set({ status: 'loading', pendingPatches: [], pendingErrors: [] });

      try {
        const snapshot = await invoke<WorkspaceSnapshot>('open_workspace', {
          rootPath: selectedPath,
        });
        update((state) => completeWorkspaceOpen(state, snapshot));
      } catch (error) {
        set({ status: 'error', message: errorMessage(error) });
      }
    },
    selectDocument(documentId: DocumentId) {
      update((state) => {
        if (state.status !== 'loaded' || !state.project.documents[documentId]) {
          return state;
        }

        return {
          ...state,
          selectedDocumentId: documentId,
          documentChangeVersions: Object.fromEntries(
            Object.entries(state.documentChangeVersions).filter(
              ([id]) =>
                Boolean(state.project.documents[id]) || id === documentId,
            ),
          ),
        };
      });
    },
    closeDocument(documentId: DocumentId) {
      update((state) => closeSelectedDocument(state, documentId));
    },
    applyDocumentIndex(updateValue: DocumentIndexUpdate) {
      update((state) => applyDocumentIndex(state, updateValue));
    },
    async refreshWorkspace() {
      let generation: number | null = null;
      let revision: number | null = null;
      update((state) => {
        if (state.status === 'loaded') {
          generation = state.workspaceGeneration;
          revision = state.workspaceRevision;
        }
        return state;
      });
      if (generation === null) return;
      try {
        const snapshot = await invoke<WorkspaceSnapshot>('refresh_workspace', {
          workspaceGeneration: generation,
        });
        update((state) => applyWorkspaceSnapshot(state, snapshot));
      } catch (error) {
        update((state) =>
          generation === null
            ? state
            : applyWorkspaceError(state, {
                workspaceGeneration: generation,
                workspaceRevision: revision ?? 0,
                message: errorMessage(error),
              }),
        );
      }
    },
    async listenForWorkspaceUpdates(): Promise<UnlistenFn> {
      const unlistenPatch = await listen<WorkspacePatch>(
        'workspace-patch',
        (event) => update((state) => applyWorkspacePatch(state, event.payload)),
      );
      let unlistenError: UnlistenFn;
      try {
        unlistenError = await listen<WorkspaceEventError>(
          'workspace-watch-error',
          (event) =>
            update((state) => applyWorkspaceError(state, event.payload)),
        );
      } catch (error) {
        unlistenPatch();
        throw error;
      }
      return () => {
        unlistenPatch();
        unlistenError();
      };
    },
  };
}

export const projectStore = createProjectStore();
