'use client';

import { useLocale } from 'next-intl';
import { issueAssignees, issueTitle, issueType } from '../../lib/issue';
import { stageOf } from '../../lib/cards';
import { formatDate } from '../../lib/format';
import { IssueTypeIcon, Lozenge, PointsBadge, PriorityIcon, stageKind } from '../jira/issue-ui';
import { UserAvatar } from '../shell/user-avatar';
import { useWorkspaceCtx } from '../shell/workspace-context';
import type { ViewProps } from './types';

/** Jira's List view: one dense row per issue with type, key, summary, status, priority and assignee. */
export function ListView({ list, stages, cards, members, onOpen }: ViewProps) {
  const locale = useLocale();
  const ctx = useWorkspaceCtx();
  return (
    <div className="overflow-hidden rounded-[3px] border border-[#DCDFE4]" data-testid="issue-list">
      <ul className="divide-y divide-[#DCDFE4]">
        {cards.map((card) => {
          const type = issueType(card, ctx.cardTypes);
          const stage = stages.find((s) => s.id === stageOf(card, list.id));
          const people = issueAssignees(card, ctx.cardTypes, members);
          return (
            <li key={card.id} className="flex items-center gap-3 px-3 py-2 hover:bg-[#F7F8F9]">
              <IssueTypeIcon icon={type?.icon} color={type?.color} title={type?.name} />
              <span className="w-20 shrink-0 text-xs font-medium text-[#626F86]">{card.key}</span>
              <button className="min-w-0 flex-1 truncate text-left text-sm hover:text-[#0C66E4] hover:underline" onClick={() => onOpen(card.id)}>
                {issueTitle(card, ctx.cardTypes)}
              </button>
              {typeof card.data.due_at === 'string' && <span className="hidden text-xs text-[#626F86] sm:inline">{formatDate(card.data.due_at, locale)}</span>}
              <PriorityIcon value={card.data.priority} />
              <PointsBadge value={card.data.story_points} />
              {stage && <Lozenge kind={stageKind(stage, stages[0]?.id === stage.id)}>{stage.name}</Lozenge>}
              <span className="w-6">{people[0] && <UserAvatar user={people[0]} size="sm" />}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
