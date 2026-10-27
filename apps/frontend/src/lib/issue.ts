import type { CardOut, CardTypeOut } from '../api/generated/model';
import type { Member } from '../components/views/types';
import { asStrings } from './cards';

export function issueType(card: CardOut, types: CardTypeOut[]) {
  return types.find((t) => t.id === card.card_type_id);
}

export function issueTitle(card: CardOut, types: CardTypeOut[]): string {
  const f = issueType(card, types)?.fields.find((x) => x.is_title);
  const v = f ? card.data[f.slug] : undefined;
  return typeof v === 'string' && v ? v : '—';
}

export function issueAssignees(
  card: CardOut,
  types: CardTypeOut[],
  members: Member[],
): Member[] {
  const f = issueType(card, types)?.fields.find((x) => x.is_assignee);
  if (!f) return [];
  return asStrings(card.data[f.slug])
    .map((id) => members.find((m) => m.user_id === id))
    .filter((m): m is Member => !!m);
}
