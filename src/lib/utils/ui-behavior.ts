export type LazyViewStatus = 'idle' | 'loading' | 'ready' | 'error';

export interface LazyViewState {
  requestId: number;
  status: LazyViewStatus;
  message: string | null;
}

export function createLazyViewState(): LazyViewState {
  return { requestId: 0, status: 'idle', message: null };
}

export function beginLazyViewLoad(state: LazyViewState): LazyViewState {
  return {
    requestId: state.requestId + 1,
    status: 'loading',
    message: null,
  };
}

export function completeLazyViewLoad(
  state: LazyViewState,
  requestId: number,
): LazyViewState {
  return requestId === state.requestId
    ? { ...state, status: 'ready', message: null }
    : state;
}

export function failLazyViewLoad(
  state: LazyViewState,
  requestId: number,
  message: string,
): LazyViewState {
  return requestId === state.requestId
    ? { ...state, status: 'error', message }
    : state;
}

export function lazyViewPresentation(
  active: boolean,
  state: LazyViewState,
): 'hidden' | 'loading' | 'ready-hidden' | 'ready-visible' | 'error' {
  if (state.status === 'ready') {
    return active ? 'ready-visible' : 'ready-hidden';
  }
  if (!active || state.status === 'idle') return 'hidden';
  return state.status;
}

export function lazyViewShouldLoad(state: LazyViewState) {
  return state.status === 'idle';
}

export interface SaveShortcutModifiers {
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  key: string;
}

export function saveShortcutAction(
  event: SaveShortcutModifiers,
  enabled: boolean,
): 'ignore' | 'block' | 'save' {
  if (
    event.altKey ||
    (!event.ctrlKey && !event.metaKey) ||
    event.key.toLowerCase() !== 's'
  ) {
    return 'ignore';
  }
  return event.shiftKey || !enabled ? 'block' : 'save';
}

export function mapFitDuration(reducedMotion: boolean) {
  return reducedMotion ? 0 : 180;
}
