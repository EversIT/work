'use client';

import { SearchBox } from '../ui/inputs';
import {
  Button,
  Chip,
  Dropdown,
  Table,
  buttonVariants,
  toast,
} from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { MoreHorizontal } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useState } from 'react';
import {
  adminUpdateUser,
  useAdminListUsers,
} from '../../api/generated/admin/admin';
import { body, errorKind, serverMessage } from '../../lib/api';
import { formatDate } from '../../lib/format';

const PAGE = 50;

export function Users({ meId }: { meId: string }) {
  const t = useTranslations('admin');
  const te = useTranslations('errors');
  const locale = useLocale();
  const qc = useQueryClient();
  const [query, setQuery] = useState('');
  const [page, setPage] = useState(0);
  const data = body(
    useAdminListUsers({
      query: query || undefined,
      limit: PAGE,
      offset: page * PAGE,
    }).data,
  );

  const act = async (
    id: string,
    patch: {
      is_instance_admin?: boolean;
      disabled?: boolean;
      unlock?: boolean;
    },
  ) => {
    try {
      await adminUpdateUser(id, patch);
      await qc.invalidateQueries();
    } catch (e) {
      toast.danger(
        errorKind(e) === 'conflict'
          ? t('users.guard')
          : (serverMessage(e) ?? te(errorKind(e))),
      );
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <SearchBox
        label={t('users.search')}
        className="w-64"
        value={query}
        onChange={(v) => {
          setQuery(v);
          setPage(0);
        }}
        testId="admin-user-search"
      />
      <Table>
        <Table.ScrollContainer>
          <Table.Content aria-label={t('tab.users')} data-testid="admin-users">
            <Table.Header>
              <Table.Column isRowHeader id="name">
                {t('users.name')}
              </Table.Column>
              <Table.Column id="signin">{t('users.signIn')}</Table.Column>
              <Table.Column id="status">{t('users.status')}</Table.Column>
              <Table.Column id="projects">{t('users.projects')}</Table.Column>
              <Table.Column id="joined">{t('users.joined')}</Table.Column>
              <Table.Column id="actions">
                <span className="sr-only">{t('users.actions')}</span>
              </Table.Column>
            </Table.Header>
            <Table.Body
              items={data?.users ?? []}
              renderEmptyState={() => (
                <div className="p-6 text-center text-muted">
                  {t('users.none')}
                </div>
              )}
            >
              {(u) => (
                <Table.Row id={u.id}>
                  <Table.Cell>
                    <span className="block font-medium">
                      {u.first_name} {u.last_name}
                    </span>
                    <span className="block text-xs text-muted">{u.email}</span>
                  </Table.Cell>
                  <Table.Cell>
                    <span className="flex flex-wrap gap-1">
                      {u.has_password && (
                        <Chip size="sm">
                          <Chip.Label>{t('users.password')}</Chip.Label>
                        </Chip>
                      )}
                      {u.mfa_enabled && (
                        <Chip size="sm" color="success">
                          <Chip.Label>MFA</Chip.Label>
                        </Chip>
                      )}
                      {u.passkeys > 0 && (
                        <Chip size="sm" color="success">
                          <Chip.Label>
                            {t('users.passkeys', { count: u.passkeys })}
                          </Chip.Label>
                        </Chip>
                      )}
                      {u.sso.map((p) => (
                        <Chip key={p} size="sm">
                          <Chip.Label>{p}</Chip.Label>
                        </Chip>
                      ))}
                    </span>
                  </Table.Cell>
                  <Table.Cell>
                    <span className="flex flex-wrap gap-1">
                      {u.is_instance_admin && (
                        <Chip size="sm" color="accent">
                          <Chip.Label>{t('users.admin')}</Chip.Label>
                        </Chip>
                      )}
                      {u.disabled && (
                        <Chip size="sm" color="danger">
                          <Chip.Label>{t('users.disabled')}</Chip.Label>
                        </Chip>
                      )}
                      {u.locked && (
                        <Chip size="sm" color="warning">
                          <Chip.Label>{t('users.locked')}</Chip.Label>
                        </Chip>
                      )}
                    </span>
                  </Table.Cell>
                  <Table.Cell>{u.projects}</Table.Cell>
                  <Table.Cell>{formatDate(u.created_at, locale)}</Table.Cell>
                  <Table.Cell>
                    <Dropdown>
                      <Dropdown.Trigger
                        className={buttonVariants({
                          variant: 'ghost',
                          isIconOnly: true,
                          size: 'sm',
                        })}
                        aria-label={t('users.actions')}
                        data-testid={`user-actions-${u.email}`}
                      >
                        <MoreHorizontal size={14} />
                      </Dropdown.Trigger>
                      <Dropdown.Popover>
                        <Dropdown.Menu
                          aria-label={t('users.actions')}
                          onAction={(key) => {
                            if (key === 'admin')
                              void act(u.id, {
                                is_instance_admin: !u.is_instance_admin,
                              });
                            if (key === 'disable')
                              void act(u.id, { disabled: !u.disabled });
                            if (key === 'unlock')
                              void act(u.id, { unlock: true });
                          }}
                        >
                          <Dropdown.Item
                            id="admin"
                            textValue={t('users.toggleAdmin')}
                            isDisabled={u.id === meId}
                          >
                            {u.is_instance_admin
                              ? t('users.removeAdmin')
                              : t('users.makeAdmin')}
                          </Dropdown.Item>
                          <Dropdown.Item
                            id="disable"
                            textValue={t('users.toggleDisabled')}
                            isDisabled={u.id === meId}
                            variant={u.disabled ? undefined : 'danger'}
                          >
                            {u.disabled
                              ? t('users.enable')
                              : t('users.disable')}
                          </Dropdown.Item>
                          {u.locked ? (
                            <Dropdown.Item
                              id="unlock"
                              textValue={t('users.unlock')}
                            >
                              {t('users.unlock')}
                            </Dropdown.Item>
                          ) : null}
                        </Dropdown.Menu>
                      </Dropdown.Popover>
                    </Dropdown>
                  </Table.Cell>
                </Table.Row>
              )}
            </Table.Body>
          </Table.Content>
        </Table.ScrollContainer>
      </Table>
      {data && data.total > PAGE && (
        <div className="flex items-center justify-center gap-3 text-sm">
          <Button
            size="sm"
            variant="tertiary"
            isDisabled={page === 0}
            onPress={() => setPage(page - 1)}
          >
            {t('previous')}
          </Button>
          <span>
            {t('pageOf', {
              page: page + 1,
              pages: Math.ceil(data.total / PAGE),
            })}
          </span>
          <Button
            size="sm"
            variant="tertiary"
            isDisabled={(page + 1) * PAGE >= data.total}
            onPress={() => setPage(page + 1)}
          >
            {t('next')}
          </Button>
        </div>
      )}
    </div>
  );
}
