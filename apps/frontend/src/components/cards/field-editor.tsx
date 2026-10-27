'use client';

import { useState } from 'react';
import type { FieldOut } from '../../api/generated/model';
import { FieldInput } from './field-input';

type Member = { user_id: string; first_name: string; last_name: string };

const COMMIT_ON_BLUR = new Set([
  'text',
  'text_area',
  'rich',
  'email',
  'url',
  'number',
  'percentage',
  'currency',
]);

/**
 * Keeps a local draft and saves it: text-like inputs on blur, pickers/toggles immediately, so
 * typing never triggers a request per keystroke.
 */
export function FieldEditor({
  field,
  value,
  members,
  onCommit,
  label,
  hideLabel,
}: {
  field: FieldOut;
  value: unknown;
  members: Member[];
  onCommit: (v: unknown) => void;
  label?: string;
  hideLabel?: boolean;
}) {
  const [draft, setDraft] = useState<unknown>(value);
  // A new saved value replaces the draft. Done while rendering, which React supports for derived state.
  const [seen, setSeen] = useState<unknown>(value);
  if (seen !== value) {
    setSeen(value);
    setDraft(value);
  }
  const blur = COMMIT_ON_BLUR.has(field.type);
  return (
    <div
      onBlur={() => {
        if (
          blur &&
          JSON.stringify(draft ?? null) !== JSON.stringify(value ?? null)
        )
          onCommit(draft ?? null);
      }}
    >
      <FieldInput
        field={field}
        value={draft}
        members={members}
        label={label}
        hideLabel={hideLabel}
        onChange={(v) => {
          setDraft(v);
          if (!blur) onCommit(v);
        }}
      />
    </div>
  );
}
