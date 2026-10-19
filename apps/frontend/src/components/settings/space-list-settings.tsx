'use client';

import { Button, Card, Chip, Switch, toast, Label } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import { useCreateStage, useDeleteList, useDeleteStage, useGetList, useListListGrants, usePutListGrant, useDeleteListGrant, useReorderStages, useUpdateList, useUpdateStage } from '../../api/generated/lists/lists';
import { useDeleteSpace, useDeleteSpaceGrant, useGetWorkspaceTree, useListSpaceGrants, usePutSpaceGrant, useUpdateSpace } from '../../api/generated/spaces/spaces';
import type { SpaceTree } from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { fullName } from '../../lib/format';
import { useMembers, useWorkspace } from '../../lib/hooks';
import { SimpleSelect, TextInput } from '../ui/inputs';

export function SpaceListSettings({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations('settings');
  const tree = body(useGetWorkspaceTree(workspaceId).data);
  return (
    <div className="flex flex-col gap-4">
      <h2 className="text-lg font-medium">{t('spacesAndLists')}</h2>
      {tree?.spaces.length === 0 && <p className="text-muted">{t('noSpaces')}</p>}
      {tree?.spaces.map((s) => (
        <SpaceCard key={s.id} workspaceId={workspaceId} space={s} />
      ))}
    </div>
  );
}

type Space = SpaceTree;

function GrantEditor({ kind, id, projectId }: { kind: 'space' | 'list'; id: string; projectId?: string }) {
  const t = useTranslations('settings');
  const qc = useQueryClient();
  const members = useMembers(projectId);
  const spaceGrants = body(useListSpaceGrants(id, { query: { enabled: kind === 'space' } }).data) ?? [];
  const listGrants = body(useListListGrants(id, { query: { enabled: kind === 'list' } }).data) ?? [];
  const grants = kind === 'space' ? spaceGrants : listGrants;
  const putSpace = usePutSpaceGrant();
  const putList = usePutListGrant();
  const delSpace = useDeleteSpaceGrant();
  const delList = useDeleteListGrant();
  const [pick, setPick] = useState<string | null>(null);
  const [role, setRole] = useState<string | null>('member');
  const candidates = members.filter((m) => !grants.some((g) => g.user_id === m.user_id));
  const refresh = () => qc.invalidateQueries();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? errorKind(e));
  return (
    <div className="flex flex-col gap-2 rounded-lg bg-surface-secondary p-3" data-testid={`grants-${kind}`}>
      <p className="text-sm font-medium">{t('access')}</p>
      <ul className="text-sm">
        {grants.map((g) => {
          const m = members.find((x) => x.user_id === g.user_id);
          return (
            <li key={g.user_id} className="flex items-center justify-between py-1">
              <span>{m ? fullName(m) : g.user_id} · {t(`roles.${g.role}`)}</span>
              <Button isIconOnly size="sm" variant="ghost" aria-label={t('revoke')} onPress={async () => {
                try {
                  if (kind === 'space') await delSpace.mutateAsync({ id, userId: g.user_id });
                  else await delList.mutateAsync({ id, userId: g.user_id });
                  await refresh();
                } catch (e) { fail(e); }
              }}><Trash2 size={12} /></Button>
            </li>
          );
        })}
      </ul>
      {candidates.length > 0 && (
        <div className="flex items-end gap-2">
          <div className="flex-1"><SimpleSelect label={t('person')} value={pick} onChange={setPick} options={candidates.map((m) => ({ value: m.user_id, label: fullName(m) }))} /></div>
          <div className="w-32"><SimpleSelect label={t('role')} value={role} onChange={setRole} options={['viewer', 'member', 'admin'].map((r) => ({ value: r, label: t(`roles.${r}`) }))} /></div>
          <Button size="sm" isDisabled={!pick} onPress={async () => {
            if (!pick) return;
            try {
              if (kind === 'space') await putSpace.mutateAsync({ id, userId: pick, data: { role: role ?? 'member' } });
              else await putList.mutateAsync({ id, userId: pick, data: { role: role ?? 'member' } });
              setPick(null);
              await refresh();
            } catch (e) { fail(e); }
          }} data-testid={`grant-${kind}`}>{t('grant')}</Button>
        </div>
      )}
    </div>
  );
}

function SpaceCard({ workspaceId, space }: { workspaceId: string; space: Space }) {
  const t = useTranslations('settings');
  const qc = useQueryClient();
  const workspace = useWorkspace(workspaceId);
  const update = useUpdateSpace();
  const del = useDeleteSpace();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? errorKind(e));
  return (
    <Card>
      <Card.Header className="flex-row items-center justify-between">
        <Card.Title>{space.name}</Card.Title>
        <div className="flex items-center gap-3">
          <Switch className="flex flex-row items-center gap-2" isSelected={space.access_type === 'private'} onChange={async (v) => { try { await update.mutateAsync({ id: space.id, data: { access_type: v ? 'private' : 'public' } }); await qc.invalidateQueries(); } catch (e) { fail(e); } }} data-testid={`private-${space.name}`}>
            <Switch.Control><Switch.Thumb /></Switch.Control>
            <Switch.Content><Label>{t('private')}</Label></Switch.Content>
          </Switch>
          <Button isIconOnly size="sm" variant="ghost" aria-label={t('deleteSpace')} onPress={async () => { if (!window.confirm(t('confirmDeleteSpace'))) return; try { await del.mutateAsync({ id: space.id }); await qc.invalidateQueries(); } catch (e) { fail(e); } }}><Trash2 size={14} /></Button>
        </div>
      </Card.Header>
      <Card.Content className="flex flex-col gap-3">
        {space.access_type === 'private' && <GrantEditor kind="space" id={space.id} projectId={workspace?.project_id} />}
        {space.lists.map((l) => <ListSettings key={l.id} listId={l.id} projectId={workspace?.project_id} />)}
      </Card.Content>
    </Card>
  );
}

function ListSettings({ listId, projectId }: { listId: string; projectId?: string }) {
  const t = useTranslations('settings');
  const qc = useQueryClient();
  const list = body(useGetList(listId).data);
  const update = useUpdateList();
  const del = useDeleteList();
  const createStage = useCreateStage();
  const updateStage = useUpdateStage();
  const delStage = useDeleteStage();
  const reorder = useReorderStages();
  const [open, setOpen] = useState(false);
  const [newStage, setNewStage] = useState('');
  if (!list) return null;
  const refresh = () => qc.invalidateQueries();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? errorKind(e));
  const move = async (i: number, d: number) => {
    const ids = list.stages.map((s) => s.id);
    const [x] = ids.splice(i, 1);
    ids.splice(i + d, 0, x);
    try { await reorder.mutateAsync({ id: list.id, data: { ids } }); await refresh(); } catch (e) { fail(e); }
  };
  return (
    <div className="rounded-lg border border-separator p-3" data-testid={`list-settings-${list.name}`}>
      <div className="flex items-center justify-between">
        <button className="text-left font-medium" onClick={() => setOpen(!open)} aria-expanded={open}>{list.name}</button>
        <Chip size="sm"><Chip.Label>{list.access_type === 'private' ? t('private') : t('public')}</Chip.Label></Chip>
      </div>
      {open && (
        <div className="mt-3 flex flex-col gap-3">
          <Switch className="flex flex-row items-center gap-2" isSelected={list.access_type === 'private'} onChange={async (v) => { try { await update.mutateAsync({ id: list.id, data: { access_type: v ? 'private' : 'public' } }); await refresh(); } catch (e) { fail(e); } }}>
            <Switch.Control><Switch.Thumb /></Switch.Control>
            <Switch.Content><Label>{t('private')}</Label></Switch.Content>
          </Switch>
          {list.access_type === 'private' && <GrantEditor kind="list" id={list.id} projectId={projectId} />}
          <p className="text-sm font-medium">{t('stages')}</p>
          <ul className="flex flex-col gap-1">
            {list.stages.map((s, i) => (
              <li key={s.id} className="flex items-center gap-2">
                <input aria-label={t('stageName')} defaultValue={s.name} maxLength={100} className="h-8 flex-1 rounded-lg border border-field-border bg-field-background px-2 text-sm"
                  onBlur={async (e) => { const v = e.currentTarget.value.trim(); if (v && v !== s.name) { try { await updateStage.mutateAsync({ id: s.id, data: { name: v } }); await refresh(); } catch (err) { fail(err); } } }} />
                <Switch className="flex flex-row items-center gap-2" isSelected={s.is_completed} onChange={async (v) => { try { await updateStage.mutateAsync({ id: s.id, data: { is_completed: v } }); await refresh(); } catch (err) { fail(err); } }}>
                  <Switch.Control><Switch.Thumb /></Switch.Control>
                  <Switch.Content><Label>{t('completes')}</Label></Switch.Content>
                </Switch>
                <Button isIconOnly size="sm" variant="ghost" aria-label={t('moveUp')} isDisabled={i === 0} onPress={() => move(i, -1)}><ArrowUp size={12} /></Button>
                <Button isIconOnly size="sm" variant="ghost" aria-label={t('moveDown')} isDisabled={i === list.stages.length - 1} onPress={() => move(i, 1)}><ArrowDown size={12} /></Button>
                <Button isIconOnly size="sm" variant="ghost" aria-label={t('deleteStage')} onPress={async () => { try { await delStage.mutateAsync({ id: s.id }); await refresh(); } catch (err) { toast.danger(errorKind(err) === 'conflict' ? t('stageInUse') : serverMessage(err) ?? errorKind(err)); } }}><Trash2 size={12} /></Button>
              </li>
            ))}
          </ul>
          <form className="flex items-end gap-2" onSubmit={async (e) => { e.preventDefault(); if (!newStage.trim()) return; try { await createStage.mutateAsync({ id: list.id, data: { name: newStage.trim() } }); setNewStage(''); await refresh(); } catch (err) { fail(err); } }}>
            <div className="flex-1"><TextInput label={t('newStage')} value={newStage} onChange={setNewStage} maxLength={100} /></div>
            <Button type="submit" size="sm" variant="secondary"><Plus size={14} />{t('add')}</Button>
          </form>
          <Button variant="danger-soft" size="sm" className="self-start" onPress={async () => { if (!window.confirm(t('confirmDeleteList'))) return; try { await del.mutateAsync({ id: list.id }); await refresh(); } catch (e) { fail(e); } }}>{t('deleteList')}</Button>
        </div>
      )}
    </div>
  );
}

