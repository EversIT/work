'use client';

import {
  Bookmark,
  Bug,
  Check,
  ChevronDown,
  ChevronUp,
  ChevronsDown,
  ChevronsUp,
  GitBranch,
  Minus,
  Zap,
  type LucideIcon,
} from 'lucide-react';

const TYPE_ICON: Record<string, LucideIcon> = { epic: Zap, story: Bookmark, task: Check, bug: Bug, subtask: GitBranch };
const TYPE_BG: Record<string, string> = {
  purple: '#904EE2',
  green: '#4BCE97',
  blue: '#4688EC',
  red: '#E5493A',
  orange: '#F18D13',
  teal: '#2898BD',
  grey: '#8590A2',
};

/** Jira's issue type glyph: a small coloured rounded square with a white icon. */
export function IssueTypeIcon({ icon, color, size = 16, title }: { icon?: string; color?: string; size?: number; title?: string }) {
  const Icon = TYPE_ICON[icon ?? 'task'] ?? Check;
  return (
    <span
      title={title}
      aria-label={title}
      role={title ? 'img' : undefined}
      className="inline-flex shrink-0 items-center justify-center rounded-[3px] text-white"
      style={{ width: size, height: size, background: TYPE_BG[color ?? 'blue'] ?? TYPE_BG.blue }}
    >
      <Icon size={Math.round(size * 0.7)} strokeWidth={3} />
    </span>
  );
}

const PRIORITY: Record<string, { Icon: LucideIcon; color: string }> = {
  highest: { Icon: ChevronsUp, color: '#C9372C' },
  high: { Icon: ChevronUp, color: '#E2483D' },
  medium: { Icon: Minus, color: '#E56910' },
  low: { Icon: ChevronDown, color: '#2684FF' },
  lowest: { Icon: ChevronsDown, color: '#2684FF' },
};

export function PriorityIcon({ value, size = 16 }: { value?: unknown; size?: number }) {
  const key = typeof value === 'string' ? value.toLowerCase() : '';
  const p = PRIORITY[key];
  if (!p) return null;
  const { Icon, color } = p;
  return (
    <span title={String(value)} aria-label={`Priority ${String(value)}`} role="img" className="inline-flex shrink-0" style={{ color }}>
      <Icon size={size} strokeWidth={3} />
    </span>
  );
}

const LOZENGE = {
  todo: { bg: '#DFE1E6', fg: '#42526E' },
  progress: { bg: '#CCE0FF', fg: '#0747A6' },
  done: { bg: '#DCFFF1', fg: '#216E4E' },
} as const;

export type LozengeKind = keyof typeof LOZENGE;

export function stageKind(stage: { is_completed: boolean }, isFirst: boolean): LozengeKind {
  if (stage.is_completed) return 'done';
  return isFirst ? 'todo' : 'progress';
}

/** The status "lozenge": bold upper-case text on a tinted background. */
export function Lozenge({ children, kind = 'todo', className = '', trailing }: { children: React.ReactNode; kind?: LozengeKind; className?: string; trailing?: React.ReactNode }) {
  const c = LOZENGE[kind];
  return (
    <span
      className={`inline-flex max-w-full items-center rounded-[3px] px-1 py-px text-[11px] font-bold uppercase leading-4 tracking-wide ${className}`}
      style={{ background: c.bg, color: c.fg }}
    >
      <span className="truncate">{children}</span>
      {trailing}
    </span>
  );
}

/** Story points bubble, as shown on cards and backlog rows. */
export function PointsBadge({ value }: { value: unknown }) {
  if (typeof value !== 'number') return null;
  return (
    <span title="Story points" className="inline-flex min-w-5 items-center justify-center rounded-full bg-[#DFE1E6] px-1.5 text-[11px] font-semibold leading-5 text-[#42526E]">
      {value}
    </span>
  );
}
