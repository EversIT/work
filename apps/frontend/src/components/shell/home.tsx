'use client';

import { routes } from '../../lib/routes';
import { Button, Card, Spinner } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useEffect, useState } from 'react';
import { useCreateProject } from '../../api/generated/projects/projects';
import { useCreateWorkspace } from '../../api/generated/workspaces/workspaces';
import { errorKind, serverMessage } from '../../lib/api';
import { useAllWorkspaces } from '../../lib/hooks';
import { TextInput } from '../ui/inputs';

/** Sends the user to their (last used) workspace, or helps create the first one. */
export function Home() {
  const t = useTranslations('home');
  const te = useTranslations('errors');
  const router = useRouter();
  const qc = useQueryClient();
  const { workspaces, projects, loading } = useAllWorkspaces();
  const createWorkspace = useCreateWorkspace();
  const createProject = useCreateProject();
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (loading || workspaces.length === 0) return;
    let last: string | null = null;
    try {
      last = localStorage.getItem('work.lastWorkspace');
    } catch {
      /* storage may be unavailable */
    }
    const target = workspaces.find((w) => w.id === last) ?? workspaces[0];
    router.replace(routes.workspace(target.key_prefix));
  }, [loading, workspaces, router]);

  if (loading || workspaces.length > 0) {
    return (
      <div className="flex min-h-[60vh] items-center justify-center">
        <Spinner aria-label={t('loading')} />
      </div>
    );
  }

  return (
    <div className="mx-auto flex max-w-md flex-col gap-4 px-4 py-16">
      <Card>
        <Card.Header>
          <Card.Title>{t('welcome')}</Card.Title>
          <Card.Description>{t('createFirst')}</Card.Description>
        </Card.Header>
        <Card.Content>
          <form
            className="flex flex-col gap-4"
            onSubmit={async (e) => {
              e.preventDefault();
              setError(null);
              const name = String(new FormData(e.currentTarget).get('name'));
              try {
                const projectId =
                  projects[0]?.id ??
                  (await createProject
                    .mutateAsync({ data: { name: 'My team' } })
                    .then((r) => (r.status === 200 ? r.data.id : undefined)));
                if (!projectId) return;
                const ws = await createWorkspace.mutateAsync({
                  id: projectId,
                  data: { name },
                });
                await qc.invalidateQueries();
                if (ws.status === 200)
                  router.replace(routes.workspace(ws.data.key_prefix));
              } catch (err) {
                setError(serverMessage(err) ?? te(errorKind(err)));
              }
            }}
          >
            <TextInput
              label={t('workspaceName')}
              name="name"
              required
              maxLength={100}
            />
            {error && <p className="text-sm text-danger">{error}</p>}
            <Button type="submit" isPending={createWorkspace.isPending}>
              {t('createWorkspace')}
            </Button>
          </form>
        </Card.Content>
      </Card>
    </div>
  );
}
