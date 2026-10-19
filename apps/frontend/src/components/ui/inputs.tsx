'use client';

import { Checkbox, CheckboxGroup, FieldError, Input, Label, ListBox, Select, TextArea, TextField } from '@heroui/react';

export function TextInput({
  label,
  name,
  value,
  onChange,
  type = 'text',
  required,
  maxLength,
  placeholder,
  autoFocus,
  defaultValue,
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
  autoFocus?: boolean;
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
      autoFocus={autoFocus}
      fullWidth
    >
      <Label>{label}</Label>
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
}: {
  label: string;
  name?: string;
  value?: string;
  defaultValue?: string;
  onChange?: (v: string) => void;
  rows?: number;
  maxLength?: number;
  placeholder?: string;
}) {
  return (
    <TextField name={name} value={value} defaultValue={defaultValue} onChange={onChange} maxLength={maxLength} fullWidth>
      <Label>{label}</Label>
      <TextArea rows={rows} placeholder={placeholder} />
    </TextField>
  );
}

export type Option = { value: string; label: string };

export function SimpleSelect({
  label,
  value,
  onChange,
  options,
  placeholder,
  name,
  allowEmpty,
  testId,
}: {
  label: string;
  value: string | null | undefined;
  onChange: (v: string | null) => void;
  options: Option[];
  placeholder?: string;
  name?: string;
  allowEmpty?: boolean;
  testId?: string;
}) {
  return (
    <Select
      name={name}
      selectedKey={value ?? null}
      onSelectionChange={(k) => onChange(k === null ? null : String(k))}
      placeholder={placeholder ?? label}
      fullWidth
      data-testid={testId}
    >
      <Label>{label}</Label>
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

export function MultiCheck({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string[];
  onChange: (v: string[]) => void;
  options: Option[];
}) {
  return (
    <CheckboxGroup value={value} onChange={onChange} name={label}>
      <Label>{label}</Label>
      <div className="flex flex-wrap gap-x-4 gap-y-1">
        {options.map((o) => (
          <Checkbox className="flex flex-row items-center gap-2" key={o.value} value={o.value}>
            <Checkbox.Control>
              <Checkbox.Indicator />
            </Checkbox.Control>
            <Checkbox.Content><Label>{o.label}</Label></Checkbox.Content>
          </Checkbox>
        ))}
      </div>
    </CheckboxGroup>
  );
}
