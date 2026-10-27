'use client';

import { useEffect, useRef } from 'react';
import { apiFetchStream } from '../api/stream';

export type ServerEvent = {
  type: 'notification';
  id: string;
  kind: string;
  message: string;
  workspace_id: string | null;
  card_id: string | null;
  created_at: string;
};

/** Subscribes to the caller's own realtime stream (SSE over fetch, so the bearer token can be sent). */
export function useServerEvents(onEvent: (e: ServerEvent) => void) {
  const handler = useRef(onEvent);
  useEffect(() => {
    handler.current = onEvent;
  });

  useEffect(() => {
    const ctrl = new AbortController();
    let delay = 1000;
    (async () => {
      while (!ctrl.signal.aborted) {
        try {
          await apiFetchStream('/api/v1/events', ctrl.signal, (data) => {
            delay = 1000;
            try {
              handler.current(JSON.parse(data) as ServerEvent);
            } catch {
              /* ignore malformed frames */
            }
          });
        } catch {
          /* fall through to reconnect */
        }
        if (ctrl.signal.aborted) return;
        await new Promise((r) => setTimeout(r, delay));
        delay = Math.min(delay * 2, 30000);
      }
    })();
    return () => ctrl.abort();
  }, []);
}
