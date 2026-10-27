'use client';

import { Button, Input, Label, Switch, TextField, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import {
  useCreateStage,
  useDeleteStage,
  useReorderStages,
  useUpdateStage,
} from '../../api/generated/lists/lists';
import type { ListDetail } from '../../api/generated/model';
import { errorKind, serverMessage } from '../../lib/api';
import { SimpleSelect, TextInput } from '../ui/inputs';
import { EditScope, Panel, Stagger, nth } from '../ui/settings';

const COLORS = ['grey', 'blue', 'purple', 'green', 'orange', 'red', 'teal'];

/** The statuses an issue moves through, in order. They are the columns of the board. */
export function ProjectWorkflow({
  list,
  canEdit,
}: {
  list: ListDetail;
  canEdit: boolean;
}) {
  const t = useTranslations('psettings');
  const ts = useTranslations('settings');
  const qc = useQueryClient();
  const create = useCreateStage();
  const update = useUpdateStage();
  const del = useDeleteStage();
  const reorder = useReorderStages();
  const [newName, setNewName] = useState('');
  const refresh = () => qc.invalidateQueries();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? errorKind(e));
  const stages = [...list.stages].sort((a, b) => a.sort_order - b.sort_order);

  const move = async (i: number, d: number) => {
    const ids = stages.map((s) => s.id);
    const [x] = ids.splice(i, 1);
    ids.splice(i + d, 0, x);
    try {
      await reorder.mutateAsync({ id: list.id, data: { ids } });
      await refresh();
    } catch (e) {
      fail(e);
    }
  };
  const patch = async (id: string, data: Record<string, unknown>) => {
    try {
      await update.mutateAsync({ id, data });
      await refresh();
    } catch (e) {
      fail(e);
    }
  };

  return (
    <EditScope canEdit={canEdit}>
      <div className="flex flex-col gap-6">
        <Panel testId="statuses">
          <Panel.Header>
            <div>
              <Panel.Title>{t('statuses')}</Panel.Title>
              <Panel.Description>{t('statusesHelp')}</Panel.Description>
            </div>
          </Panel.Header>
          <Stagger as="ul" className="divide-y divide-separator">
            {stages.map((s, i) => (
              <li
                key={s.id}
                style={nth(i)}
                className="flex flex-wrap items-center gap-3 px-5 py-3 transition-colors hover:bg-surface-secondary"
              >
                <span
                  className="size-3 shrink-0 rounded-full bg-accent"
                  style={{ opacity: s.is_completed ? 1 : 0.45 }}
                  aria-hidden
                />
                <TextField
                  aria-label={ts('stageName')}
                  defaultValue={s.name}
                  maxLength={100}
                  className="min-w-40 flex-1"
                  onBlur={(e) => {
                    const v = (e.target as HTMLInputElement).value.trim();
                    if (v && v !== s.name) void patch(s.id, { name: v });
                  }}
                >
                  <Input />
                </TextField>
                <div className="w-32">
                  <SimpleSelect
                    hideLabel
                    label={t('color')}
                    value={s.color}
                    options={COLORS.map((c) => ({
                      value: c,
                      label: t(`colors.${c}`),
                    }))}
                    onChange={(v) => v && void patch(s.id, { color: v })}
                  />
                </div>
                <Switch
                  className="flex flex-row items-center gap-2"
                  isSelected={s.is_completed}
                  onChange={(v) => void patch(s.id, { is_completed: v })}
                >
                  <Switch.Control>
                    <Switch.Thumb />
                  </Switch.Control>
                  <Switch.Content>
                    <Label>{ts('completes')}</Label>
                  </Switch.Content>
                </Switch>
                <div className="flex">
                  <Button
                    isIconOnly
                    size="sm"
                    variant="ghost"
                    aria-label={ts('moveUp')}
                    isDisabled={i === 0}
                    onPress={() => move(i, -1)}
                  >
                    <ArrowUp size={14} />
                  </Button>
                  <Button
                    isIconOnly
                    size="sm"
                    variant="ghost"
                    aria-label={ts('moveDown')}
                    isDisabled={i === stages.length - 1}
                    onPress={() => move(i, 1)}
                  >
                    <ArrowDown size={14} />
                  </Button>
                  <Button
                    isIconOnly
                    size="sm"
                    variant="ghost"
                    aria-label={ts('deleteStage')}
                    onPress={async () => {
                      try {
                        await del.mutateAsync({ id: s.id });
                        await refresh();
                      } catch (err) {
                        toast.danger(
                          errorKind(err) === 'conflict'
                            ? ts('stageInUse')
                            : (serverMessage(err) ?? errorKind(err)),
                        );
                      }
                    }}
                  >
                    <Trash2 size={14} />
                  </Button>
                </div>
              </li>
            ))}
          </Stagger>
          <form
            className="flex items-end gap-2 border-t border-separator px-5 py-4"
            onSubmit={async (e) => {
              e.preventDefault();
              if (!newName.trim()) return;
              try {
                await create.mutateAsync({
                  id: list.id,
                  data: { name: newName.trim() },
                });
                setNewName('');
                await refresh();
              } catch (err) {
                fail(err);
              }
            }}
          >
            <div className="flex-1">
              <TextInput
                label={ts('newStage')}
                value={newName}
                onChange={setNewName}
                maxLength={100}
              />
            </div>
            <Button type="submit" variant="secondary">
              <Plus size={14} />
              {ts('add')}
            </Button>
          </form>
        </Panel>
      </div>
    </EditScope>
  );
}
