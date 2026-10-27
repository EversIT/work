'use client';

import { Button } from '@heroui/react';
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
export function BoardView({
  list,
  stages,
  cards,
  members,
  onOpen,
  onMove,
  activeSprintId,
}: ViewProps) {
  const t = useTranslations('views');
  const ctx = useWorkspaceCtx();
  const create = useCreateIssue();
  const [over, setOver] = useState<string | null>(null);

  return (
    <div
      className="flex items-start gap-3 overflow-x-auto px-1 pb-4 pt-1"
      data-testid="board"
    >
      {stages.map((stage) => {
        const inStage = cards.filter((c) => stageOf(c, list.id) === stage.id);
        return (
          <div
            key={stage.id}
            aria-label={stage.name}
            data-testid={`stage-${stage.name}`}
            className={`flex w-[272px] shrink-0 flex-col rounded-2xl bg-surface-secondary p-2 ${over === stage.id ? 'ring-2 ring-accent' : ''}`}
            onDragOver={(e) => {
              e.preventDefault();
              setOver(stage.id);
            }}
            onDragLeave={() => setOver((o) => (o === stage.id ? null : o))}
            onDrop={(e) => {
              e.preventDefault();
              setOver(null);
              const card = cards.find(
                (c) => c.id === e.dataTransfer.getData('text/card-id'),
              );
              if (card && stageOf(card, list.id) !== stage.id)
                onMove(card, stage.id);
            }}
          >
            <header className="px-1.5 pb-2 pt-1 text-xs font-semibold uppercase tracking-wide text-muted">
              {stage.name}{' '}
              <span className="ml-1 font-normal">{inStage.length}</span>
              {stage.is_completed && (
                <span className="ml-1 text-success">✓</span>
              )}
            </header>
            <div className="flex flex-col gap-1.5">
              {inStage.map((card) => {
                const type = issueType(card, ctx.cardTypes);
                const people = issueAssignees(card, ctx.cardTypes, members);
                return (
                  <button
                    type="button"
                    key={card.id}
                    draggable
                    data-testid={`card-${issueTitle(card, ctx.cardTypes)}`}
                    onDragStart={(e) =>
                      e.dataTransfer.setData('text/card-id', card.id)
                    }
                    onClick={() => onOpen(card.id)}
                    className="block w-full cursor-pointer rounded-xl border border-separator bg-surface p-3 text-left shadow-sm transition-shadow hover:shadow-md focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                  >
                    <p className="text-sm text-foreground">
                      {issueTitle(card, ctx.cardTypes)}
                    </p>
                    <div className="mt-3 flex items-center gap-2">
                      <IssueTypeIcon
                        icon={type?.icon}
                        color={type?.color}
                        title={type?.name}
                      />
                      <span className="text-xs font-medium text-muted">
                        {card.key}
                      </span>
                      <span className="ml-auto flex items-center gap-1.5">
                        <PriorityIcon value={card.data.priority} />
                        <PointsBadge value={card.data.story_points} />
                        {people[0] && <UserAvatar user={people[0]} size="sm" />}
                      </span>
                    </div>
                  </button>
                );
              })}
            </div>
            <Button
              onPress={() =>
                create.open({
                  listId: list.id,
                  stageId: stage.id,
                  sprintId:
                    list.methodology === 'scrum' ? activeSprintId : undefined,
                })
              }
              data-testid={`add-card-${stage.name}`}
              variant="ghost"
              size="sm"
              className="mt-1.5 justify-start text-muted"
            >
              <Plus size={14} />
              {t('createIssue')}
            </Button>
          </div>
        );
      })}
    </div>
  );
}
