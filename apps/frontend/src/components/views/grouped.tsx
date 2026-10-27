'use client';

import { Button } from '@heroui/react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import type { Group } from '../../lib/grouping';

/**
 * Collapsible sections for a grouped list or table. Without groups it renders the content as is.
 * `render` draws one section's cards, so every view keeps its own look.
 */
export function Grouped({
  groups,
  all,
  render,
}: {
  groups: Group[] | null;
  all: React.ReactNode;
  render: (g: Group) => React.ReactNode;
}) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  if (!groups) return <>{all}</>;
  return (
    <div className="flex flex-col gap-4" data-testid="groups">
      {groups.map((g) => {
        const closed = collapsed[g.key] ?? false;
        return (
          <section
            key={g.key}
            data-testid={`group-${g.label}`}
            aria-label={g.label}
          >
            <Button
              type="button"
              aria-expanded={!closed}
              onPress={() => setCollapsed({ ...collapsed, [g.key]: !closed })}
              variant="ghost"
              size="sm"
              className="mb-1 px-1 font-semibold"
            >
              {closed ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
              {g.label}
              <span
                className="ml-1 rounded-full bg-border px-2 text-xs font-normal"
                data-testid="group-count"
              >
                {g.cards.length}
              </span>
            </Button>
            {!closed && render(g)}
          </section>
        );
      })}
    </div>
  );
}
