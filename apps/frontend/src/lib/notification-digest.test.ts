import { describe, expect, it } from 'vitest';
import type { NotificationOut } from '../api/generated/model';
import { digest } from './notification-digest';

let n = 0;
const note = (
  type: string,
  card: string | null,
  at: string,
  read = false,
): NotificationOut =>
  ({
    id: `n${++n}`,
    type,
    message: `${type} on ${card}`,
    related_resource_type: card ? 'card' : null,
    related_resource_id: card,
    is_read: read,
    created_at: at,
  }) as unknown as NotificationOut;

describe('digest', () => {
  it('groups unread notifications by issue and ignores read ones', () => {
    const g = digest([
      note('comment', 'A', '2026-10-28T09:00:00Z'),
      note('comment', 'A', '2026-10-28T10:00:00Z'),
      note('comment', 'B', '2026-10-28T08:00:00Z'),
      note('mention', 'C', '2026-10-27T08:00:00Z', true),
    ]);
    expect(g.map((x) => [x.cardId, x.notifications.length])).toEqual([
      ['A', 2],
      ['B', 1],
    ]);
    expect(g[0].counts).toEqual({ comment: 2 });
  });

  it('ranks mentions and assignments before comments and status changes', () => {
    const g = digest([
      note('stage_updated', 'S', '2026-10-28T12:00:00Z'),
      note('comment', 'C', '2026-10-28T11:00:00Z'),
      note('assigned', 'A', '2026-10-28T07:00:00Z'),
      note('mention', 'M', '2026-10-28T06:00:00Z'),
    ]);
    expect(g.map((x) => x.cardId)).toEqual(['M', 'A', 'C', 'S']);
  });

  it('leads with the most important notification of an issue, and lets more of them break a tie', () => {
    const g = digest([
      note('comment', 'A', '2026-10-28T12:00:00Z'),
      note('mention', 'A', '2026-10-28T08:00:00Z'),
      note('comment', 'B', '2026-10-28T12:00:00Z'),
      note('comment', 'B', '2026-10-28T12:30:00Z'),
      note('comment', 'C', '2026-10-28T13:00:00Z'),
    ]);
    expect(g[0].cardId).toBe('A');
    expect(g[0].lead.type).toBe('mention');
    expect(g.map((x) => x.cardId)).toEqual(['A', 'B', 'C']);
  });

  it('collects what is not about an issue in one group, and limits the list', () => {
    const g = digest([
      note('system_announcement', null, '2026-10-28T09:00:00Z'),
      note('automation', null, '2026-10-28T10:00:00Z'),
    ]);
    expect(g).toHaveLength(1);
    expect(g[0].cardId).toBeNull();
    const many = digest(
      Array.from({ length: 20 }, (_, i) =>
        note('comment', `X${i}`, '2026-10-28T09:00:00Z'),
      ),
      5,
    );
    expect(many).toHaveLength(5);
  });

  it('is empty without unread notifications', () => {
    expect(digest([])).toEqual([]);
    expect(
      digest([note('mention', 'A', '2026-10-28T09:00:00Z', true)]),
    ).toEqual([]);
  });
});
