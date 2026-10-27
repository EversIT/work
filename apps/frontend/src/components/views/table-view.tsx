'use client';

import { Table } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { issueAssignees, issueTitle, issueType } from '../../lib/issue';
import { stageOf } from '../../lib/cards';
import { FieldValue } from '../cards/field-value';
import {
  IssueTypeIcon,
  Lozenge,
  PriorityIcon,
  stageKind,
} from '../jira/issue-ui';
import { UserAvatar } from '../shell/user-avatar';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { groupCards } from '../../lib/grouping';
import { Grouped } from './grouped';
import type { ViewOptions, ViewProps } from './types';
import type { CardOut } from '../../api/generated/model';

export function TableView({
  list,
  stages,
  fields,
  cards,
  members,
  view,
  onOpen,
}: ViewProps) {
  const t = useTranslations('views');
  const ctx = useWorkspaceCtx();
  const opts = (view.options ?? {}) as ViewOptions;
  const shown = fields.filter(
    (f) =>
      !f.is_title &&
      f.type !== 'rich' &&
      f.type !== 'stage' &&
      !['priority', 'assignee'].includes(f.slug),
  );
  const columns = opts.columns?.length
    ? shown.filter((f) => opts.columns?.includes(f.slug))
    : shown.slice(0, 4);

  const groups = groupCards(cards, opts.groupBy, {
    listId: list.id,
    stages,
    fields,
    members,
    noValue: t('noValue'),
  });
  const grid = (items: CardOut[]) => (
    <Table>
      <Table.ScrollContainer>
        <Table.Content
          aria-label={t('tableLabel')}
          onRowAction={(key) => onOpen(String(key))}
        >
          <Table.Header>
            <Table.Column id="type">{t('type.label')}</Table.Column>
            <Table.Column id="key">{t('key')}</Table.Column>
            <Table.Column isRowHeader id="title">
              {t('summary')}
            </Table.Column>
            <Table.Column id="stage">{t('status')}</Table.Column>
            <Table.Column id="priority">{t('priority')}</Table.Column>
            <Table.Column id="assignee">{t('assignee')}</Table.Column>
            {columns.map((f) => (
              <Table.Column key={f.id} id={f.id}>
                {f.name}
              </Table.Column>
            ))}
          </Table.Header>
          <Table.Body
            items={items}
            renderEmptyState={() => (
              <div className="p-6 text-center text-muted">{t('empty')}</div>
            )}
          >
            {(card) => {
              const type = issueType(card, ctx.cardTypes);
              const stage = stages.find((s) => s.id === stageOf(card, list.id));
              const people = issueAssignees(card, ctx.cardTypes, members);
              return (
                <Table.Row id={card.id}>
                  <Table.Cell>
                    <IssueTypeIcon
                      icon={type?.icon}
                      color={type?.color}
                      title={type?.name}
                    />
                  </Table.Cell>
                  <Table.Cell>{card.key}</Table.Cell>
                  <Table.Cell>{issueTitle(card, ctx.cardTypes)}</Table.Cell>
                  <Table.Cell>
                    {stage && (
                      <Lozenge
                        kind={stageKind(stage, stages[0]?.id === stage.id)}
                      >
                        {stage.name}
                      </Lozenge>
                    )}
                  </Table.Cell>
                  <Table.Cell>
                    <PriorityIcon value={card.data.priority} />
                  </Table.Cell>
                  <Table.Cell>
                    {people[0] ? (
                      <span className="flex items-center gap-2">
                        <UserAvatar user={people[0]} size="sm" />
                        {people[0].first_name}
                      </span>
                    ) : (
                      '—'
                    )}
                  </Table.Cell>
                  {columns.map((f) => (
                    <Table.Cell key={f.id}>
                      <FieldValue
                        field={f}
                        value={card.data[f.slug]}
                        members={members}
                      />
                    </Table.Cell>
                  ))}
                </Table.Row>
              );
            }}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
  return (
    <Grouped groups={groups} all={grid(cards)} render={(g) => grid(g.cards)} />
  );
}
