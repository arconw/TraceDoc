export interface MapEdgeTraceState {
  pointerHoveredEdgeId: string | null;
  focusedEdgeId: string | null;
}

export type MapEdgeTraceEvent = {
  source: 'pointer' | 'focus';
  edgeId: string | null;
};

export function createMapEdgeTraceState(): MapEdgeTraceState {
  return { pointerHoveredEdgeId: null, focusedEdgeId: null };
}

export function reduceMapEdgeTrace(
  state: MapEdgeTraceState,
  event: MapEdgeTraceEvent,
): MapEdgeTraceState {
  return event.source === 'pointer'
    ? { ...state, pointerHoveredEdgeId: event.edgeId }
    : { ...state, focusedEdgeId: event.edgeId };
}

export function effectiveMapEdgeTraceId(state: MapEdgeTraceState) {
  return state.pointerHoveredEdgeId ?? state.focusedEdgeId;
}

export type MapLayoutStatus = 'loading' | 'ready' | 'empty' | 'error';

export interface MapLayoutSession<Layout> {
  requestId: number;
  status: MapLayoutStatus;
  layout: Layout | null;
  message: string | null;
}

export function createMapLayoutSession<Layout>(): MapLayoutSession<Layout> {
  return { requestId: 0, status: 'loading', layout: null, message: null };
}

export function beginMapLayout<Layout>(
  session: MapLayoutSession<Layout>,
  status: 'loading' | 'empty',
): MapLayoutSession<Layout> {
  return {
    requestId: session.requestId + 1,
    status,
    layout: null,
    message: null,
  };
}

export function completeMapLayout<Layout>(
  session: MapLayoutSession<Layout>,
  requestId: number,
  layout: Layout,
): MapLayoutSession<Layout> {
  return requestId === session.requestId
    ? { ...session, status: 'ready', layout, message: null }
    : session;
}

export function completeEmptyMapLayout<Layout>(
  session: MapLayoutSession<Layout>,
  requestId: number,
): MapLayoutSession<Layout> {
  return requestId === session.requestId
    ? { ...session, status: 'empty', layout: null, message: null }
    : session;
}

export function failMapLayout<Layout>(
  session: MapLayoutSession<Layout>,
  requestId: number,
  message: string,
): MapLayoutSession<Layout> {
  return requestId === session.requestId
    ? { ...session, status: 'error', layout: null, message }
    : session;
}

export function mapLayoutIsInteractive<Layout>(
  session: MapLayoutSession<Layout>,
  visible: boolean,
  flowReady: boolean,
) {
  return session.status === 'ready' && visible && flowReady;
}
