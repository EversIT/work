'use client';

import { Alert, Button, Form } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useId } from 'react';
import { JiraDialog } from './jira-dialog';

export function FormDialog({
  isOpen,
  onClose,
  title,
  children,
  onSubmit,
  submitLabel,
  isPending,
  error,
  size = 'md',
  danger,
}: {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: React.ReactNode;
  onSubmit: (data: FormData) => void | Promise<void>;
  submitLabel?: string;
  isPending?: boolean;
  error?: string | null;
  size?: 'sm' | 'md' | 'lg' | 'xl' | 'xs' | 'cover' | 'full';
  danger?: boolean;
}) {
  const t = useTranslations('common');
  const formId = useId();
  const mapped =
    size === 'xs' ? 'sm' : size === 'cover' || size === 'full' ? 'xl' : size;
  return (
    <JiraDialog
      isOpen={isOpen}
      onClose={onClose}
      title={title}
      size={mapped}
      footer={
        <>
          <Button type="button" variant="tertiary" onPress={onClose}>
            {t('cancel')}
          </Button>
          <Button
            type="submit"
            form={formId}
            variant={danger ? 'danger' : 'primary'}
            isPending={isPending}
          >
            {submitLabel ?? t('save')}
          </Button>
        </>
      }
    >
      <Form
        id={formId}
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          void onSubmit(new FormData(e.currentTarget));
        }}
      >
        {error && (
          <Alert status="danger">
            <Alert.Content>
              <Alert.Description>{error}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {children}
      </Form>
    </JiraDialog>
  );
}
