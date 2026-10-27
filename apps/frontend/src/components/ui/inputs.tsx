'use client';

import {
  Checkbox,
  CheckboxGroup,
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
  return (
    <CheckboxGroup value={value} onChange={onChange} name={label}>
      <Label className={hideLabel ? 'sr-only' : undefined}>{label}</Label>
      <div className="flex flex-wrap gap-x-4 gap-y-1">
        {options.map((o) => (
          <Checkbox
            className="flex flex-row items-center gap-2"
            key={o.value}
            value={o.value}
          >
            <Checkbox.Control>
              <Checkbox.Indicator />
            </Checkbox.Control>
            <Checkbox.Content>
              <Label>{o.label}</Label>
            </Checkbox.Content>
          </Checkbox>
        ))}
      </div>
    </CheckboxGroup>
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
