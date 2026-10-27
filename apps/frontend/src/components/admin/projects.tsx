'use client';

import { Table } from '@heroui/react';
import { useLocale, useTranslations } from 'next-intl';
import { useAdminListProjects } from '../../api/generated/admin/admin';
import { body } from '../../lib/api';
import { formatDate } from '../../lib/format';

const size = (n: number) =>
  n < 1024 ** 2
    ? `${(n / 1024).toFixed(0)} KB`
    : `${(n / 1024 ** 2).toFixed(1)} MB`;

export function Projects() {
  const t = useTranslations('admin');
  const locale = useLocale();
  const projects = body(useAdminListProjects().data) ?? [];
  return (
    <Table>
      <Table.ScrollContainer>
        <Table.Content
          aria-label={t('tab.projects')}
          data-testid="admin-projects"
        >
          <Table.Header>
            <Table.Column isRowHeader id="name">
              {t('projects.name')}
            </Table.Column>
            <Table.Column id="owner">{t('projects.owner')}</Table.Column>
            <Table.Column id="members">{t('projects.members')}</Table.Column>
            <Table.Column id="workspaces">
              {t('projects.workspaces')}
            </Table.Column>
            <Table.Column id="cards">{t('projects.cards')}</Table.Column>
            <Table.Column id="files">{t('projects.files')}</Table.Column>
            <Table.Column id="created">{t('projects.created')}</Table.Column>
          </Table.Header>
          <Table.Body
            items={projects}
            renderEmptyState={() => (
              <div className="p-6 text-center text-muted">
                {t('projects.none')}
              </div>
            )}
          >
            {(p) => (
              <Table.Row id={p.id}>
                <Table.Cell className="font-medium">{p.name}</Table.Cell>
                <Table.Cell>{p.owner_email ?? '—'}</Table.Cell>
                <Table.Cell>{p.members}</Table.Cell>
                <Table.Cell>{p.workspaces}</Table.Cell>
                <Table.Cell>{p.cards}</Table.Cell>
                <Table.Cell>{size(p.file_bytes)}</Table.Cell>
                <Table.Cell>{formatDate(p.created_at, locale)}</Table.Cell>
              </Table.Row>
            )}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}
