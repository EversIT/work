'use client';

import { Table } from '@heroui/react';
import { useLocale, useTranslations } from 'next-intl';
import { useAdminAuditLog } from '../../api/generated/admin/admin';
import { body } from '../../lib/api';
import { formatDate } from '../../lib/format';

/** What administrators changed, newest first. Values are shown as text, never interpreted. */
export function AuditLog() {
  const t = useTranslations('admin');
  const locale = useLocale();
  const entries = body(useAdminAuditLog().data) ?? [];
  return (
    <Table>
      <Table.ScrollContainer>
        <Table.Content aria-label={t('tab.audit')} data-testid="admin-audit">
          <Table.Header>
            <Table.Column isRowHeader id="at">
              {t('audit.when')}
            </Table.Column>
            <Table.Column id="who">{t('audit.who')}</Table.Column>
            <Table.Column id="action">{t('audit.action')}</Table.Column>
            <Table.Column id="detail">{t('audit.detail')}</Table.Column>
          </Table.Header>
          <Table.Body
            items={entries}
            renderEmptyState={() => (
              <div className="p-6 text-center text-muted">
                {t('audit.none')}
              </div>
            )}
          >
            {(e) => (
              <Table.Row id={String(e.id)}>
                <Table.Cell>{formatDate(e.at, locale, true)}</Table.Cell>
                <Table.Cell>{e.actor_email ?? '—'}</Table.Cell>
                <Table.Cell>
                  <code>{e.action}</code>
                </Table.Cell>
                <Table.Cell>
                  <code className="break-all text-xs">
                    {e.target ? `${e.target} ` : ''}
                    {JSON.stringify(e.detail)}
                  </code>
                </Table.Cell>
              </Table.Row>
            )}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}
