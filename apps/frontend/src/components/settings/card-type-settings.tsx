'use client';

import { Button, Card, Chip, Switch, toast, Label } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Plus, Trash2 } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import { useCreateCardType, useDeleteCardType } from '../../api/generated/card-types/card-types';
import { useCreateField, useDeleteField, useUpdateField } from '../../api/generated/fields/fields';
import type { CardTypeOut } from '../../api/generated/model';
import { errorKind, serverMessage } from '../../lib/api';
import { useCardTypes } from '../../lib/hooks';
import { FormDialog } from '../ui/form-dialog';
import { SimpleSelect, TextInput } from '../ui/inputs';

const FIELD_TYPES = ['text', 'text_area', 'rich', 'number', 'percentage', 'currency', 'checkbox', 'date', 'date_time', 'dropdown', 'label', 'user', 'email', 'url'];
const LAYOUTS = ['default', 'person', 'organization', 'deal'];

export function CardTypeSettings({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations('settings');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const types = useCardTypes(workspaceId);
  const createType = useCreateCardType();
  const delType = useDeleteCardType();
  const createField = useCreateField();
  const updateField = useUpdateField();
  const delField = useDeleteField();
  const [typeDialog, setTypeDialog] = useState(false);
  const [fieldFor, setFieldFor] = useState<CardTypeOut | null>(null);
  const [fType, setFType] = useState<string | null>('text');
  const [layout, setLayout] = useState<string | null>('default');
  const [multiple, setMultiple] = useState(false);
  const [required, setRequired] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? te(errorKind(e)));
  const refresh = () => qc.invalidateQueries();

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-medium">{t('cardTypes')}</h2>
        <Button size="sm" onPress={() => { setError(null); setTypeDialog(true); }} data-testid="new-card-type"><Plus size={14} />{t('newCardType')}</Button>
      </div>
      {types.map((ct) => (
        <Card key={ct.id}>
          <Card.Header className="flex-row items-center justify-between">
            <Card.Title>{ct.name}</Card.Title>
            <div className="flex gap-2">
              <Button size="sm" variant="tertiary" onPress={() => { setError(null); setFType('text'); setMultiple(false); setRequired(false); setFieldFor(ct); }} data-testid={`new-field-${ct.name}`}><Plus size={14} />{t('newField')}</Button>
              <Button isIconOnly size="sm" variant="ghost" aria-label={t('deleteCardType')} onPress={async () => {
                if (!window.confirm(t('confirmDeleteCardType'))) return;
                try { await delType.mutateAsync({ id: ct.id }); await refresh(); } catch (e) { fail(e); }
              }}><Trash2 size={14} /></Button>
            </div>
          </Card.Header>
          <Card.Content>
            <ul className="divide-y divide-separator text-sm" data-testid={`fields-${ct.name}`}>
              {ct.fields.map((f) => (
                <li key={f.id} className="flex items-center gap-3 py-2">
                  <span className="flex-1">{f.name} <span className="text-xs text-muted">({f.slug})</span></span>
                  <Chip size="sm"><Chip.Label>{t(`fieldType.${f.type}`)}</Chip.Label></Chip>
                  {f.is_title && <Chip size="sm" color="accent"><Chip.Label>{t('titleField')}</Chip.Label></Chip>}
                  <Switch className="flex flex-row items-center gap-2" isSelected={f.required} isDisabled={f.is_title} onChange={async (v) => { try { await updateField.mutateAsync({ id: f.id, data: { required: v } }); await refresh(); } catch (e) { fail(e); } }}>
                    <Switch.Control><Switch.Thumb /></Switch.Control>
                    <Switch.Content><Label>{t('required')}</Label></Switch.Content>
                  </Switch>
                  {!f.is_title && (
                    <Button isIconOnly size="sm" variant="ghost" aria-label={t('deleteField')} onPress={async () => {
                      if (!window.confirm(t('confirmDeleteField'))) return;
                      try { await delField.mutateAsync({ id: f.id }); await refresh(); } catch (e) { fail(e); }
                    }}><Trash2 size={14} /></Button>
                  )}
                </li>
              ))}
            </ul>
          </Card.Content>
        </Card>
      ))}

      <FormDialog isOpen={typeDialog} onClose={() => setTypeDialog(false)} title={t('newCardType')} error={error} isPending={createType.isPending} submitLabel={t('create')}
        onSubmit={async (f) => {
          setError(null);
          try {
            await createType.mutateAsync({ id: workspaceId, data: { name: String(f.get('name')), layout: layout ?? 'default', has_children: false } });
            await refresh();
            setTypeDialog(false);
          } catch (e) { setError(serverMessage(e) ?? te(errorKind(e))); }
        }}>
        <TextInput label={t('name')} name="name" required maxLength={100} autoFocus />
        <SimpleSelect label={t('layout')} value={layout} onChange={setLayout} options={LAYOUTS.map((l) => ({ value: l, label: l }))} />
      </FormDialog>

      <FormDialog isOpen={!!fieldFor} onClose={() => setFieldFor(null)} title={t('newField')} error={error} isPending={createField.isPending} submitLabel={t('create')}
        onSubmit={async (f) => {
          setError(null);
          const items = String(f.get('items') ?? '').split(',').map((s) => s.trim()).filter(Boolean).map((item) => ({ item }));
          try {
            await createField.mutateAsync({
              id: workspaceId,
              data: { name: String(f.get('name')), type: fType ?? 'text', card_type_id: fieldFor?.id, required, multiple, items: ['dropdown', 'label'].includes(fType ?? '') ? (items as unknown as never) : undefined },
            });
            await refresh();
            setFieldFor(null);
          } catch (e) { setError(serverMessage(e) ?? te(errorKind(e))); }
        }}>
        <TextInput label={t('name')} name="name" required maxLength={100} autoFocus />
        <SimpleSelect label={t('type')} value={fType} onChange={setFType} options={FIELD_TYPES.map((x) => ({ value: x, label: t(`fieldType.${x}`) }))} testId="field-type" />
        {['dropdown', 'label'].includes(fType ?? '') && <TextInput label={t('options')} name="items" placeholder={t('optionsHint')} />}
        {['dropdown', 'label', 'user'].includes(fType ?? '') && (
          <Switch className="flex flex-row items-center gap-2" isSelected={multiple} onChange={setMultiple}><Switch.Control><Switch.Thumb /></Switch.Control><Switch.Content><Label>{t('multiple')}</Label></Switch.Content></Switch>
        )}
        <Switch className="flex flex-row items-center gap-2" isSelected={required} onChange={setRequired}><Switch.Control><Switch.Thumb /></Switch.Control><Switch.Content><Label>{t('required')}</Label></Switch.Content></Switch>
      </FormDialog>
    </div>
  );
}
