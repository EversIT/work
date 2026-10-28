'use client';

import { Button, toast } from '@heroui/react';
import { Copy } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useSyncExternalStore } from 'react';
import { ReadonlyField } from '../ui/inputs';
import { Panel } from '../ui/settings';

const TOOLS = [
  ['list_projects', false],
  ['list_lists', false],
  ['search_issues', false],
  ['get_issue', false],
  ['find_related_issues', false],
  ['create_issue', true],
  ['update_issue', true],
  ['transition_issue', true],
  ['add_comment', true],
] as const;

const subscribe = () => () => {};

/**
 * The MCP server: how an AI agent (Claude Code, Cursor, any MCP client) works with the issues of
 * this instance. It uses a personal API token, so the agent can do what the token's owner can do,
 * and a read-only token cannot change anything. The server needs no language model and is on
 * whether or not the AI features are.
 */
export function McpSettings() {
  const t = useTranslations('mcp');
  const origin = useSyncExternalStore(
    subscribe,
    () => window.location.origin,
    () => '',
  );
  const url = `${origin}/api/v1/mcp`;
  // `<token>` is a placeholder: the person pastes their own token, created above.
  const command = `claude mcp add --transport http work ${url} --header "Authorization: Bearer <token>"`;
  const config = JSON.stringify(
    {
      mcpServers: {
        work: {
          type: 'http',
          url,
          headers: { Authorization: 'Bearer <token>' },
        },
      },
    },
    null,
    2,
  );
  const copy = (text: string) =>
    navigator.clipboard?.writeText(text).then(() => toast.success(t('copied')));

  return (
    <Panel data-testid="mcp-settings">
      <Panel.Header>
        <Panel.Title>{t('title')}</Panel.Title>
        <Panel.Description>{t('description')}</Panel.Description>
      </Panel.Header>
      <Panel.Content className="flex flex-col gap-4">
        <div>
          <p className="mb-1 text-sm font-medium">{t('endpoint')}</p>
          <div className="flex items-center gap-2">
            <ReadonlyField label={t('endpoint')} value={url} testId="mcp-url" />
            <Button
              isIconOnly
              variant="tertiary"
              aria-label={t('copy')}
              onPress={() => copy(url)}
            >
              <Copy size={14} />
            </Button>
          </div>
          <p className="mt-1 text-xs text-muted">{t('endpointHelp')}</p>
        </div>
        <div>
          <p className="mb-1 text-sm font-medium">{t('connect')}</p>
          <pre
            className="overflow-x-auto whitespace-pre-wrap break-all rounded-md bg-default p-3 text-xs"
            data-testid="mcp-command"
          >
            {command}
          </pre>
          <p className="mt-2 mb-1 text-xs text-muted">{t('otherClients')}</p>
          <pre className="overflow-x-auto rounded-md bg-default p-3 text-xs">
            {config}
          </pre>
          <Button
            size="sm"
            variant="tertiary"
            className="mt-1"
            onPress={() => copy(config)}
          >
            <Copy size={14} />
            {t('copy')}
          </Button>
        </div>
        <div>
          <p className="mb-1 text-sm font-medium">{t('tools')}</p>
          <ul
            className="divide-y divide-border text-sm"
            data-testid="mcp-tools"
          >
            {TOOLS.map(([name, writes]) => (
              <li key={name} className="flex items-baseline gap-3 py-1.5">
                <code className="w-44 shrink-0 text-xs">{name}</code>
                <span className="flex-1 text-muted">{t(`tool.${name}`)}</span>
                <span className="text-xs text-muted">
                  {writes ? t('writes') : t('reads')}
                </span>
              </li>
            ))}
          </ul>
          <p className="mt-2 text-xs text-muted">{t('safety')}</p>
        </div>
      </Panel.Content>
    </Panel>
  );
}
