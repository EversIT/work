'use client';

import { Button, Card, toast } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import { FormDialog } from './form-dialog';
import { TextInput } from './inputs';

const cx = (...parts: (string | false | null | undefined)[]) =>
  parts.filter(Boolean).join(' ');

/**
 * The container every settings page is made of: HeroUI's own `Card`, with its radius, surface and
 * shadow, so a theme changes settings together with the rest of the app. Only the padding is
 * taken over, so that header, rows and lists can run edge to edge.
 */
export function Panel({
  children,
  className,
  tone,
  testId,
  'data-testid': dataTestId,
}: {
  children: React.ReactNode;
  className?: string;
  tone?: 'danger';
  testId?: string;
  'data-testid'?: string;
}) {
  return (
    <Card
      data-testid={dataTestId ?? testId}
      className={cx(
        'gap-0 p-0 transition-shadow',
        tone === 'danger' && 'border border-danger/40',
        className,
      )}
    >
      {children}
    </Card>
  );
}

Panel.Header = function PanelHeader({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <Card.Header
      className={cx(
        'flex-row items-start justify-between gap-4 border-b border-separator px-5 py-4',
        className,
      )}
    >
      {children}
    </Card.Header>
  );
};

Panel.Title = function PanelTitle({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <Card.Title className={cx('text-sm', className)}>{children}</Card.Title>
  );
};

Panel.Description = function PanelDescription({
  children,
}: {
  children: React.ReactNode;
}) {
  return <Card.Description className="text-xs">{children}</Card.Description>;
};

Panel.Content = function PanelContent({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <Card.Content className={cx('px-5 py-4', className)}>
      {children}
    </Card.Content>
  );
};

/** Label and explanation on the left, the control on the right; stacks on small screens. */
export function SettingsRow({
  label,
  help,
  children,
}: {
  label: string;
  help?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="grid gap-x-10 gap-y-2 border-t border-separator px-5 py-4 first:border-t-0 md:grid-cols-[minmax(0,1fr)_minmax(0,24rem)]">
      <div className="min-w-0">
        <p className="text-sm font-medium text-foreground">{label}</p>
        {help && <p className="mt-0.5 text-xs text-muted">{help}</p>}
      </div>
      <div className="min-w-0 self-center">{children}</div>
    </div>
  );
}

/** Rows that fade in one after another. Put the rows directly inside. */
export function Stagger({
  children,
  className,
  as: Tag = 'div',
  testId,
}: {
  children: React.ReactNode;
  className?: string;
  as?: 'div' | 'ul' | 'ol';
  testId?: string;
}) {
  return (
    <Tag data-testid={testId} className={cx('settings-stagger', className)}>
      {children}
    </Tag>
  );
}

/** `style` for the n-th child of a `Stagger`. */
export const nth = (i: number) =>
  ({ '--i': Math.min(i, 12) }) as React.CSSProperties;

/** Page title with a tinted icon tile, as at the top of every settings page. */
export function PageHeader({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: React.ComponentType<{ size?: number }>;
  title: string;
  description: string;
  children?: React.ReactNode;
}) {
  return (
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
      <div className="flex items-start gap-3.5">
        <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent">
          <Icon size={20} />
        </span>
        <div>
          <h1 className="text-2xl font-semibold leading-tight text-foreground">
            {title}
          </h1>
          <p className="mt-1 max-w-xl text-sm text-muted">{description}</p>
        </div>
      </div>
      {children}
    </header>
  );
}

/** Grey bars where content will be, so the page does not jump when it arrives. */
export function SettingsSkeleton() {
  return (
    <div className="flex flex-col gap-4" aria-hidden>
      <div className="h-10 w-64 animate-pulse rounded-lg bg-default" />
      <div className="h-40 animate-pulse rounded-lg bg-default" />
      <div className="h-28 animate-pulse rounded-lg bg-default" />
    </div>
  );
}

/**
 * The bar that slides up when a form has changes that are not saved yet, with the two things you
 * can do about it. It appears only when it matters, and so the forms need no permanent save row.
 */
export function UnsavedBar({
  show,
  pending,
  onSave,
  onDiscard,
}: {
  show: boolean;
  pending?: boolean;
  onSave: () => void;
  onDiscard: () => void;
}) {
  const t = useTranslations('settingsui');
  if (!show) return null;
  return (
    <section
      aria-label={t('unsaved')}
      data-testid="unsaved-bar"
      className={cx(
        'fixed bottom-6 left-1/2 z-40 flex w-[calc(100%-2rem)] max-w-md items-center gap-3 rounded-xl border border-border bg-surface px-4 py-2.5 shadow-lg',
        'settings-bar-in',
      )}
    >
      <p className="flex-1 text-sm text-foreground">{t('unsaved')}</p>
      <Button
        size="sm"
        variant="tertiary"
        onPress={onDiscard}
        isDisabled={pending}
      >
        {t('discard')}
      </Button>
      <Button
        size="sm"
        onPress={onSave}
        isPending={pending}
        data-testid="save-changes"
      >
        {t('saveChanges')}
      </Button>
    </section>
  );
}

/**
 * For things that cannot be undone: the confirm button only works once the name has been typed,
 * which is slower than a browser dialog on purpose.
 */
export function ConfirmByTyping({
  isOpen,
  onClose,
  title,
  help,
  expected,
  confirmLabel,
  onConfirm,
}: {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  help: string;
  expected: string;
  confirmLabel: string;
  onConfirm: () => Promise<void>;
}) {
  const t = useTranslations('settingsui');
  const [typed, setTyped] = useState('');
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const close = () => {
    setTyped('');
    setError(null);
    onClose();
  };
  return (
    <FormDialog
      isOpen={isOpen}
      onClose={close}
      title={title}
      submitLabel={confirmLabel}
      isPending={pending}
      error={error}
      danger
      onSubmit={async () => {
        if (typed.trim() !== expected) return setError(t('mismatch'));
        setError(null);
        setPending(true);
        try {
          await onConfirm();
          close();
        } catch {
          toast.danger(t('failed'));
        } finally {
          setPending(false);
        }
      }}
    >
      <p className="text-sm text-foreground">{help}</p>
      <TextInput
        label={t('typeToConfirm', { name: expected })}
        value={typed}
        onChange={setTyped}
        placeholder={expected}
      />
    </FormDialog>
  );
}

/**
 * Turns every control inside off when the person may look but not change (a plain member of a
 * project, say). The server refuses such changes anyway; this only keeps the page honest.
 */
export function EditScope({
  canEdit,
  children,
}: {
  canEdit: boolean;
  children: React.ReactNode;
}) {
  return (
    <fieldset disabled={!canEdit} className="contents">
      {children}
    </fieldset>
  );
}
