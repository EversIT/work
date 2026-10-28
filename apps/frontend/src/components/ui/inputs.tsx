'use client';

import { X } from 'lucide-react';
import { useTranslations } from 'next-intl';

import {
  Autocomplete,
  Button,
  FieldError,
  Input,
  Label,
  ListBox,
  Select,
  TextArea,
  TextField,
  SearchField,
} from '@heroui/react';

export function TextInput({
  label,
  name,
  value,
  onChange,
  type = 'text',
  required,
  maxLength,
  placeholder,
  defaultValue,
  hideLabel,
}: {
  label: string;
  name?: string;
  value?: string;
  defaultValue?: string;
  onChange?: (v: string) => void;
  type?: string;
  required?: boolean;
  maxLength?: number;
  placeholder?: string;
  /** Keep the label for screen readers but do not draw it. */
  hideLabel?: boolean;
}) {
  return (
    <TextField
      name={name}
      type={type}
      isRequired={required}
      maxLength={maxLength}
      value={value}
      defaultValue={defaultValue}
      onChange={onChange}
      fullWidth
    >
      <Label className={hideLabel ? 'sr-only' : undefined}>{label}</Label>
      <Input placeholder={placeholder} />
      <FieldError />
    </TextField>
  );
}

export function TextAreaInput({
  label,
  name,
  value,
  onChange,
  rows = 4,
  maxLength,
  defaultValue,
  placeholder,
  hideLabel,
}: {
  label: string;
  name?: string;
  value?: string;
  defaultValue?: string;
  onChange?: (v: string) => void;
  rows?: number;
  maxLength?: number;
  placeholder?: string;
  hideLabel?: boolean;
}) {
  return (
    <TextField
      name={name}
      value={value}
      defaultValue={defaultValue}
      onChange={onChange}
      maxLength={maxLength}
      fullWidth
    >
      <Label className={hideLabel ? 'sr-only' : undefined}>{label}</Label>
      <TextArea rows={rows} placeholder={placeholder} />
    </TextField>
  );
}

export type Option = { value: string; label: string };

/** From this many options on, a select gets a search field (people, labels, projects). */
const SEARCH_FROM = 7;

const contains = (text: string, input: string) =>
  text.toLowerCase().includes(input.trim().toLowerCase());

/** The search field and the filtered list inside an autocomplete's popover. */
function SearchList({
  options,
  allowEmpty,
}: {
  options: Option[];
  allowEmpty?: boolean;
}) {
  const t = useTranslations('common');
  return (
    <Autocomplete.Filter filter={contains}>
      <SearchField
        // oxlint-disable-next-line jsx-a11y/no-autofocus
        autoFocus
        name="search"
        variant="secondary"
        aria-label={t('search')}
      >
        <SearchField.Group>
          <SearchField.SearchIcon />
          <SearchField.Input
            placeholder={t('search')}
            data-testid="select-search"
          />
          <SearchField.ClearButton />
        </SearchField.Group>
      </SearchField>
      <ListBox
        renderEmptyState={() => (
          <p className="px-3 py-2 text-sm text-muted">{t('noMatches')}</p>
        )}
      >
        {allowEmpty && (
          <ListBox.Item id="__none" textValue="—">
            —<ListBox.ItemIndicator />
          </ListBox.Item>
        )}
        {options.map((o) => (
          <ListBox.Item key={o.value} id={o.value} textValue={o.label}>
            {o.label}
            <ListBox.ItemIndicator />
          </ListBox.Item>
        ))}
      </ListBox>
    </Autocomplete.Filter>
  );
}

/** A select with a search field: type to narrow a long list. */
function SearchableSelect({
  label,
  value,
  onChange,
  options,
  placeholder,
  allowEmpty,
  testId,
  hideLabel,
}: {
  label: string;
  value: string | null | undefined;
  onChange: (v: string | null) => void;
  options: Option[];
  placeholder?: string;
  allowEmpty?: boolean;
  testId?: string;
  hideLabel?: boolean;
}) {
  return (
    <Autocomplete
      selectionMode="single"
      value={value ?? null}
      onChange={(k) => onChange(k === null ? null : String(k))}
      placeholder={placeholder ?? label}
      fullWidth
      data-testid={testId}
    >
      <Label className={hideLabel ? 'sr-only' : undefined}>{label}</Label>
      <Autocomplete.Trigger>
        <Autocomplete.Value />
        <Autocomplete.Indicator />
      </Autocomplete.Trigger>
      <Autocomplete.Popover>
        <SearchList options={options} allowEmpty={allowEmpty} />
      </Autocomplete.Popover>
    </Autocomplete>
  );
}

export function SimpleSelect({
  label,
  value,
  onChange,
  options,
  placeholder,
  name,
  allowEmpty,
  testId,
  hideLabel,
}: {
  label: string;
  value: string | null | undefined;
  onChange: (v: string | null) => void;
  options: Option[];
  placeholder?: string;
  name?: string;
  allowEmpty?: boolean;
  testId?: string;
  /** The label stays for screen readers but is not drawn (for selects in table-like rows). */
  hideLabel?: boolean;
}) {
  if (options.length > SEARCH_FROM)
    return (
      <SearchableSelect
        label={label}
        value={value}
        onChange={onChange}
        options={options}
        placeholder={placeholder}
        allowEmpty={allowEmpty}
        testId={testId}
        hideLabel={hideLabel}
      />
    );
  return (
    <Select
      name={name}
      selectedKey={value ?? null}
      onSelectionChange={(k) => onChange(k === null ? null : String(k))}
      placeholder={placeholder ?? label}
      fullWidth
      data-testid={testId}
    >
      <Label className={hideLabel ? 'sr-only' : undefined}>{label}</Label>
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {allowEmpty && (
            <ListBox.Item id="__none" textValue="—">
              —<ListBox.ItemIndicator />
            </ListBox.Item>
          )}
          {options.map((o) => (
            <ListBox.Item key={o.value} id={o.value} textValue={o.label}>
              {o.label}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}

/**
 * Several values out of a list (labels, people): the chosen ones as chips that can be removed, and
 * a search field to add more, so a long list is never shown in full.
 */
export function MultiCheck({
  label,
  value,
  onChange,
  options,
  hideLabel,
}: {
  label: string;
  value: string[];
  onChange: (v: string[]) => void;
  options: Option[];
  hideLabel?: boolean;
}) {
  const t = useTranslations('common');
  const nameOf = (v: string) => options.find((o) => o.value === v)?.label ?? v;
  return (
    <div className="flex flex-col gap-1.5" data-testid="multi-select">
      {value.length > 0 && (
        <ul
          aria-label={label}
          className="flex flex-wrap gap-1"
          data-testid="selected-values"
        >
          {value.map((v) => (
            <li
              key={v}
              className="flex items-center gap-1 rounded-sm bg-accent-soft py-0.5 pl-2 pr-0.5 text-xs font-medium text-accent"
            >
              {nameOf(v)}
              <Button
                isIconOnly
                size="sm"
                variant="ghost"
                aria-label={t('removeValue', { value: nameOf(v) })}
                className="size-5 min-w-0 text-accent"
                onPress={() => onChange(value.filter((x) => x !== v))}
              >
                <X size={12} />
              </Button>
            </li>
          ))}
        </ul>
      )}
      <Autocomplete
        selectionMode="multiple"
        value={value}
        onChange={(keys) => onChange(keys.map(String))}
        placeholder={t('add')}
        fullWidth
      >
        <Label className={hideLabel ? 'sr-only' : undefined}>{label}</Label>
        <Autocomplete.Trigger>
          <Autocomplete.Value>{() => t('add')}</Autocomplete.Value>
          <Autocomplete.Indicator />
        </Autocomplete.Trigger>
        <Autocomplete.Popover>
          <SearchList options={options} />
        </Autocomplete.Popover>
      </Autocomplete>
    </div>
  );
}

/** A selectable, read-only value such as a token or an invite link. */
export function ReadonlyField({
  label,
  value,
  testId,
}: {
  label: string;
  value: string;
  testId?: string;
}) {
  return (
    <TextField fullWidth isReadOnly value={value} className="flex-1">
      <Label className="sr-only">{label}</Label>
      <Input
        data-testid={testId}
        className="font-mono text-xs"
        onFocus={(e) => e.currentTarget.select()}
      />
    </TextField>
  );
}

/** Search box with a clear button, used above lists and tables. */
export function SearchBox({
  label,
  value,
  onChange,
  testId,
  maxLength = 100,
  className = 'w-48',
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  testId?: string;
  maxLength?: number;
  className?: string;
}) {
  return (
    <SearchField
      aria-label={label}
      value={value}
      onChange={onChange}
      className={className}
    >
      <SearchField.Group>
        <SearchField.SearchIcon />
        <SearchField.Input
          placeholder={label}
          maxLength={maxLength}
          data-testid={testId}
        />
        <SearchField.ClearButton />
      </SearchField.Group>
    </SearchField>
  );
}
