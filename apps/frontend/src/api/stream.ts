import { refreshAccessToken } from './fetcher';
import { currentAccessToken } from './fetcher';

/** Minimal SSE reader on top of fetch (EventSource cannot send an Authorization header). */
export async function apiFetchStream(url: string, signal: AbortSignal, onData: (data: string) => void) {
  let res = await fetch(url, { headers: bearer(), signal, credentials: 'same-origin' });
  if (res.status === 401 && (await refreshAccessToken())) {
    res = await fetch(url, { headers: bearer(), signal, credentials: 'same-origin' });
  }
  if (!res.ok || !res.body) throw new Error(`stream failed: ${res.status}`);
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  let buffer = '';
  for (;;) {
    const { value, done } = await reader.read();
    if (done) return;
    buffer += value;
    let idx: number;
    while ((idx = buffer.indexOf('\n\n')) >= 0) {
      const frame = buffer.slice(0, idx);
      buffer = buffer.slice(idx + 2);
      const data = frame
        .split('\n')
        .filter((l) => l.startsWith('data:'))
        .map((l) => l.slice(5).trimStart())
        .join('\n');
      if (data) onData(data);
    }
  }
}

function bearer(): HeadersInit {
  const t = currentAccessToken();
  return t ? { Authorization: `Bearer ${t}`, Accept: 'text/event-stream' } : { Accept: 'text/event-stream' };
}
