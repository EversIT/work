'use client';

import { routes } from '../../lib/routes';
import {
  Button,
  Chip,
  Dropdown,
  Switch,
  Table,
  buttonVariants,
  toast,
} from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { ArrowRight, Copy, MoreHorizontal, Plus, Zap } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import {
  getSigningSecret,
  regenerateWebhookToken,
  useDeleteAutomation,
  useListAutomations,
  useUpdateAutomation,
} from '../../api/generated/automations/automations';
import type { AutomationOut } from '../../api/generated/model';
import { useGetWorkspaceTree } from '../../api/generated/spaces/spaces';
import { body, errorKind, serverMessage } from '../../lib/api';
import { useWorkspace } from '../../lib/hooks';
import { FormDialog } from '../ui/form-dialog';
import { JiraDialog } from '../ui/jira-dialog';
import { ReadonlyField } from '../ui/inputs';
import { Panel } from '../ui/settings';
import { Runs } from './parts';

/**
 * The rules of a workspace, or of one project. Creating or editing a rule opens its own page (the
 * builder); this list is for the overview, switching rules on and off, and the odd jobs around a
 * rule: its runs, its webhook address and its signing key.
 */
export function AutomationList({
  workspaceId,
  listId,
}: {
  workspaceId: string;
  /** Only the rules of this project (rules for the whole workspace are counted, not listed). */
  listId?: string;
}) {
  const t = useTranslations('automations');
  const te = useTranslations('errors');
  const locale = useLocale();
  const router = useRouter();
  const qc = useQueryClient();
  const tree = body(useGetWorkspaceTree(workspaceId).data);
  const all = body(useListAutomations(workspaceId).data) ?? [];
  const update = useUpdateAutomation();
  const del = useDeleteAutomation();
  const [runsFor, setRunsFor] = useState<string | null>(null);
  const [secret, setSecret] = useState<{
    title: string;
    value: string;
    hint: string;
  } | null>(null);
  const lists = tree?.spaces.flatMap((s) => s.lists) ?? [];
  const lists_of = (a: AutomationOut) =>
    a.locations.filter((l) => l.location_type === 'list');
  const items = listId
    ? all.filter((a) => lists_of(a).some((l) => l.location_id === listId))
    : all;
  const workspaceWide = listId
    ? all.filter((a) => a.locations.length === 0).length
    : 0;
  const refresh = () => qc.invalidateQueries();
  const fail = (e: unknown) =>
    toast.danger(serverMessage(e) ?? te(errorKind(e)));
  const wsKey = useWorkspace(workspaceId)?.key_prefix ?? '';
  const open = (id: string) =>
    router.push(routes.automation(wsKey, id, listId));
  const scopeName = (a: AutomationOut) => {
    const ls = lists_of(a);
    if (ls.length === 0) return t('allLists');
    return ls
      .map((l) => lists.find((x) => x.id === l.location_id)?.name ?? '…')
      .join(', ');
  };
  const runsAutomation = items.find((a) => a.id === runsFor);

  const rotateWebhook = async (a: AutomationOut) => {
    if (a.has_webhook && !window.confirm(t('confirmRotate'))) return;
    try {
      const r = await regenerateWebhookToken(a.id);
      if (r.status === 200)
        setSecret({
          title: t('webhookUrl'),
          value: `${window.location.origin}${r.data.path}`,
          hint: t('webhookUrlHint'),
        });
      await refresh();
    } catch (e) {
      fail(e);
    }
  };
  const showSigningSecret = async (a: AutomationOut) => {
    try {
      const r = await getSigningSecret(a.id);
      if (r.status === 200)
        setSecret({
          title: t('signingSecret'),
          value: r.data.secret,
          hint: t('signingSecretHint'),
        });
    } catch (e) {
      fail(e);
    }
  };
  const remove = async (a: AutomationOut) => {
    if (!window.confirm(t('confirmDelete'))) return;
    try {
      await del.mutateAsync({ id: a.id });
      await refresh();
    } catch (e) {
      fail(e);
    }
  };
  const menuAction = (a: AutomationOut, key: string) => {
    if (key === 'edit') open(a.id);
    if (key === 'runs') setRunsFor(a.id);
    if (key === 'webhook') void rotateWebhook(a);
    if (key === 'secret') void showSigningSecret(a);
    if (key === 'delete') void remove(a);
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start justify-between gap-4">
        <p className="text-sm text-muted">{t('adminOnly')}</p>
        <Button
          size="sm"
          onPress={() => open('new')}
          data-testid="new-automation"
        >
          <Plus size={14} />
          {t('new')}
        </Button>
      </div>

      {items.length === 0 ? (
        <Panel>
          <Panel.Content className="flex flex-col items-center gap-2 py-10 text-center">
            <span className="flex size-12 items-center justify-center rounded-xl bg-accent-soft text-accent">
              <Zap size={22} />
            </span>
            <p className="font-medium">{t('emptyTitle')}</p>
            <p className="max-w-sm text-sm text-muted">
              {listId ? t('emptyProject') : t('empty')}
            </p>
            <Button className="mt-2" onPress={() => open('new')}>
              <Plus size={14} />
              {t('new')}
            </Button>
          </Panel.Content>
        </Panel>
      ) : (
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label={t('title')}>
              <Table.Header>
                <Table.Column isRowHeader id="name">
                  {t('name')}
                </Table.Column>
                <Table.Column id="rule">{t('rule')}</Table.Column>
                {!listId && (
                  <Table.Column id="scope">{t('scope')}</Table.Column>
                )}
                <Table.Column id="enabled">{t('enabled')}</Table.Column>
                <Table.Column id="menu" className="w-12">
                  <span className="sr-only">{t('manage')}</span>
                </Table.Column>
              </Table.Header>
              <Table.Body>
                {items.map((a) => (
                  <Table.Row
                    key={a.id}
                    id={a.id}
                    data-testid={`automation-${a.name}`}
                  >
                    <Table.Cell className="font-medium">
                      <Link
                        href={routes.automation(wsKey, a.id, listId)}
                        className="hover:text-accent hover:underline"
                      >
                        {a.name}
                      </Link>
                    </Table.Cell>
                    <Table.Cell>
                      <div className="flex flex-wrap items-center gap-1.5">
                        <Chip size="sm" color="accent" variant="soft">
                          <Chip.Label>
                            {t(`trigger.${a.trigger_type}`)}
                          </Chip.Label>
                        </Chip>
                        <ArrowRight size={12} className="text-muted" />
                        {a.steps.map((s, i) => (
                          <Chip key={i} size="sm">
                            <Chip.Label>
                              {t(`action.${s.action_type}`)}
                            </Chip.Label>
                          </Chip>
                        ))}
                      </div>
                    </Table.Cell>
                    {!listId && (
                      <Table.Cell className="text-muted">
                        {scopeName(a)}
                      </Table.Cell>
                    )}
                    <Table.Cell>
                      <Switch
                        aria-label={t('enabled')}
                        isSelected={a.is_enabled}
                        onChange={async (v) => {
                          try {
                            await update.mutateAsync({
                              id: a.id,
                              data: { is_enabled: v },
                            });
                            await refresh();
                          } catch (e) {
                            fail(e);
                          }
                        }}
                      >
                        <Switch.Control>
                          <Switch.Thumb />
                        </Switch.Control>
                      </Switch>
                    </Table.Cell>
                    <Table.Cell>
                      <Dropdown>
                        <Dropdown.Trigger
                          className={buttonVariants({
                            variant: 'ghost',
                            isIconOnly: true,
                            size: 'sm',
                          })}
                          aria-label={t('manage')}
                          data-testid={`automation-menu-${a.name}`}
                        >
                          <MoreHorizontal size={16} />
                        </Dropdown.Trigger>
                        <Dropdown.Popover>
                          <Dropdown.Menu
                            aria-label={t('manage')}
                            onAction={(k) => menuAction(a, String(k))}
                          >
                            <Dropdown.Item id="edit" textValue={t('edit')}>
                              {t('edit')}
                            </Dropdown.Item>
                            <Dropdown.Item id="runs" textValue={t('runs')}>
                              {t('runs')}
                            </Dropdown.Item>
                            {a.trigger_type === 'webhook' ? (
                              <Dropdown.Item
                                id="webhook"
                                textValue={t('webhookUrl')}
                                data-testid={`webhook-url-${a.name}`}
                              >
                                {a.has_webhook
                                  ? t('rotateUrl')
                                  : t('createUrl')}
                              </Dropdown.Item>
                            ) : null}
                            {a.steps.some(
                              (st) => st.action_type === 'send_webhook',
                            ) ? (
                              <Dropdown.Item
                                id="secret"
                                textValue={t('signingSecret')}
                                data-testid={`signing-secret-${a.name}`}
                              >
                                {t('signingSecret')}
                              </Dropdown.Item>
                            ) : null}
                            <Dropdown.Item
                              id="delete"
                              textValue={t('delete')}
                              variant="danger"
                            >
                              {t('delete')}
                            </Dropdown.Item>
                          </Dropdown.Menu>
                        </Dropdown.Popover>
                      </Dropdown>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}
      {listId && workspaceWide > 0 && (
        <p className="text-xs text-muted" data-testid="workspace-wide-note">
          {t('workspaceWide', { count: workspaceWide })}
        </p>
      )}

      {runsAutomation && (
        <JiraDialog
          isOpen
          onClose={() => setRunsFor(null)}
          title={`${t('runs')}: ${runsAutomation.name}`}
          footer={
            <Button variant="tertiary" onPress={() => setRunsFor(null)}>
              {t('done')}
            </Button>
          }
        >
          <Runs id={runsAutomation.id} locale={locale} />
        </JiraDialog>
      )}

      {secret && (
        <FormDialog
          isOpen
          onClose={() => setSecret(null)}
          title={secret.title}
          submitLabel={t('done')}
          onSubmit={() => setSecret(null)}
        >
          <p className="text-sm">{secret.hint}</p>
          <div className="flex items-end gap-2">
            <ReadonlyField
              label={secret.title}
              value={secret.value}
              testId="secret-value"
            />
            <Button
              isIconOnly
              variant="tertiary"
              aria-label={t('copy')}
              onPress={() =>
                navigator.clipboard
                  ?.writeText(secret.value)
                  .then(() => toast.success(t('copied')))
              }
            >
              <Copy size={14} />
            </Button>
          </div>
        </FormDialog>
      )}
    </div>
  );
}
