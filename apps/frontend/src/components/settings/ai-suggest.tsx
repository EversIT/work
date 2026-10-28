'use client';

import { Button, Checkbox, Label, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Sparkles } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useState } from 'react';
import { createField, useListFields } from '../../api/generated/fields/fields';
import {
  useCreateStage,
  useSetListFields,
} from '../../api/generated/lists/lists';
import type { ListDetail } from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import {
  FIELD_TEMPLATES,
  label,
  rankTemplates,
  STATUS_TEMPLATES,
  type FieldTemplate,
  type StatusTemplate,
} from '../../lib/templates';
import { TextInput } from '../ui/inputs';
import { Panel } from '../ui/settings';
import { useAiStatus } from '../../lib/ai';

const same = (a: string, b: string) =>
  a.trim().toLowerCase() === b.trim().toLowerCase();

/** "Describe your team": proposes workflows to add in one click. A template library, not a model. */
/** Appears only while a language model is set up (the AI features are on). */
export function SuggestStatuses({ list }: { list: ListDetail }) {
  const t = useTranslations('psettings');
  const ai = useAiStatus();
  const locale = useLocale();
  const qc = useQueryClient();
  const create = useCreateStage();
  const [text, setText] = useState('');
  const [found, setFound] = useState<StatusTemplate[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const add = async (tpl: StatusTemplate) => {
    setBusy(tpl.id);
    try {
      let added = 0;
      for (const s of tpl.stages) {
        const name = label(s.name, locale);
        if (list.stages.some((x) => same(x.name, name))) continue;
        await create.mutateAsync({
          id: list.id,
          data: { name, color: s.color, is_completed: s.done },
        });
        added += 1;
      }
      await qc.invalidateQueries();
      toast.success(t('aiStatusesAdded', { count: added }));
    } catch (e) {
      toast.danger(serverMessage(e) ?? errorKind(e));
    } finally {
      setBusy(null);
    }
  };

  if (!ai) return null;
  return (
    <Panel testId="suggest-statuses">
      <Panel.Header>
        <div>
          <Panel.Title>{t('aiStatusesTitle')}</Panel.Title>
          <Panel.Description>{t('aiStatusesHelp')}</Panel.Description>
        </div>
      </Panel.Header>
      <form
        className="flex items-end gap-2 px-5 pb-4"
        onSubmit={(e) => {
          e.preventDefault();
          setFound(
            rankTemplates(STATUS_TEMPLATES, text, ['simple', 'software']),
          );
        }}
      >
        <div className="flex-1">
          <TextInput
            label={t('aiDescribe')}
            placeholder={t('aiStatusesExample')}
            value={text}
            onChange={setText}
            maxLength={300}
          />
        </div>
        <Button
          type="submit"
          variant="secondary"
          isDisabled={!text.trim()}
          data-testid="suggest-statuses-go"
        >
          <Sparkles size={14} />
          {t('aiSuggest')}
        </Button>
      </form>
      {found && (
        <ul
          className="divide-y divide-separator border-t border-separator"
          data-testid="status-proposals"
        >
          {found.map((tpl) => (
            <li
              key={tpl.id}
              className="flex flex-wrap items-center gap-3 px-5 py-3"
            >
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium">{label(tpl.name, locale)}</p>
                <p className="text-xs text-muted">
                  {tpl.stages.map((s) => label(s.name, locale)).join(' → ')}
                </p>
              </div>
              <Button
                size="sm"
                variant="secondary"
                isPending={busy === tpl.id}
                onPress={() => add(tpl)}
              >
                {t('aiStatusesAdd')}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

/** "Describe your project": proposes fields to create and switch on for this project, which the person ticks. */
export function SuggestFields({
  list,
  workspaceId,
}: {
  list: ListDetail;
  workspaceId: string;
}) {
  const t = useTranslations('psettings');
  const ai = useAiStatus();
  const locale = useLocale();
  const qc = useQueryClient();
  const set = useSetListFields();
  const existing = body(useListFields(workspaceId).data) ?? [];
  const [text, setText] = useState('');
  const [found, setFound] = useState<FieldTemplate[] | null>(null);
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);
  const key = (tpl: FieldTemplate, i: number) => `${tpl.id}:${i}`;

  const create = async (tpl: FieldTemplate) => {
    setBusy(tpl.id);
    try {
      const ids: string[] = [];
      for (const [i, f] of tpl.fields.entries()) {
        if (skipped.has(key(tpl, i))) continue;
        const name = label(f.name, locale);
        const have = existing.find((x) => same(x.name, name));
        if (have) {
          ids.push(have.id);
          continue;
        }
        const made = body(
          await createField(workspaceId, {
            name,
            type: f.type,
            // The API's schema calls this an object; the server stores the list of `{item}` it is given.
            items: f.items?.map((it) => ({
              item: label(it, locale),
            })) as unknown as never,
          }),
        );
        if (made) ids.push(made.id);
      }
      const next = [...new Set([...list.field_ids, ...ids])];
      await set.mutateAsync({ id: list.id, data: { field_ids: next } });
      await qc.invalidateQueries();
      toast.success(t('aiFieldsAdded', { count: ids.length }));
    } catch (e) {
      toast.danger(serverMessage(e) ?? errorKind(e));
    } finally {
      setBusy(null);
    }
  };

  if (!ai) return null;
  return (
    <Panel testId="suggest-fields">
      <Panel.Header>
        <div>
          <Panel.Title>{t('aiFieldsTitle')}</Panel.Title>
          <Panel.Description>{t('aiFieldsHelp')}</Panel.Description>
        </div>
      </Panel.Header>
      <form
        className="flex items-end gap-2 px-5 pb-4"
        onSubmit={(e) => {
          e.preventDefault();
          setSkipped(new Set());
          setFound(rankTemplates(FIELD_TEMPLATES, text));
        }}
      >
        <div className="flex-1">
          <TextInput
            label={t('aiDescribe')}
            placeholder={t('aiFieldsExample')}
            value={text}
            onChange={setText}
            maxLength={300}
          />
        </div>
        <Button
          type="submit"
          variant="secondary"
          isDisabled={!text.trim()}
          data-testid="suggest-fields-go"
        >
          <Sparkles size={14} />
          {t('aiSuggest')}
        </Button>
      </form>
      {found &&
        (found.length === 0 ? (
          <p className="border-t border-separator px-5 py-4 text-sm text-muted">
            {t('aiNothingFits')}
          </p>
        ) : (
          <ul
            className="divide-y divide-separator border-t border-separator"
            data-testid="field-proposals"
          >
            {found.map((tpl) => (
              <li key={tpl.id} className="flex flex-col gap-2 px-5 py-3">
                <p className="text-sm font-medium">{label(tpl.name, locale)}</p>
                <div className="flex flex-wrap gap-x-5 gap-y-1">
                  {tpl.fields.map((f, i) => (
                    <Checkbox
                      key={key(tpl, i)}
                      className="flex flex-row items-center gap-2"
                      isSelected={!skipped.has(key(tpl, i))}
                      onChange={(on) =>
                        setSkipped((s) => {
                          const n = new Set(s);
                          if (on) n.delete(key(tpl, i));
                          else n.add(key(tpl, i));
                          return n;
                        })
                      }
                    >
                      <Checkbox.Control>
                        <Checkbox.Indicator />
                      </Checkbox.Control>
                      <Checkbox.Content>
                        <Label>
                          {label(f.name, locale)}
                          {f.items && (
                            <span className="text-muted">
                              {' '}
                              (
                              {f.items
                                .map((it) => label(it, locale))
                                .join(', ')}
                              )
                            </span>
                          )}
                        </Label>
                      </Checkbox.Content>
                    </Checkbox>
                  ))}
                </div>
                <Button
                  size="sm"
                  variant="secondary"
                  className="self-start"
                  isPending={busy === tpl.id}
                  onPress={() => create(tpl)}
                >
                  {t('aiFieldsCreate')}
                </Button>
              </li>
            ))}
          </ul>
        ))}
    </Panel>
  );
}
