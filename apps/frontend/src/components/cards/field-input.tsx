'use client';

import { Checkbox, Input, Label, TextField } from '@heroui/react';
import { useTranslations } from 'next-intl';
import type { FieldOut } from '../../api/generated/model';
import { asStrings } from '../../lib/cards';
import { fullName, toDateInput, toDateTimeInput } from '../../lib/format';
import { MultiCheck, SimpleSelect, TextAreaInput, TextInput } from '../ui/inputs';

type Member = { user_id: string; first_name: string; last_name: string };

export function itemsOf(field: FieldOut): string[] {
  const items = field.items as unknown;
  return Array.isArray(items) ? items.map((i) => String((i as { item: unknown }).item)) : [];
}

/**
 * Editor for one field, driven by the field definition. `onChange` receives the value in the shape
 * the API expects (string, number, boolean, or list of strings); `null` clears the field.
 */
export function FieldInput({
  field,
  value,
  onChange,
  members,
  label,
}: {
  field: FieldOut;
  value: unknown;
  onChange: (v: unknown) => void;
  members: Member[];
  label?: string;
}) {
  const t = useTranslations('cards');
  const name = label ?? field.name;
  switch (field.type) {
    case 'text':
    case 'email':
    case 'url':
      return (
        <TextInput
          label={name}
          type={field.type === 'email' ? 'email' : field.type === 'url' ? 'url' : 'text'}
          value={typeof value === 'string' ? value : ''}
          onChange={(v) => onChange(v === '' ? null : v)}
          maxLength={field.type === 'text' ? 500 : 2000}
        />
      );
    case 'text_area':
    case 'rich':
      return (
        <TextAreaInput
          label={name}
          value={typeof value === 'string' ? value : ''}
          onChange={(v) => onChange(v === '' ? null : v)}
          maxLength={field.type === 'rich' ? 100000 : 20000}
        />
      );
    case 'number':
    case 'percentage':
    case 'currency':
      return (
        <TextField type="number" value={value === null || value === undefined ? '' : String(value)} onChange={(v) => onChange(v === '' ? null : Number(v))} fullWidth>
          <Label>{name}</Label>
          <Input />
        </TextField>
      );
    case 'checkbox':
      return (
        <Checkbox className="flex flex-row items-center gap-2" isSelected={value === true} onChange={(v) => onChange(v)}>
          <Checkbox.Control>
            <Checkbox.Indicator />
          </Checkbox.Control>
          <Checkbox.Content><Label>{name}</Label></Checkbox.Content>
        </Checkbox>
      );
    case 'date':
      return <TextInput label={name} type="date" value={toDateInput(value)} onChange={(v) => onChange(v === '' ? null : v)} />;
    case 'date_time':
      return (
        <TextInput
          label={name}
          type="datetime-local"
          value={toDateTimeInput(value)}
          onChange={(v) => onChange(v === '' ? null : new Date(v).toISOString())}
        />
      );
    case 'dropdown':
    case 'label': {
      const options = itemsOf(field).map((i) => ({ value: i, label: i }));
      return field.multiple ? (
        <MultiCheck label={name} value={asStrings(value)} onChange={(v) => onChange(v.length ? v : null)} options={options} />
      ) : (
        <SimpleSelect label={name} value={asStrings(value)[0] ?? null} allowEmpty onChange={(v) => onChange(v === null || v === '__none' ? null : v)} options={options} placeholder={t('select')} />
      );
    }
    case 'user': {
      const options = members.map((m) => ({ value: m.user_id, label: fullName(m) }));
      return field.multiple ? (
        <MultiCheck label={name} value={asStrings(value)} onChange={(v) => onChange(v.length ? v : null)} options={options} />
      ) : (
        <SimpleSelect label={name} value={asStrings(value)[0] ?? null} allowEmpty onChange={(v) => onChange(v === null || v === '__none' ? null : v)} options={options} placeholder={t('select')} />
      );
    }
    default:
      return null; // stage / card references are managed elsewhere
  }
}
