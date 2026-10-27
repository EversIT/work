'use client';

import type { JSONContent } from '@tiptap/core';
import Collaboration from '@tiptap/extension-collaboration';
import { EditorContent, useEditor, type Editor } from '@tiptap/react';
import { useTranslations } from 'next-intl';
import { useCallback, useEffect, useRef, useState } from 'react';
import { isEmptyDoc, richExtensions, toDoc } from '../../lib/rich';
import { CollabClient, FRAGMENT, type CollabStatus } from './collab-client';
import { RichView } from './rich-view';
import { editorClass, Toolbar } from './rich-editor';

const SAVE_AFTER_MS = 1500;

type Labels = Record<
  'live' | 'offline' | 'connecting' | 'saving' | 'saved',
  string
>;
type SaveState = 'idle' | 'saving' | 'saved';

/**
 * A rich-text field of a card that several people can edit at once. Edits travel through the
 * shared Yjs document; the document's JSON is also saved into the card (`onSave`) a moment after
 * the last local edit, so the card, its API and searches always have the current text.
 * People who may not edit see the saved content.
 */
export function CollabField({
  label,
  cardId,
  slug,
  value,
  canWrite,
  onSave,
}: {
  label: string;
  cardId: string;
  slug: string;
  value: unknown;
  canWrite: boolean;
  /** Saves the document into the card as the editor's own save (see `collab_keep`). */
  onSave: (doc: JSONContent | null) => Promise<void>;
}) {
  const t = useTranslations('rich');
  // Bumped when the field was changed outside the editor: a new session then starts from the
  // card's current content (a new key gives a fresh session with fresh state).
  const [generation, setGeneration] = useState(0);
  const reset = useCallback(() => setGeneration((g) => g + 1), []);
  if (!canWrite) {
    return (
      <div>
        <p className="mb-1 text-xs font-semibold text-muted">{label}</p>
        {isEmptyDoc(value) ? (
          <p className="text-sm text-muted">{t('empty')}</p>
        ) : (
          <RichView value={value} />
        )}
      </div>
    );
  }
  const text: Labels = {
    live: t('live'),
    offline: t('offline'),
    connecting: t('connecting'),
    saving: t('saving'),
    saved: t('saved'),
  };
  return (
    <Session
      key={generation}
      label={label}
      cardId={cardId}
      slug={slug}
      value={value}
      onSave={onSave}
      onReset={reset}
      text={text}
    />
  );
}

function Session({
  label,
  cardId,
  slug,
  value,
  onSave,
  onReset,
  text,
}: {
  label: string;
  cardId: string;
  slug: string;
  value: unknown;
  onSave: (doc: JSONContent | null) => Promise<void>;
  onReset: () => void;
  text: Labels;
}) {
  const [client, setClient] = useState<CollabClient | null>(null);
  const [status, setStatus] = useState<CollabStatus>('connecting');
  const [saved, setSaved] = useState<SaveState>('idle');
  // The seed is read when a session starts, not on every render.
  const valueRef = useRef(value);
  useEffect(() => {
    valueRef.current = value;
  });

  useEffect(() => {
    let cancelled = false;
    const c = new CollabClient(cardId, slug, () => toDoc(valueRef.current), {
      onStatus: (s) => !cancelled && setStatus(s),
      onReset: () => !cancelled && onReset(),
      onLocalChange: () => !cancelled && setSaved('idle'),
    });
    c.start().then(
      () => (cancelled ? c.stop() : setClient(c)),
      () => !cancelled && setStatus('offline'),
    );
    return () => {
      cancelled = true;
      c.stop();
    };
  }, [cardId, slug, onReset]);

  if (!client) {
    return (
      <div>
        <p className="mb-1 text-xs font-semibold text-muted">{label}</p>
        <div
          className="min-h-24 rounded-sm border border-field-border p-3 text-sm text-muted"
          aria-busy
        >
          {status === 'offline' ? text.offline : text.connecting}
        </div>
      </div>
    );
  }
  return (
    <Editing
      label={label}
      client={client}
      status={status}
      saved={saved}
      setSaved={setSaved}
      onSave={onSave}
      text={text}
    />
  );
}

function Editing({
  label,
  client,
  status,
  saved,
  setSaved,
  onSave,
  text,
}: {
  label: string;
  client: CollabClient;
  status: CollabStatus;
  saved: SaveState;
  setSaved: (s: SaveState) => void;
  onSave: (doc: JSONContent | null) => Promise<void>;
  text: Labels;
}) {
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const editorRef = useRef<Editor | null>(null);
  // The parent re-renders often and passes a new function each time; saving must not restart.
  const onSaveRef = useRef(onSave);
  useEffect(() => {
    onSaveRef.current = onSave;
  });

  const saveRef = useRef<() => Promise<void>>(async () => undefined);
  useEffect(() => {
    saveRef.current = async () => {
      clearTimeout(timer.current);
      const editor = editorRef.current;
      if (!editor || editor.isDestroyed || !client.hasUnsavedEdits()) return;
      client.setUnsaved(false);
      setSaved('saving');
      try {
        await client.push(); // everything typed is on the server before the card is written
        const doc = editor.getJSON();
        await onSaveRef.current(isEmptyDoc(doc) ? null : doc);
        setSaved('saved');
      } catch {
        client.setUnsaved(true); // try again with the next edit or blur
        setSaved('idle');
      }
    };
  }, [client, setSaved]);

  const editor = useEditor(
    {
      extensions: [
        ...richExtensions(true),
        Collaboration.configure({ document: client.doc, field: FRAGMENT }),
      ],
      immediatelyRender: false,
      editorProps: {
        attributes: {
          'aria-label': label,
          role: 'textbox',
          'aria-multiline': 'true',
        },
      },
      onCreate: ({ editor: e }) => {
        editorRef.current = e;
      },
      onUpdate: () => {
        if (!client.hasUnsavedEdits()) return; // a change that came from somebody else
        clearTimeout(timer.current);
        timer.current = setTimeout(() => void saveRef.current(), SAVE_AFTER_MS);
      },
      onBlur: () => void saveRef.current(),
    },
    [client],
  );

  // Leaving the page or the card must not lose the last edits.
  useEffect(() => {
    const flush = saveRef;
    return () => {
      if (client.hasUnsavedEdits()) void flush.current();
    };
  }, [client]);

  if (!editor)
    return (
      <div
        className="min-h-24 rounded-sm border border-field-border"
        aria-busy
      />
    );
  return (
    <div data-testid={`collab-${label}`}>
      <div className="mb-1 flex items-center justify-between">
        <p className="text-xs font-semibold text-muted">{label}</p>
        <p
          className="text-xs text-muted"
          aria-live="polite"
          data-testid="collab-status"
        >
          {status === 'live'
            ? text.live
            : status === 'offline'
              ? text.offline
              : text.connecting}
          {saved === 'saving'
            ? ` · ${text.saving}`
            : saved === 'saved'
              ? ` · ${text.saved}`
              : ''}
        </p>
      </div>
      <div className="overflow-hidden rounded-sm border border-field-border bg-surface focus-within:border-accent">
        <Toolbar editor={editor} history={false} />
        <EditorContent editor={editor} className={editorClass} />
      </div>
    </div>
  );
}
