'use client';

import { routes } from '../../lib/routes';
import { Button, Checkbox, Label, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useMemo, useState } from 'react';
import { useCreateCard, putCardSprint } from '../../api/generated/cards/cards';
import { useGetList } from '../../api/generated/lists/lists';
import { body, errorKind, serverMessage } from '../../lib/api';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { JiraDialog } from '../ui/jira-dialog';
import { SimpleSelect } from '../ui/inputs';
import { plainText } from '../../lib/rich';
import { AiCreateHelper } from './ai-create';
import { FieldInput } from './field-input';

const ORDER = [
  'title',
  'description',
  'assignee',
  'reporter',
  'priority',
  'labels',
  'story_points',
  'due_at',
  'starts_at',
  'estimate_hours',
];

function rank(slug: string) {
  const i = ORDER.indexOf(slug);
  return i === -1 ? ORDER.length : i;
}

/** Jira's "Create issue": project, issue type, summary and the fields of that type. */
export function CreateIssueDialog({
  listId: initialList,
  stageId,
  sprintId,
  parentId,
  onClose,
}: {
  listId?: string;
  stageId?: string;
  sprintId?: string | null;
  parentId?: string;
  onClose: () => void;
}) {
  const t = useTranslations('issue');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const ctx = useWorkspaceCtx();
  const create = useCreateCard();
  const [listId, setListId] = useState<string | null>(() => {
    if (initialList) return initialList;
    try {
      return localStorage.getItem('work.lastList') ?? ctx.lists[0]?.id ?? null;
    } catch {
      return ctx.lists[0]?.id ?? null;
    }
  });
  const list = body(
    useGetList(listId ?? '', { query: { enabled: !!listId } }).data,
  );
  const [typeId, setTypeId] = useState<string | null>(null);
  const [data, setData] = useState<Record<string, unknown>>({});
  const [another, setAnother] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const defaultType = useMemo(() => {
    if (parentId) return ctx.cardTypes.find((c) => c.icon === 'subtask')?.id;
    return list?.default_card_type_id ?? ctx.cardTypes[0]?.id;
  }, [parentId, list, ctx.cardTypes]);
  const activeType = ctx.cardTypes.find(
    (c) => c.id === (typeId ?? defaultType),
  );
  const fields = useMemo(
    () =>
      [...(activeType?.fields ?? [])]
        .filter((f) => !['stage', 'card'].includes(f.type))
        .sort((a, b) => rank(a.slug) - rank(b.slug)),
    [activeType],
  );
  const members = ctx.members;

  // Reporter defaults to the person creating the issue, like Jira.
  const reporterField = fields.find((f) => f.slug === 'reporter');
  const values: Record<string, unknown> =
    reporterField && ctx.me && data.reporter === undefined
      ? { ...data, reporter: reporterField.multiple ? [ctx.me.id] : ctx.me.id }
      : data;

  const submit = async () => {
    setError(null);
    if (!listId || !activeType) return;
    try {
      const payload = Object.fromEntries(
        Object.entries(values).filter(
          ([, v]) => v !== null && v !== undefined && v !== '',
        ),
      );
      const res = await create.mutateAsync({
        id: listId,
        data: {
          data: payload,
          card_type_id: activeType.id,
          stage_id: stageId,
          parent_id: parentId,
        },
      });
      if (res.status !== 200) return;
      if (sprintId)
        await putCardSprint(res.data.id, {
          list_id: listId,
          sprint_id: sprintId,
        });
      try {
        localStorage.setItem('work.lastList', listId);
      } catch {
        /* ignore */
      }
      await qc.invalidateQueries();
      toast.success(
        <span>
          {t('created')}{' '}
          <Link className="underline" href={routes.browse(res.data.key)}>
            {res.data.key}
          </Link>
        </span>,
      );
      if (another) {
        setData({});
      } else {
        onClose();
      }
    } catch (e) {
      setError(serverMessage(e) ?? te(errorKind(e)));
    }
  };

  const assignee = fields.find((f) => f.is_assignee);

  return (
    <JiraDialog
      isOpen
      onClose={onClose}
      title={parentId ? t('createSubtask') : t('create')}
      size="md"
      testId="create-issue"
      footer={
        <>
          <Checkbox
            isSelected={another}
            onChange={setAnother}
            className="mr-auto"
          >
            <Checkbox.Control>
              <Checkbox.Indicator />
            </Checkbox.Control>
            <Checkbox.Content>
              <Label>{t('createAnother')}</Label>
            </Checkbox.Content>
          </Checkbox>
          <Button variant="tertiary" onPress={onClose}>
            {t('cancel')}
          </Button>
          <Button
            onPress={submit}
            isPending={create.isPending}
            data-testid="create-issue-submit"
          >
            {t('createButton')}
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <p className="text-xs text-muted">{t('requiredHint')}</p>
        {error && (
          <p className="rounded bg-danger-soft px-3 py-2 text-sm text-danger">
            {error}
          </p>
        )}
        <div className="grid gap-3 sm:grid-cols-2">
          {!initialList && (
            <SimpleSelect
              label={t('project')}
              value={listId}
              onChange={(v) => {
                setListId(v);
                setTypeId(null);
              }}
              options={ctx.lists.map((l) => ({
                value: l.id,
                label: `${l.name} (${l.spaceName})`,
              }))}
              testId="create-project"
            />
          )}
          <SimpleSelect
            label={t('issueType')}
            value={activeType?.id ?? null}
            onChange={(v) => v && setTypeId(v)}
            options={ctx.cardTypes
              .filter((c) => !parentId || c.icon === 'subtask')
              .map((c) => ({ value: c.id, label: c.name }))}
            testId="create-type"
          />
        </div>
        <div className="grid gap-4 sm:grid-cols-2">
          {fields.map((f) => (
            <div
              key={f.id}
              className={
                f.is_title
                  ? 'sm:col-span-2 [&_input]:rounded-none [&_input]:border-0 [&_input]:border-b [&_input]:border-border [&_input]:bg-transparent [&_input]:px-0 [&_input]:text-xl [&_input]:font-medium [&_input]:shadow-none'
                  : f.is_description ||
                      f.type === 'rich' ||
                      f.type === 'text_area'
                    ? 'sm:col-span-2'
                    : undefined
              }
            >
              <FieldInput
                field={f}
                value={values[f.slug]}
                members={members}
                onChange={(v) => setData((d) => ({ ...d, [f.slug]: v }))}
                label={f.is_title ? `${f.name} *` : f.name}
              />
              {f.id === assignee?.id && ctx.me && (
                <Button
                  type="button"

                  onPress={() =>
                    setData((d) => ({
                      ...d,
                      [f.slug]: f.multiple ? [ctx.me?.id] : ctx.me?.id,
                    }))
                  }
                  variant="ghost"
                  size="sm"
                  className="mt-1 h-auto px-0 text-xs text-accent"
                >
                  {t('assignToMe')}
                </Button>
              )}
            </div>
          ))}
        </div>
        {ctx.workspace && (
          <AiCreateHelper
            workspaceId={ctx.workspace.id}
            cardTypeId={activeType?.id}
            kind={activeType?.name}
            title={String(
              values[fields.find((f) => f.is_title)?.slug ?? ''] ?? '',
            )}
            description={plainText(
              values[fields.find((f) => f.is_description)?.slug ?? ''] ?? '',
            )}
            filled={Object.entries(values)
              .filter(([, v]) => v !== null && v !== undefined && v !== '')
              .map(([k]) => k)}
            descriptionSlug={
              fields.find((f) => f.is_description && f.type === 'rich')?.slug
            }
            onApply={(slug, value) => setData((d) => ({ ...d, [slug]: value }))}
          />
        )}
      </form>
    </JiraDialog>
  );
}
