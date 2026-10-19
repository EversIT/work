'use client';

import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import { useCompleteSprint, useStartSprint } from '../../api/generated/sprints/sprints';
import type { SprintOut } from '../../api/generated/model';
import { errorKind, serverMessage } from '../../lib/api';
import { FormDialog } from '../ui/form-dialog';
import { SimpleSelect, TextInput } from '../ui/inputs';

export function StartSprintDialog({ sprint, onClose }: { sprint: SprintOut; onClose: () => void }) {
  const t = useTranslations('backlog');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const start = useStartSprint();
  const [weeks, setWeeks] = useState<string | null>('2');
  const [error, setError] = useState<string | null>(null);
  const today = new Date().toISOString().slice(0, 10);
  return (
    <FormDialog
      isOpen
      onClose={onClose}
      title={t('startSprint', { name: sprint.name })}
      submitLabel={t('start')}
      isPending={start.isPending}
      error={error}
      onSubmit={async (f) => {
        setError(null);
        const from = new Date(`${String(f.get('start') || today)}T09:00:00`);
        const to = new Date(from.getTime() + Number(weeks ?? '2') * 7 * 86_400_000);
        try {
          await start.mutateAsync({ id: sprint.id, data: { starts_at: from.toISOString(), ends_at: to.toISOString() } });
          await qc.invalidateQueries();
          onClose();
        } catch (e) {
          setError(errorKind(e) === 'conflict' ? t('alreadyActive') : (serverMessage(e) ?? te(errorKind(e))));
        }
      }}
    >
      <p className="text-sm text-muted">{t('issuesInSprint', { count: sprint.issue_count })}</p>
      <SimpleSelect label={t('duration')} value={weeks} onChange={setWeeks} options={['1', '2', '3', '4'].map((w) => ({ value: w, label: t('weeks', { count: Number(w) }) }))} testId="sprint-duration" />
      <TextInput label={t('startDate')} name="start" type="date" defaultValue={today} />
    </FormDialog>
  );
}

export function CompleteSprintDialog({ sprint, planned, onClose }: { sprint: SprintOut; planned: SprintOut[]; onClose: () => void }) {
  const t = useTranslations('backlog');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const complete = useCompleteSprint();
  const [target, setTarget] = useState<string | null>('__backlog');
  const [error, setError] = useState<string | null>(null);
  const open = sprint.issue_count - sprint.done_count;
  return (
    <FormDialog
      isOpen
      onClose={onClose}
      title={t('completeSprint', { name: sprint.name })}
      submitLabel={t('complete')}
      isPending={complete.isPending}
      error={error}
      onSubmit={async () => {
        setError(null);
        try {
          await complete.mutateAsync({ id: sprint.id, data: { move_to: target && target !== '__backlog' ? target : null } });
          await qc.invalidateQueries();
          onClose();
        } catch (e) {
          setError(serverMessage(e) ?? te(errorKind(e)));
        }
      }}
    >
      <p className="text-sm">{t('completeSummary', { done: sprint.done_count, open })}</p>
      {open > 0 && (
        <SimpleSelect
          label={t('moveOpenTo')}
          value={target}
          onChange={setTarget}
          options={[{ value: '__backlog', label: t('backlog') }, ...planned.map((p) => ({ value: p.id, label: p.name }))]}
          testId="complete-target"
        />
      )}
    </FormDialog>
  );
}
