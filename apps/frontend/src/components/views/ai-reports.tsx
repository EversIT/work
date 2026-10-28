'use client';

import { Button, Checkbox, Spinner, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Sparkles } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useState } from 'react';
import {
  estimateBacklog,
  groupBacklog,
  planSprint,
  summarizeBoard,
  summarizeSprint,
} from '../../api/generated/ai/ai';
import { useAiStatus } from '../../lib/ai';
import { putCardSprint, updateCard } from '../../api/generated/cards/cards';
import type {
  BoardSummary,
  Estimate,
  IssueGroup,
  SprintOut,
  SprintPlan,
  SprintSummary,
} from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { routes } from '../../lib/routes';
import { JiraDialog } from '../ui/jira-dialog';
import { SimpleSelect } from '../ui/inputs';

/** Runs a report call, with the busy state and the toast for a failure. */
function useReport() {
  const t = useTranslations('reports');
  const te = useTranslations('errors');
  const [busy, setBusy] = useState(false);
  const run = async <T,>(
    call: () => Promise<T | undefined>,
    then: (v: T) => void,
  ) => {
    setBusy(true);
    try {
      const v = await call();
      if (v !== undefined) then(v);
    } catch (e) {
      const kind = errorKind(e);
      toast.danger(
        serverMessage(e) ?? (kind === 'generic' ? t('failed') : te(kind)),
      );
    } finally {
      setBusy(false);
    }
  };
  return { busy, run };
}

function ReportButton({
  busy,
  onPress,
  testId,
  children,
}: {
  busy: boolean;
  onPress: () => void;
  testId: string;
  children: React.ReactNode;
}) {
  return (
    <Button
      size="sm"
      variant="tertiary"
      onPress={onPress}
      isPending={busy}
      data-testid={testId}
    >
      {busy ? <Spinner size="sm" /> : <Sparkles size={14} />}
      {children}
    </Button>
  );
}

/** "Summarize" on a board: what moved, and what has sat untouched much longer than is usual in its column. */
export function BoardSummaryButton({
  listId,
  keyPrefix,
}: {
  listId: string;
  keyPrefix: string;
}) {
  const t = useTranslations('reports');
  const status = useAiStatus();
  const { busy, run } = useReport();
  const [data, setData] = useState<BoardSummary | null>(null);
  if (!status) return null;
  return (
    <>
      <ReportButton
        busy={busy}
        testId="summarize-board"
        onPress={() =>
          run(async () => body(await summarizeBoard(listId)), setData)
        }
      >
        {t('summarize')}
      </ReportButton>
      {data && (
        <JiraDialog
          isOpen
          onClose={() => setData(null)}
          title={t('boardTitle')}
          testId="board-summary"
          footer={
            <Button variant="tertiary" onPress={() => setData(null)}>
              {t('close')}
            </Button>
          }
        >
          <div className="flex flex-col gap-3 pb-2 text-sm">
            <p>{t('boardTotals', { total: data.total, done: data.done })}</p>
            <p>
              {t('boardWeek', {
                changed: data.updated_week,
                done: data.done_week,
              })}
            </p>
            <p className="text-muted">
              {data.by_stage.map((s) => `${s.name}: ${s.count}`).join(' · ')}
            </p>
            <div>
              <h4 className="mb-1 font-medium">{t('stuckTitle')}</h4>
              {data.stale.length === 0 ? (
                <p className="text-muted">{t('stuckNone')}</p>
              ) : (
                <ul className="flex flex-col gap-1">
                  {data.stale.map((c) => (
                    <li key={c.card_id}>
                      <Link
                        href={routes.browse(c.key)}
                        className="hover:underline"
                      >
                        <span className="mr-2 text-xs text-muted">{c.key}</span>
                        {c.title}
                      </Link>
                      <span className="block text-xs text-muted">
                        {t('stuckReason', {
                          days: c.days,
                          stage: c.stage,
                          usual: c.median_days,
                        })}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
              <p className="mt-2 text-xs text-muted">
                {t('stuckNote', { prefix: keyPrefix })}
              </p>
            </div>
          </div>
        </JiraDialog>
      )}
    </>
  );
}

/** Retro notes for the sprint being completed: text to copy into a comment or a document. */
export function SprintNotes({ sprint }: { sprint: SprintOut }) {
  const t = useTranslations('reports');
  const status = useAiStatus();
  const { busy, run } = useReport();
  const [notes, setNotes] = useState<string | null>(null);
  if (!status) return null;
  const write = (d: SprintSummary) => {
    const line = (i: { key: string; title: string; points?: number | null }) =>
      `- ${i.key} ${i.title}${i.points ? ` (${i.points})` : ''}`;
    const parts = [
      `## ${sprint.name}`,
      t('notesDone', { count: d.completed.length, points: d.points_done }),
      ...d.completed.map(line),
      '',
      t('notesOpen', { count: d.open.length, points: d.points_open }),
      ...d.open.map(line),
    ];
    if (d.themes.length > 0)
      parts.push('', t('notesThemes', { themes: d.themes.join(', ') }));
    if (d.slowest.length > 0)
      parts.push(
        '',
        t('notesSlowest'),
        ...d.slowest.map(
          (s) => `- ${s.key} ${s.title} (${t('days', { n: s.days })})`,
        ),
      );
    setNotes(parts.join('\n'));
  };
  return (
    <div className="flex flex-col gap-2">
      <ReportButton
        busy={busy}
        testId="sprint-notes"
        onPress={() =>
          run(async () => body(await summarizeSprint(sprint.id)), write)
        }
      >
        {t('notesButton')}
      </ReportButton>
      {notes && (
        <>
          <textarea
            readOnly
            value={notes}
            rows={10}
            aria-label={t('notesButton')}
            data-testid="sprint-notes-text"
            className="w-full rounded-sm border border-field-border bg-surface p-2 font-mono text-xs"
          />
          <Button
            size="sm"
            variant="secondary"
            className="self-start"
            onPress={() =>
              navigator.clipboard
                ?.writeText(notes)
                .then(() => toast.success(t('copied')))
            }
          >
            {t('copy')}
          </Button>
        </>
      )}
    </div>
  );
}

type Dialog =
  | { kind: 'plan'; data: SprintPlan }
  | { kind: 'estimate'; data: Estimate[] }
  | { kind: 'groups'; data: IssueGroup[] };

/** Backlog helpers: propose the next sprint, estimate unestimated issues, and group loose issues by topic. */
export function BacklogAssistant({
  listId,
  planned,
}: {
  listId: string;
  planned: SprintOut[];
}) {
  const t = useTranslations('reports');
  const qc = useQueryClient();
  const status = useAiStatus();
  const { busy, run } = useReport();
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const [target, setTarget] = useState<string | null>(planned[0]?.id ?? null);
  const [applying, setApplying] = useState(false);
  if (!status) return null;
  const open = (d: Dialog) => {
    setSkipped(new Set());
    setTarget(planned[0]?.id ?? null);
    setDialog(d);
  };
  const apply = async (what: () => Promise<unknown>, done: string) => {
    setApplying(true);
    try {
      await what();
      await qc.invalidateQueries();
      toast.success(done);
      setDialog(null);
    } catch (e) {
      toast.danger(serverMessage(e) ?? t('failed'));
    } finally {
      setApplying(false);
    }
  };

  return (
    <>
      <ReportButton
        busy={busy}
        testId="suggest-sprint"
        onPress={() =>
          run(
            async () => body(await planSprint(listId)),
            (data) => open({ kind: 'plan', data }),
          )
        }
      >
        {t('suggestSprint')}
      </ReportButton>
      <ReportButton
        busy={busy}
        testId="estimate-backlog"
        onPress={() =>
          run(
            async () => body(await estimateBacklog(listId)),
            (data) => open({ kind: 'estimate', data }),
          )
        }
      >
        {t('estimate')}
      </ReportButton>
      <ReportButton
        busy={busy}
        testId="group-backlog"
        onPress={() =>
          run(
            async () => body(await groupBacklog(listId)),
            (data) => open({ kind: 'groups', data }),
          )
        }
      >
        {t('groupTopics')}
      </ReportButton>

      {dialog?.kind === 'plan' && (
        <JiraDialog
          isOpen
          onClose={() => setDialog(null)}
          title={t('suggestSprint')}
          testId="sprint-plan"
          footer={
            <>
              <Button
                variant="primary"
                isPending={applying}
                isDisabled={
                  !target ||
                  dialog.data.picks.every((p) => skipped.has(p.card_id))
                }
                data-testid="plan-apply"
                onPress={() =>
                  target &&
                  apply(async () => {
                    for (const p of dialog.data.picks.filter(
                      (x) => !skipped.has(x.card_id),
                    ))
                      await putCardSprint(p.card_id, {
                        list_id: listId,
                        sprint_id: target,
                      });
                  }, t('planned'))
                }
              >
                {t('planApply')}
              </Button>
              <Button variant="tertiary" onPress={() => setDialog(null)}>
                {t('close')}
              </Button>
            </>
          }
        >
          <div className="flex flex-col gap-3 pb-2 text-sm">
            {dialog.data.velocity === null ||
            dialog.data.velocity === undefined ? (
              <p className="text-muted">{t('planNoHistory')}</p>
            ) : (
              <>
                <p>
                  {t('planVelocity', {
                    velocity: Math.round(dialog.data.velocity * 10) / 10,
                    sprints: dialog.data.sprints_used,
                  })}
                </p>
                {dialog.data.picks.length === 0 ? (
                  <p className="text-muted">{t('planNothing')}</p>
                ) : (
                  <ul className="flex flex-col gap-1">
                    {dialog.data.picks.map((p) => (
                      <li key={p.card_id}>
                        <Checkbox
                          className="flex flex-row items-center gap-2"
                          isSelected={!skipped.has(p.card_id)}
                          onChange={(on) =>
                            setSkipped((s) => {
                              const n = new Set(s);
                              if (on) n.delete(p.card_id);
                              else n.add(p.card_id);
                              return n;
                            })
                          }
                        >
                          <Checkbox.Control>
                            <Checkbox.Indicator />
                          </Checkbox.Control>
                          <Checkbox.Content>
                            <span className="text-xs text-muted">{p.key}</span>{' '}
                            {p.title}
                            <span className="ml-2 text-xs text-muted">
                              {p.estimated ? '~' : ''}
                              {p.points} {t('points')}
                              {p.priority ? ` · ${p.priority}` : ''}
                            </span>
                          </Checkbox.Content>
                        </Checkbox>
                      </li>
                    ))}
                  </ul>
                )}
                <p className="text-xs text-muted">
                  {t('planTotal', {
                    total: dialog.data.total,
                    capacity: Math.round(dialog.data.capacity * 10) / 10,
                  })}
                </p>
                {planned.length > 0 ? (
                  <SimpleSelect
                    label={t('planInto')}
                    value={target}
                    onChange={setTarget}
                    options={planned.map((p) => ({
                      value: p.id,
                      label: p.name,
                    }))}
                  />
                ) : (
                  <p className="text-xs text-muted">{t('planNoSprint')}</p>
                )}
              </>
            )}
          </div>
        </JiraDialog>
      )}

      {dialog?.kind === 'estimate' && (
        <JiraDialog
          isOpen
          onClose={() => setDialog(null)}
          title={t('estimate')}
          testId="estimates"
          footer={
            <>
              {dialog.data.length > 0 && (
                <Button
                  variant="primary"
                  isPending={applying}
                  data-testid="estimate-apply"
                  onPress={() =>
                    apply(
                      async () => {
                        for (const e of dialog.data)
                          await updateCard(e.card_id, {
                            data: { story_points: e.points },
                          });
                      },
                      t('estimated', { count: dialog.data.length }),
                    )
                  }
                >
                  {t('estimateApply')}
                </Button>
              )}
              <Button variant="tertiary" onPress={() => setDialog(null)}>
                {t('close')}
              </Button>
            </>
          }
        >
          <div className="flex flex-col gap-2 pb-2 text-sm">
            {dialog.data.length === 0 ? (
              <p className="text-muted">{t('estimateNone')}</p>
            ) : (
              <ul className="divide-y divide-separator">
                {dialog.data.map((e) => (
                  <li
                    key={e.card_id}
                    className="flex items-center gap-2 py-1.5"
                  >
                    <span className="min-w-0 flex-1 truncate">
                      <span className="mr-2 text-xs text-muted">{e.key}</span>
                      {e.title}
                    </span>
                    <strong>{e.points}</strong>
                    <span
                      className="text-xs text-muted"
                      title={t('estimateBasis', { count: e.based_on })}
                    >
                      {Math.round(e.confidence * 100)}%
                    </span>
                  </li>
                ))}
              </ul>
            )}
            <p className="text-xs text-muted">{t('estimateNote')}</p>
          </div>
        </JiraDialog>
      )}

      {dialog?.kind === 'groups' && (
        <JiraDialog
          isOpen
          onClose={() => setDialog(null)}
          title={t('groupTopics')}
          testId="topic-groups"
          footer={
            <Button variant="tertiary" onPress={() => setDialog(null)}>
              {t('close')}
            </Button>
          }
        >
          <div className="flex flex-col gap-4 pb-2 text-sm">
            {dialog.data.length === 0 ? (
              <p className="text-muted">{t('groupNone')}</p>
            ) : (
              dialog.data.map((g, i) => (
                <div key={i}>
                  <h4 className="mb-1 font-medium">
                    {g.keywords.join(', ') || t('groupOther')}
                  </h4>
                  <ul className="flex flex-col">
                    {g.cards.map((c) => (
                      <li key={c.card_id}>
                        <Link
                          href={routes.browse(c.key)}
                          className="hover:underline"
                        >
                          <span className="mr-2 text-xs text-muted">
                            {c.key}
                          </span>
                          {c.title}
                        </Link>
                      </li>
                    ))}
                  </ul>
                </div>
              ))
            )}
          </div>
        </JiraDialog>
      )}
    </>
  );
}
