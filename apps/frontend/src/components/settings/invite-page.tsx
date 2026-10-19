'use client';

import { Alert, Button, Card } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useRouter, useSearchParams } from 'next/navigation';
import { useState } from 'react';
import { acceptInvite } from '../../api/generated/projects/projects';
import { errorKind } from '../../lib/api';

export function InvitePage() {
  const t = useTranslations('project');
  const params = useSearchParams();
  const router = useRouter();
  const qc = useQueryClient();
  const token = params.get('token');
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  return (
    <div className="mx-auto max-w-md p-4 pt-16">
      <Card>
        <Card.Header>
          <Card.Title>{t('joinTitle')}</Card.Title>
          <Card.Description>{t('joinText')}</Card.Description>
        </Card.Header>
        <Card.Content className="flex flex-col gap-3">
          {(!token || error) && <Alert status="danger"><Alert.Content><Alert.Description>{error ?? t('inviteInvalid')}</Alert.Description></Alert.Content></Alert>}
          <Button
            isDisabled={!token}
            isPending={pending}
            data-testid="accept-invite"
            onPress={async () => {
              setPending(true);
              setError(null);
              try {
                await acceptInvite({ token: token ?? '' });
                await qc.invalidateQueries();
                router.replace('/');
              } catch (e) {
                setError(errorKind(e) === 'notFound' ? t('inviteInvalid') : t('inviteFailed'));
              } finally {
                setPending(false);
              }
            }}
          >
            {t('acceptInvite')}
          </Button>
        </Card.Content>
      </Card>
    </div>
  );
}
