'use client';

import { Plus } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import { issueAssignees, issueTitle, issueType } from '../../lib/issue';
import { stageOf } from '../../lib/cards';
import { IssueTypeIcon, PointsBadge, PriorityIcon } from '../jira/issue-ui';
import { useCreateIssue } from '../shell/create-issue-context';
import { UserAvatar } from '../shell/user-avatar';
import { useWorkspaceCtx } from '../shell/workspace-context';
import type { ViewProps } from './types';

/** Jira board: one column per status, issue cards with type, key, priority, points and assignee. */
export function BoardView({ list, stages, cards, members, onOpen, onMove, activeSprintId }: ViewProps) {
  const t = useTranslations('views');
  const ctx = useWorkspaceCtx();
  const create = useCreateIssue();
  const [over, setOver] = useState<string | null>(null);

  return (
    <div className="flex items-start gap-2 overflow-x-auto pb-4" data-testid="board">
      {stages.map((stage) => {
        const inStage = cards.filter((c) => stageOf(c, list.id) === stage.id);
        return (
          <section
            key={stage.id}
            aria-label={stage.name}
            data-testid={`stage-${stage.name}`}
            className={`flex w-[272px] shrink-0 flex-col rounded-[3px] bg-[#F7F8F9] p-1.5 ${over === stage.id ? 'ring-2 ring-[#0C66E4]' : ''}`}
            onDragOver={(e) => {
              e.preventDefault();
              setOver(stage.id);
            }}
            onDragLeave={() => setOver((o) => (o === stage.id ? null : o))}
            onDrop={(e) => {
              e.preventDefault();
              setOver(null);
              const card = cards.find((c) => c.id === e.dataTransfer.getData('text/card-id'));
              if (card && stageOf(card, list.id) !== stage.id) onMove(card, stage.id);
            }}
          >
            <header className="px-1.5 pb-2 pt-1 text-xs font-semibold uppercase tracking-wide text-[#626F86]">
              {stage.name} <span className="ml-1 font-normal">{inStage.length}</span>
              {stage.is_completed && <span className="ml-1 text-[#1F845A]">✓</span>}
            </header>
            <div className="flex flex-col gap-1.5">
              {inStage.map((card) => {
                const type = issueType(card, ctx.cardTypes);
                const people = issueAssignees(card, ctx.cardTypes, members);
                return (
                  <article
                    key={card.id}
                    draggable
                    data-testid={`card-${issueTitle(card, ctx.cardTypes)}`}
                    onDragStart={(e) => e.dataTransfer.setData('text/card-id', card.id)}
                    onClick={() => onOpen(card.id)}
                    onKeyDown={(e) => e.key === 'Enter' && onOpen(card.id)}
                    tabIndex={0}
                    role="button"
                    className="cursor-pointer rounded-[3px] bg-white p-2.5 shadow-[0_1px_1px_#091e4240,0_0_1px_#091e424f] hover:bg-[#F7F8F9] focus:outline-none focus-visible:ring-2 focus-visible:ring-[#0C66E4]"
                  >
                    <p className="text-sm text-[#172B4D]">{issueTitle(card, ctx.cardTypes)}</p>
                    <div className="mt-3 flex items-center gap-2">
                      <IssueTypeIcon icon={type?.icon} color={type?.color} title={type?.name} />
                      <span className="text-xs font-medium text-[#626F86]">{card.key}</span>
                      <span className="ml-auto flex items-center gap-1.5">
                        <PriorityIcon value={card.data.priority} />
                        <PointsBadge value={card.data.story_points} />
                        {people[0] && <UserAvatar user={people[0]} size="sm" />}
                      </span>
                    </div>
                  </article>
                );
              })}
            </div>
            <button
              className="mt-1.5 flex h-8 items-center gap-1.5 rounded-[3px] px-2 text-sm text-[#44546F] hover:bg-[#091E420F]"
              onClick={() => create.open({ listId: list.id, stageId: stage.id, sprintId: list.methodology === 'scrum' ? activeSprintId : undefined })}
              data-testid={`add-card-${stage.name}`}
            >
              <Plus size={14} />
              {t('createIssue')}
            </button>
          </section>
        );
      })}
    </div>
  );
}
