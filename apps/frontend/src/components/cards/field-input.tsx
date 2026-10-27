'use client';

import { isEmptyDoc } from '../../lib/rich';
import { RichEditor } from '../rich/rich-editor';
import { Checkbox, Input, Label, TextField } from '@heroui/react';
import { useTranslations } from 'next-intl';
import type { FieldOut } from '../../api/generated/model';
import { asStrings } from '../../lib/cards';
import { fullName, toDateInput, toDateTimeInput } from '../../lib/format';
import {
  MultiCheck,
  SimpleSelect,
  TextAreaInput,
  TextInput,
} from '../ui/inputs';

type Member = { user_id: string; first_name: string; last_name: string };

export function itemsOf(field: FieldOut): string[] {
  const items = field.items as unknown;
  return Array.isArray(items)
    ? items.map((i) => String((i as { item: unknown }).item))
    : [];
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
  hideLabel,
}: {
  field: FieldOut;
  value: unknown;
  onChange: (v: unknown) => void;
  members: Member[];
  label?: string;
  /** Label only for screen readers (the caller draws its own, e.g. in a table-like row). */
  hideLabel?: boolean;
}) {
  const t = useTranslations('cards');
  const name = label ?? field.name;
  switch (field.type) {
    case 'text':
    case 'email':
    case 'url':
      return (
        <TextInput
          hideLabel={hideLabel}
          label={name}
          type={
            field.type === 'email'
              ? 'email'
              : field.type === 'url'
                ? 'url'
                : 'text'
          }
          value={typeof value === 'string' ? value : ''}
          onChange={(v) => onChange(v === '' ? null : v)}
          maxLength={field.type === 'text' ? 500 : 2000}
        />
      );
    case 'rich':
      return (
        <div className="flex flex-col gap-1">
          <span className="text-sm font-medium">{name}</span>
          <RichEditor
            label={name}
            value={value}
            testId={`rich-${field.slug}`}
            onChange={(doc) => onChange(isEmptyDoc(doc) ? null : doc)}
          />
        </div>
      );
    case 'text_area':
      return (
        <TextAreaInput
          hideLabel={hideLabel}
          label={name}
          value={typeof value === 'string' ? value : ''}
          onChange={(v) => onChange(v === '' ? null : v)}
          maxLength={20000}
        />
      );
    case 'number':
    case 'percentage':
    case 'currency':
      return (
        <TextField
          type="number"
          value={value === null || value === undefined ? '' : String(value)}
          onChange={(v) => onChange(v === '' ? null : Number(v))}
          fullWidth
        >
          <Label className={hideLabel ? 'sr-only' : undefined}>{name}</Label>
          <Input />
        </TextField>
      );
    case 'checkbox':
      return (
        <Checkbox
          className="flex flex-row items-center gap-2"
          isSelected={value === true}
          onChange={(v) => onChange(v)}
        >
          <Checkbox.Control>
            <Checkbox.Indicator />
          </Checkbox.Control>
          <Checkbox.Content>
            <Label>{name}</Label>
          </Checkbox.Content>
        </Checkbox>
      );
    case 'date':
      return (
        <TextInput
          hideLabel={hideLabel}
          label={name}
          type="date"
          value={toDateInput(value)}
          onChange={(v) => onChange(v === '' ? null : v)}
        />
      );
    case 'date_time':
      return (
        <TextInput
          hideLabel={hideLabel}
          label={name}
          type="datetime-local"
          value={toDateTimeInput(value)}
          onChange={(v) =>
            onChange(v === '' ? null : new Date(v).toISOString())
          }
        />
      );
    case 'dropdown':
    case 'label': {
      const options = itemsOf(field).map((i) => ({ value: i, label: i }));
      return field.multiple ? (
        <MultiCheck
          hideLabel={hideLabel}
          label={name}
          value={asStrings(value)}
          onChange={(v) => onChange(v.length ? v : null)}
          options={options}
        />
      ) : (
        <SimpleSelect
          hideLabel={hideLabel}
          label={name}
          value={asStrings(value)[0] ?? null}
          allowEmpty
          onChange={(v) => onChange(v === null || v === '__none' ? null : v)}
          options={options}
          placeholder={t('select')}
        />
      );
    }
    case 'user': {
      const options = members.map((m) => ({
        value: m.user_id,
        label: fullName(m),
      }));
      return field.multiple ? (
        <MultiCheck
          hideLabel={hideLabel}
          label={name}
          value={asStrings(value)}
          onChange={(v) => onChange(v.length ? v : null)}
          options={options}
        />
      ) : (
        <SimpleSelect
          hideLabel={hideLabel}
          label={name}
          value={asStrings(value)[0] ?? null}
          allowEmpty
          onChange={(v) => onChange(v === null || v === '__none' ? null : v)}
          options={options}
          placeholder={t('select')}
        />
      );
    }
    default:
      return null; // stage / card references are managed elsewhere
  }
}
