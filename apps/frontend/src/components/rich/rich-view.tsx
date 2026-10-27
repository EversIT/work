import type { JSONContent } from '@tiptap/core';
import type { ReactNode } from 'react';
import { safeHref, toDoc } from '../../lib/rich';

/**
 * Draws a stored document with React elements. No HTML string is ever built or injected, so a
 * document cannot carry markup or script, whatever it contains; links are limited to http(s) and
 * mailto and always get `rel="noopener noreferrer nofollow"`.
 */
export function RichView({
  value,
  className,
}: {
  value: unknown;
  className?: string;
}) {
  const doc = toDoc(value);
  return (
    <div
      className={className ?? 'flex flex-col gap-2 break-words text-sm'}
      data-testid="rich-view"
    >
      {renderNodes(doc.content ?? [])}
    </div>
  );
}

function renderNodes(nodes: JSONContent[]): ReactNode {
  return nodes.map((n, i) => <Node key={i} node={n} />);
}

function Node({ node }: { node: JSONContent }): ReactNode {
  const kids = renderNodes(node.content ?? []);
  switch (node.type) {
    case 'paragraph':
      return <p className="min-h-[1.25em] whitespace-pre-wrap">{kids}</p>;
    case 'heading': {
      const level = Math.min(3, Math.max(1, Number(node.attrs?.level) || 1));
      const cls = [
        'text-xl font-semibold',
        'text-lg font-semibold',
        'text-base font-semibold',
      ][level - 1];
      return level === 1 ? (
        <h2 className={cls}>{kids}</h2>
      ) : level === 2 ? (
        <h3 className={cls}>{kids}</h3>
      ) : (
        <h4 className={cls}>{kids}</h4>
      );
    }
    case 'bulletList':
      return <ul className="list-disc pl-6">{kids}</ul>;
    case 'orderedList':
      return (
        <ol
          className="list-decimal pl-6"
          start={Number(node.attrs?.start) || 1}
        >
          {kids}
        </ol>
      );
    case 'listItem':
      return <li>{kids}</li>;
    case 'blockquote':
      return (
        <blockquote className="border-l-4 border-border pl-3 text-muted">
          {kids}
        </blockquote>
      );
    case 'codeBlock':
      return (
        <pre className="overflow-x-auto rounded bg-surface-tertiary p-2 font-mono text-xs">
          <code>{kids}</code>
        </pre>
      );
    case 'hardBreak':
      return <br />;
    case 'horizontalRule':
      return <hr className="border-border" />;
    case 'text':
      return <Marked node={node} />;
    default:
      return null; // unknown nodes are skipped, never rendered
  }
}

function Marked({ node }: { node: JSONContent }): ReactNode {
  let out: ReactNode = node.text ?? '';
  for (const mark of node.marks ?? []) {
    switch (mark.type) {
      case 'bold':
        out = <strong>{out}</strong>;
        break;
      case 'italic':
        out = <em>{out}</em>;
        break;
      case 'strike':
        out = <s>{out}</s>;
        break;
      case 'underline':
        out = <u>{out}</u>;
        break;
      case 'code':
        out = (
          <code className="rounded bg-surface-tertiary px-1 font-mono text-[0.9em]">
            {out}
          </code>
        );
        break;
      case 'link': {
        const href = safeHref(mark.attrs?.href);
        out = href ? (
          <a
            href={href}
            target="_blank"
            rel="noopener noreferrer nofollow"
            className="text-accent underline"
          >
            {out}
          </a>
        ) : (
          out
        );
        break;
      }
    }
  }
  return out;
}
