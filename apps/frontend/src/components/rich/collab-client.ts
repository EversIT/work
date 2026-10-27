import { getSchema, type JSONContent } from '@tiptap/core';
import { prosemirrorJSONToYDoc } from '@tiptap/y-tiptap';
import * as Y from 'yjs';
import { apiFetchStream } from '../../api/stream';
import { syncCollab } from '../../api/generated/cards/cards';
import { ApiError } from '../../api/fetcher';
import { richExtensions } from '../../lib/rich';

/** The document is stored in this Yjs fragment; the editor's Collaboration extension reads it. */
export const FRAGMENT = 'default';

export type CollabStatus = 'connecting' | 'live' | 'offline';

const b64 = (bytes: Uint8Array) => {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000)
    s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
};
const unb64 = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));

/**
 * Keeps one Yjs document in step with the server: it pushes local changes (batched), pulls what is
 * missing, and follows the other editors through a server-sent event stream. All traffic is plain
 * HTTPS requests with the normal bearer token, so no extra connection type is needed.
 *
 * `onReset` fires when the field was changed outside the editor: the session is over and the
 * caller should start a new one from the card's current content.
 */
export class CollabClient {
  readonly doc = new Y.Doc();
  private pending: Uint8Array[] = [];
  private pushing = false;
  private pushTimer: ReturnType<typeof setTimeout> | undefined;
  private stopped = false;
  private abort = new AbortController();
  private unsaved = false;

  constructor(
    private readonly cardId: string,
    private readonly slug: string,
    private readonly seedJson: () => JSONContent,
    private readonly hooks: {
      onStatus: (s: CollabStatus) => void;
      onReset: () => void;
      onLocalChange: () => void;
    },
  ) {
    this.doc.on('update', (update: Uint8Array, origin: unknown) => {
      if (origin === 'remote') return;
      this.pending.push(update);
      this.unsaved = true;
      this.hooks.onLocalChange();
      this.schedulePush();
    });
  }

  /** Are there edits made here that the card's saved data has not seen yet? */
  hasUnsavedEdits(): boolean {
    return this.unsaved;
  }

  /** Marks the edits as handed over for saving; pass `true` again if saving failed. */
  setUnsaved(value: boolean) {
    this.unsaved = value;
  }

  /** Resolves once the document holds the server's state; the editor can be created then. */
  async start(): Promise<void> {
    let res = await this.sync({
      state_vector: b64(Y.encodeStateVector(this.doc)),
    });
    if (res.empty) {
      // First editor: seed from the card's content. A concurrent seed from someone else wins and
      // ours is ignored by the server, so the text never doubles.
      const seed = prosemirrorJSONToYDoc(
        getSchema(richExtensions(true)),
        this.seedJson(),
        FRAGMENT,
      );
      res = await this.sync({
        seed: b64(Y.encodeStateAsUpdate(seed)),
        state_vector: b64(Y.encodeStateVector(this.doc)),
      });
    }
    this.apply(res.update);
    this.hooks.onStatus('live');
    void this.follow();
  }

  stop() {
    this.stopped = true;
    this.abort.abort();
    clearTimeout(this.pushTimer);
    this.doc.destroy();
  }

  private apply(update: string) {
    if (update) Y.applyUpdate(this.doc, unb64(update), 'remote');
  }

  private async sync(body: {
    state_vector?: string;
    update?: string;
    seed?: string;
  }) {
    const res = await syncCollab(this.cardId, this.slug, body);
    if (res.status !== 200) throw new ApiError(res.status, res.data);
    return res.data;
  }

  private schedulePush() {
    clearTimeout(this.pushTimer);
    this.pushTimer = setTimeout(() => void this.push(), 120);
  }

  /** Sends what has not been sent; failed batches stay queued and are retried. */
  async push(): Promise<void> {
    if (this.pushing || this.stopped || this.pending.length === 0) return;
    this.pushing = true;
    const batch = this.pending.splice(0);
    try {
      const res = await this.sync({
        update: b64(Y.mergeUpdates(batch)),
        state_vector: b64(Y.encodeStateVector(this.doc)),
      });
      this.apply(res.update);
      this.hooks.onStatus('live');
    } catch (e) {
      this.pending.unshift(...batch);
      if (e instanceof ApiError && e.status === 409)
        return this.hooks.onReset(); // the session document is gone
      this.hooks.onStatus('offline');
      this.pushTimer = setTimeout(() => void this.push(), 2000);
    } finally {
      this.pushing = false;
      if (this.pending.length > 0 && !this.stopped) this.schedulePush();
    }
  }

  /** Follows the stream, and after any interruption catches up and follows again. */
  private async follow() {
    let wait = 500;
    while (!this.stopped) {
      try {
        await apiFetchStream(
          `/api/v1/cards/${this.cardId}/collab/${this.slug}/events`,
          this.abort.signal,
          (data) => this.onEvent(data),
        );
        wait = 500; // the server ends streams now and then on purpose
      } catch {
        if (this.stopped) return;
        this.hooks.onStatus('offline');
        wait = Math.min(wait * 2, 15_000);
      }
      if (this.stopped) return;
      await new Promise((r) => setTimeout(r, wait));
      try {
        const res = await this.sync({
          state_vector: b64(Y.encodeStateVector(this.doc)),
        });
        if (res.empty) return this.hooks.onReset();
        this.apply(res.update);
        this.hooks.onStatus('live');
      } catch {
        this.hooks.onStatus('offline');
      }
    }
  }

  private onEvent(data: string) {
    try {
      const ev = JSON.parse(data) as { t: string; u?: string };
      if (ev.t === 'update' && ev.u) this.apply(ev.u);
      else if (ev.t === 'reset') this.hooks.onReset();
    } catch {
      // not ours
    }
  }
}
