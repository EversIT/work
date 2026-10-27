'use client';

import { useQuery } from '@tanstack/react-query';
import { searchCards } from '../api/generated/cards/cards';
import type { CardOut, FieldOut, SearchBody } from '../api/generated/model';
import { body } from './api';

export function useCards(listId: string | undefined, search: SearchBody) {
  return useQuery({
    queryKey: ['cards', listId, search],
    enabled: !!listId,
    queryFn: async () => {
      const res = await searchCards(listId as string, search);
      return body(res) ?? { total: 0, cards: [] };
    },
    placeholderData: (prev) => prev,
  });
}

export const titleField = (fields: FieldOut[]) =>
  fields.find((f) => f.is_title);
export const assigneeField = (fields: FieldOut[]) =>
  fields.find((f) => f.is_assignee);

export function cardTitle(card: CardOut, fields: FieldOut[]): string {
  const f = titleField(fields);
  const v = f ? card.data[f.slug] : undefined;
  return typeof v === 'string' && v ? v : '—';
}

export function stageOf(card: CardOut, listId: string): string | undefined {
  return card.lists.find((l) => l.list_id === listId)?.list_stage_id;
}

export function asStrings(v: unknown): string[] {
  if (Array.isArray(v))
    return v.filter((x): x is string => typeof x === 'string');
  return typeof v === 'string' && v ? [v] : [];
}
