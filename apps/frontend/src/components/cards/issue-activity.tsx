'use client';

import { Button, Checkbox, Chip, Label, toast, Tabs } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Paperclip, Trash2 } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useRef, useState } from 'react';
import {
  useCreateCardActivity,
  useDeleteActivity,
  useListCardActivities,
} from '../../api/generated/activities/activities';
import { uploadFile } from '../../api/generated/files/files';
import type { ActivityOut } from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { fullName, relativeTime } from '../../lib/format';
import { UserAvatar } from '../shell/user-avatar';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { isEmptyDoc, toDoc } from '../../lib/rich';
import { RichEditor } from '../rich/rich-editor';
import { RichView } from '../rich/rich-view';
import {
  ReplyChips,
  SUMMARY_FROM,
  ThreadSummary,
  ToneCheck,
} from './ai-comments';
import { downloadFile } from './files';

type Member = { user_id: string; first_name: string; last_name: string };
type Tab = 'all' | 'comments' | 'history';

/** Jira's activity section: tabs (All / Comments / History) and a comment composer. */
export function IssueActivity({
  cardId,
  projectId,
}: {
  cardId: string;
  projectId?: string;
}) {
  const t = useTranslations('issue');
  const te = useTranslations('errors');
  const locale = useLocale();
  const qc = useQueryClient();
  const ctx = useWorkspaceCtx();
  const members = ctx.members;
  const acts = body(useListCardActivities(cardId).data) ?? [];
  const create = useCreateCardActivity();
  const del = useDeleteActivity();
  const [tab, setTab] = useState<Tab>('all');
  const [composing, setComposing] = useState(false);
  const [doc, setDoc] = useState<unknown>(null);
  const [editorKey, setEditorKey] = useState(0);
  const [mentions, setMentions] = useState<string[]>([]);
  const [files, setFiles] = useState<
    { id: string; name: string; content_type: string }[]
  >([]);
  const fileRef = useRef<HTMLInputElement>(null);
  const fail = (e: unknown) =>
    toast.danger(serverMessage(e) ?? te(errorKind(e)));
  const who = (id?: string | null) => members.find((m) => m.user_id === id);

  const shown = [...acts]
    .reverse()
    .filter((a) =>
      tab === 'all'
        ? true
        : tab === 'comments'
          ? a.type === 'comment'
          : a.type === 'update',
    );

  const post = async () => {
    if (isEmptyDoc(doc) && files.length === 0) return;
    try {
      await create.mutateAsync({
        id: cardId,
        data: {
          type: 'comment',
          content: { content: isEmptyDoc(doc) ? '' : doc, files },
          mentions,
        },
      });
      setDoc(null);
      setEditorKey((k) => k + 1);
      setMentions([]);
      setFiles([]);
      setComposing(false);
      await qc.invalidateQueries();
    } catch (e) {
      fail(e);
    }
  };

  return (
    <section aria-label={t('activity')} className="flex flex-col gap-3">
      <h3 className="text-base font-semibold text-foreground">
        {t('activity')}
      </h3>
      <Tabs
        selectedKey={tab}
        onSelectionChange={(k) => setTab(String(k) as Tab)}
      >
        <Tabs.ListContainer>
          <Tabs.List aria-label={t('activity')}>
            {(['all', 'comments', 'history'] as Tab[]).map((k) => (
              <Tabs.Tab key={k} id={k}>
                {t(`tab.${k}`)}
                <Tabs.Indicator />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>

      {tab !== 'history' && (
        <div className="flex gap-3">
          {ctx.me && <UserAvatar user={ctx.me} />}
          <div className="min-w-0 flex-1">
            {!composing ? (
              <Button
                onPress={() => setComposing(true)}
                data-testid="add-comment"
                variant="tertiary"
                size="md"
                className="w-full justify-start font-normal text-muted"
              >
                {t('addComment')}
              </Button>
            ) : (
              <div className="flex flex-col gap-2">
                <RichEditor
                  key={editorKey}
                  label={t('comment')}
                  value={doc}
                  onChange={setDoc}
                  testId="comment-editor"
                />
                {isEmptyDoc(doc) && (
                  <ReplyChips
                    cardId={cardId}
                    onPick={(text) => {
                      setDoc(toDoc(text));
                      setEditorKey((k) => k + 1);
                    }}
                  />
                )}
                {members.length > 1 && (
                  <div className="flex flex-wrap gap-x-4 gap-y-1">
                    <span className="text-xs text-muted">{t('notify')}:</span>
                    {members
                      .filter((m) => m.user_id !== ctx.me?.id)
                      .map((m) => (
                        <Checkbox
                          className="flex flex-row items-center gap-2"
                          key={m.user_id}
                          isSelected={mentions.includes(m.user_id)}
                          onChange={(v) =>
                            setMentions(
                              v
                                ? [...mentions, m.user_id]
                                : mentions.filter((x) => x !== m.user_id),
                            )
                          }
                        >
                          <Checkbox.Control>
                            <Checkbox.Indicator />
                          </Checkbox.Control>
                          <Checkbox.Content>
                            <Label>{fullName(m)}</Label>
                          </Checkbox.Content>
                        </Checkbox>
                      ))}
                  </div>
                )}
                {files.length > 0 && (
                  <ul className="flex flex-wrap gap-1">
                    {files.map((f) => (
                      <Chip key={f.id} size="sm">
                        <Chip.Label>{f.name}</Chip.Label>
                      </Chip>
                    ))}
                  </ul>
                )}
                <div className="flex items-center gap-2">
                  <Button
                    size="sm"
                    onPress={post}
                    isPending={create.isPending}
                    data-testid="post-comment"
                  >
                    {t('save')}
                  </Button>
                  <Button
                    size="sm"
                    variant="tertiary"
                    onPress={() => {
                      setComposing(false);
                      setDoc(null);
                      setEditorKey((k) => k + 1);
                    }}
                  >
                    {t('cancel')}
                  </Button>
                  <ToneCheck
                    doc={doc}
                    onUse={(d) => {
                      setDoc(d);
                      setEditorKey((k) => k + 1);
                    }}
                  />
                  <input
                    ref={fileRef}
                    type="file"
                    className="hidden"
                    data-testid="file-input"
                    onChange={async (e) => {
                      const file = e.target.files?.[0];
                      e.target.value = '';
                      if (!file || !projectId) return;
                      try {
                        const res = await uploadFile(projectId, { file });
                        if (res.status === 200)
                          setFiles((fs) => [
                            ...fs,
                            {
                              id: res.data.id,
                              name: res.data.name,
                              content_type: res.data.content_type,
                            },
                          ]);
                      } catch (err) {
                        fail(err);
                      }
                    }}
                  />
                  <Button
                    size="sm"
                    variant="ghost"
                    className="ml-auto"
                    onPress={() => fileRef.current?.click()}
                  >
                    <Paperclip size={14} />
                    {t('attach')}
                  </Button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {tab !== 'history' &&
        acts.filter((a) => a.type === 'comment').length > SUMMARY_FROM && (
          <ThreadSummary cardId={cardId} />
        )}

      <ol className="flex flex-col gap-4" data-testid="activities">
        {shown.map((a) => (
          <ActivityItem
            key={a.id}
            a={a}
            author={who(a.created_by)}
            locale={locale}
            members={members}
            onDelete={async () => {
              try {
                await del.mutateAsync({ id: a.id });
                await qc.invalidateQueries();
              } catch (e) {
                fail(e);
              }
            }}
          />
        ))}
      </ol>
    </section>
  );
}

function ActivityItem({
  a,
  author,
  locale,
  members,
  onDelete,
}: {
  a: ActivityOut;
  author?: Member;
  locale: string;
  members: Member[];
  onDelete: () => void;
}) {
  const t = useTranslations('issue');
  const c = a.content as Record<string, unknown>;
  const name = author
    ? fullName(author)
    : a.created_by_type === 'system'
      ? t('system')
      : t('someone');
  return (
    <li className="flex gap-3" data-type={a.type}>
      {author ? (
        <UserAvatar user={author} />
      ) : (
        <div className="size-8 shrink-0 rounded-full bg-default-hover" />
      )}
      <div className="min-w-0 flex-1">
        <p className="text-xs text-muted">
          <span className="font-semibold text-foreground">{name}</span> ·{' '}
          {relativeTime(a.created_at, locale)}
          {a.type === 'update' && <span> · {t('changedFields')}</span>}
        </p>
        {a.type === 'comment' && (
          <div>
            {typeof c.content === 'string' ? (
              <p className="whitespace-pre-wrap break-words text-sm">
                {c.content}
              </p>
            ) : c.content && typeof c.content === 'object' ? (
              <RichView value={c.content} />
            ) : null}
            <Attachments files={c.files} />
          </div>
        )}
        {a.type === 'task' && (
          <p className="text-sm">☐ {String(c.title ?? '')}</p>
        )}
        {a.type === 'update' && isLinkChange(c.link) && (
          <p className="text-sm text-muted">
            {t(c.link.action === 'added' ? 'linkAdded' : 'linkRemoved', {
              relation: t(`relation.${linkRelation(c.link)}`),
              key: c.link.key,
            })}
          </p>
        )}
        {a.type === 'update' && (
          <ul className="text-sm text-muted">
            {(Array.isArray(c.changes)
              ? (c.changes as { field: string; from: unknown; to: unknown }[])
              : []
            ).map((x) => (
              <li key={x.field}>
                <span className="font-medium">{x.field}</span>:{' '}
                <span className="line-through">{show(x.from, members)}</span> →{' '}
                {show(x.to, members)}
              </li>
            ))}
          </ul>
        )}
      </div>
      {a.type !== 'update' && (
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={t('deleteActivity')}
          onPress={onDelete}
        >
          <Trash2 size={12} />
        </Button>
      )}
    </li>
  );
}

type LinkChange = {
  action: string;
  type: string;
  direction: string;
  key: string;
};
const isLinkChange = (v: unknown): v is LinkChange =>
  !!v &&
  typeof v === 'object' &&
  typeof (v as LinkChange).key === 'string' &&
  typeof (v as LinkChange).type === 'string';
const INWARD: Record<string, string> = {
  blocks: 'is_blocked_by',
  duplicates: 'is_duplicated_by',
  clones: 'is_cloned_by',
};
/** History entry -> the relationship key used in the `issue.relation` messages. */
const linkRelation = (l: LinkChange) =>
  l.type !== 'relates' && l.direction === 'inward' ? INWARD[l.type] : l.type;

function show(v: unknown, members: Member[]): string {
  if (v === null || v === undefined || v === '') return '—';
  const one = (x: unknown) => {
    const m =
      typeof x === 'string'
        ? members.find((mm) => mm.user_id === x)
        : undefined;
    return m ? fullName(m) : typeof x === 'object' ? '…' : String(x);
  };
  return Array.isArray(v) ? v.map(one).join(', ') : one(v);
}

function Attachments({ files }: { files: unknown }) {
  if (!Array.isArray(files) || files.length === 0) return null;
  return (
    <ul className="mt-1 flex flex-wrap gap-2">
      {(files as { id: string; name: string }[]).map((f) => (
        <li key={f.id}>
          <Button
            onPress={() => downloadFile(f.id, f.name)}
            variant="tertiary"
            size="sm"
          >
            <Paperclip size={12} />
            {f.name}
          </Button>
        </li>
      ))}
    </ul>
  );
}
