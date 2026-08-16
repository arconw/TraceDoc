export type WorkspaceView = 'editor' | 'map';

export interface UiPreferences {
  activeView: WorkspaceView;
  sidebarWidth: number;
}

export const defaultUiPreferences: UiPreferences = {
  activeView: 'editor',
  sidebarWidth: 256,
};

const storageKey = 'tracedoc.ui-preferences.v1';
const minimumSidebarWidth = 192;
const maximumSidebarWidth = 384;

export function sidebarMaximumWidth(availableWidth: number) {
  return Math.min(
    maximumSidebarWidth,
    Math.max(minimumSidebarWidth, availableWidth * 0.38),
  );
}

export function clampSidebarWidth(
  width: number,
  availableWidth = Number.POSITIVE_INFINITY,
) {
  return Math.min(
    sidebarMaximumWidth(availableWidth),
    Math.max(minimumSidebarWidth, width),
  );
}

export function adjustSidebarWidth(
  width: number,
  adjustment: 'decrease' | 'increase' | 'minimum' | 'maximum',
  availableWidth: number,
) {
  const target =
    adjustment === 'decrease'
      ? width - 16
      : adjustment === 'increase'
        ? width + 16
        : adjustment === 'minimum'
          ? minimumSidebarWidth
          : sidebarMaximumWidth(availableWidth);
  return clampSidebarWidth(target, availableWidth);
}

export function parseUiPreferences(value: string | null): UiPreferences {
  if (!value) return defaultUiPreferences;

  try {
    const candidate = JSON.parse(value) as Partial<UiPreferences>;
    return {
      activeView: candidate.activeView === 'map' ? 'map' : 'editor',
      sidebarWidth:
        typeof candidate.sidebarWidth === 'number' &&
        Number.isFinite(candidate.sidebarWidth)
          ? clampSidebarWidth(candidate.sidebarWidth)
          : defaultUiPreferences.sidebarWidth,
    };
  } catch {
    return defaultUiPreferences;
  }
}

export function loadUiPreferences(): UiPreferences {
  try {
    return parseUiPreferences(window.localStorage.getItem(storageKey));
  } catch {
    return defaultUiPreferences;
  }
}

export function saveUiPreferences(preferences: UiPreferences) {
  try {
    window.localStorage.setItem(storageKey, JSON.stringify(preferences));
  } catch {
    return;
  }
}
