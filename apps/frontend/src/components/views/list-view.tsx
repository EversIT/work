'use client';

import { Button } from '@heroui/react';
import { useLocale, useTranslations } from 'next-intl';
import { issueAssignees, issueTitle, issueType } from '../../lib/issue';
import { stageOf } from '../../lib/cards';
import { formatDate } from '../../lib/format';
import {
  IssueTypeIcon,
  Lozenge,
  PointsBadge,
  PriorityIcon,
  stageKind,
} from '../jira/issue-ui';
import { UserAvatar } from '../shell/user-avatar';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { groupCards } from '../../lib/grouping';
import { Grouped } from './grouped';
import type { ViewOptions, ViewProps } from './types';

/** Jira's List view: one dense row per issue with type, key, summary, status, priority and assignee. */
export function ListView({
  list,
  stages,
  fields,
  cards,
  members,
  view,
  onOpen,
}: ViewProps) {
  const locale = useLocale();
  const t = useTranslations('views');
  const ctx = useWorkspaceCtx();
  const groups = groupCards(
    cards,
    ((view.options ?? {}) as ViewOptions).groupBy,
    { listId: list.id, stages, fields, members, noValue: t('noValue') },
  );
  const rows = (items: typeof cards) => (
    <div
      className="overflow-hidden rounded-xl border border-border"
      data-testid="issue-list"
    >
      <ul className="divide-y divide-border">
        {items.map((card) => {
          const type = issueType(card, ctx.cardTypes);
          const stage = stages.find((s) => s.id === stageOf(card, list.id));
          const people = issueAssignees(card, ctx.cardTypes, members);
          return (
            <li
              key={card.id}
              className="flex items-center gap-3 px-3 py-2 hover:bg-surface-secondary"
            >
              <IssueTypeIcon
                icon={type?.icon}
                color={type?.color}
                title={type?.name}
              />
              <span className="w-20 shrink-0 text-xs font-medium text-muted">
                {card.key}
              </span>
              <Button
                onPress={() => onOpen(card.id)}
                variant="ghost"
                size="sm"
                className="h-auto min-w-0 justify-start px-1 py-0.5 font-normal flex-1"
              >
                <span className="truncate">
                  {issueTitle(card, ctx.cardTypes)}
                </span>
              </Button>
              {typeof card.data.due_at === 'string' && (
                <span className="hidden text-xs text-muted sm:inline">
                  {formatDate(card.data.due_at, locale)}
                </span>
              )}
              <PriorityIcon value={card.data.priority} />
              <PointsBadge value={card.data.story_points} />
              {stage && (
                <Lozenge kind={stageKind(stage, stages[0]?.id === stage.id)}>
                  {stage.name}
                </Lozenge>
              )}
              <span className="w-6">
                {people[0] && <UserAvatar user={people[0]} size="sm" />}
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
  return (
    <Grouped groups={groups} all={rows(cards)} render={(g) => rows(g.cards)} />
  );
}
