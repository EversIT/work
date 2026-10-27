'use client';

import { Alert, Spinner } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useRouter, useSearchParams } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';
import { slackCallback } from '../../api/generated/integrations/integrations';

/** Redeems the one-time OAuth state with the signed-in session, then returns to the settings. */
export function SlackCallback() {
  const t = useTranslations('settings');
  const params = useSearchParams();
  const router = useRouter();
  const [failed, setFailed] = useState(false);
  const done = useRef(false);

  const code = params.get('code');
  const state = params.get('state');
  // A missing code or state is an error by itself; no effect needs to say so.
  const error = failed || !code || !state;

  useEffect(() => {
    if (done.current || !code || !state) return;
    done.current = true;
    slackCallback({ code, state })
      .then(() => router.replace('/settings'))
      .catch(() => setFailed(true));
  }, [code, state, router]);

  return (
    <div className="flex justify-center p-12">
      {error ? (
        <Alert status="danger">
          <Alert.Content>
            <Alert.Description>{t('slackFailed')}</Alert.Description>
          </Alert.Content>
        </Alert>
      ) : (
        <Spinner aria-label={t('connectingSlack')} />
      )}
    </div>
  );
}
