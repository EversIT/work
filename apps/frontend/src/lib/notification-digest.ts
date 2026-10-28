import type { NotificationOut } from '../api/generated/model';

/** How much a kind of notification matters: mentions and assignments first, so the inbox clears from the top. */
const WEIGHT: Record<string, number> = {
  mention: 100,
  assigned: 80,
  comment: 50,
  stage_updated: 30,
  unassigned: 20,
  subscription: 15,
  automation: 10,
};

export type DigestGroup = {
  /** The issue, or `null` for notifications that are not about one. */
  cardId: string | null;
  notifications: NotificationOut[];
  /** Number of notifications per type. */
  counts: Record<string, number>;
  /** The notification that matters most (the newest of the heaviest type): the first line of the summary. */
  lead: NotificationOut;
  score: number;
};

const weight = (n: NotificationOut) => WEIGHT[n.type] ?? 5;

/**
 * The unread notifications grouped by issue and ranked by importance. An issue with a mention outranks one
 * with only comments; among equals, more notifications and newer ones come first.
 */
export function digest(items: NotificationOut[], limit = 6): DigestGroup[] {
  const groups = new Map<string, NotificationOut[]>();
  for (const n of items.filter((x) => !x.is_read)) {
    const key =
      n.related_resource_type === 'card' && n.related_resource_id
        ? n.related_resource_id
        : '__other';
    groups.set(key, [...(groups.get(key) ?? []), n]);
  }
  return [...groups.entries()]
    .map(([key, notifications]): DigestGroup => {
      const counts: Record<string, number> = {};
      for (const n of notifications) counts[n.type] = (counts[n.type] ?? 0) + 1;
      const lead = [...notifications].sort(
        (a, b) =>
          weight(b) - weight(a) || b.created_at.localeCompare(a.created_at),
      )[0];
      const top = Math.max(...notifications.map(weight));
      return {
        cardId: key === '__other' ? null : key,
        notifications,
        counts,
        lead,
        score: top * 10 + Math.min(notifications.length, 9),
      };
    })
    .sort(
      (a, b) =>
        b.score - a.score || b.lead.created_at.localeCompare(a.lead.created_at),
    )
    .slice(0, limit);
}
