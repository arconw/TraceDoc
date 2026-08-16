export interface MapViewport {
  x: number;
  y: number;
  zoom: number;
}

export interface SavedMapViewport {
  graphSignature: string;
  layoutRevision: number;
  viewport: MapViewport;
}

export type MapViewportMountAction =
  { kind: 'fit' } | { kind: 'restore'; viewport: MapViewport };

export function validMapViewport(viewport: MapViewport) {
  return (
    Number.isFinite(viewport.x) &&
    Number.isFinite(viewport.y) &&
    Number.isFinite(viewport.zoom) &&
    viewport.zoom > 0
  );
}

export function captureMapViewport(
  graphSignature: string,
  layoutRevision: number,
  viewport: MapViewport,
): SavedMapViewport | null {
  return validMapViewport(viewport)
    ? { graphSignature, layoutRevision, viewport: { ...viewport } }
    : null;
}

export function mapViewportMountAction(
  saved: SavedMapViewport | null,
  graphSignature: string,
  layoutRevision: number,
  explicitFit: boolean,
): MapViewportMountAction {
  return !explicitFit &&
    saved?.graphSignature === graphSignature &&
    saved.layoutRevision === layoutRevision &&
    validMapViewport(saved.viewport)
    ? { kind: 'restore', viewport: { ...saved.viewport } }
    : { kind: 'fit' };
}

export function mapViewportRequestIsCurrent(
  requestMountVersion: number,
  requestLayoutRevision: number,
  currentMountVersion: number,
  currentLayoutRevision: number,
  visible: boolean,
) {
  return (
    visible &&
    requestMountVersion === currentMountVersion &&
    requestLayoutRevision === currentLayoutRevision
  );
}
