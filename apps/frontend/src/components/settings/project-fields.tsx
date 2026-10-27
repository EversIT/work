'use client';

import { Checkbox, Label, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useSetListFields } from '../../api/generated/lists/lists';
import { useListFields } from '../../api/generated/fields/fields';
import type { ListDetail } from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { EditScope, Panel, Stagger, nth } from '../ui/settings';

/** Which fields show up on the issues of this project. Summary and description are always on. */
export function ProjectFields({
  list,
  workspaceId,
  canEdit,
}: {
  list: ListDetail;
  workspaceId: string;
  canEdit: boolean;
}) {
  const t = useTranslations('psettings');
  const qc = useQueryClient();
  const set = useSetListFields();
  const fields = body(useListFields(workspaceId).data) ?? [];
  // A field can belong to one issue type; the same name may then appear once per type.
  const chosen = new Set(list.field_ids);
  const locked = (f: { is_title: boolean; is_description: boolean }) =>
    f.is_title || f.is_description;

  const toggle = async (id: string, on: boolean) => {
    const next = new Set(chosen);
    if (on) next.add(id);
    else next.delete(id);
    try {
      await set.mutateAsync({ id: list.id, data: { field_ids: [...next] } });
      await qc.invalidateQueries();
    } catch (e) {
      toast.danger(serverMessage(e) ?? errorKind(e));
    }
  };

  return (
    <EditScope canEdit={canEdit}>
      <Panel testId="project-fields">
        <Panel.Header>
          <div>
            <Panel.Title>{t('fieldsTitle')}</Panel.Title>
            <Panel.Description>{t('fieldsHelp')}</Panel.Description>
          </div>
        </Panel.Header>
        <Stagger as="ul" className="divide-y divide-separator">
          {fields.map((f, i) => (
            <li
              key={f.id}
              style={nth(i)}
              className="flex items-center gap-3 px-5 py-3 transition-colors hover:bg-surface-secondary"
            >
              <Checkbox
                isSelected={chosen.has(f.id) || locked(f)}
                isDisabled={locked(f)}
                onChange={(v) => void toggle(f.id, v)}
                aria-label={f.name}
              >
                <Checkbox.Control>
                  <Checkbox.Indicator />
                </Checkbox.Control>
                <Checkbox.Content>
                  <Label>{f.name}</Label>
                </Checkbox.Content>
              </Checkbox>
              <span className="ml-auto text-xs text-muted">{f.slug}</span>
            </li>
          ))}
        </Stagger>
      </Panel>
    </EditScope>
  );
}
