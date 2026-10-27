'use client';

import { useState } from 'react';

/**
 * The summary of an issue as a heading you can edit in place. It is saved when you leave it or
 * press Enter, and Escape takes the change back.
 */
export function InlineTitle({
  value,
  label,
  onCommit,
  disabled,
}: {
  value: string;
  label: string;
  onCommit: (v: string) => void;
  disabled?: boolean;
}) {
  const [draft, setDraft] = useState(value);
  const [seen, setSeen] = useState(value);
  if (seen !== value) {
    setSeen(value);
    setDraft(value);
  }
  const commit = () => {
    const v = draft.trim();
    if (!v) return setDraft(value);
    if (v !== value) onCommit(v);
  };
  return (
    <input
      aria-label={label}
      value={draft}
      disabled={disabled}
      maxLength={500}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') e.currentTarget.blur();
        if (e.key === 'Escape') {
          setDraft(value);
          e.currentTarget.blur();
        }
      }}
      className="w-full rounded-xl border border-transparent bg-transparent px-2 py-1.5 text-2xl font-semibold leading-snug text-foreground outline-none transition-colors placeholder:text-muted hover:bg-default focus:border-field-border-focus focus:bg-field disabled:hover:bg-transparent"
    />
  );
}
