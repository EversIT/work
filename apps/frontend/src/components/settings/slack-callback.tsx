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
  const [error, setError] = useState(false);
  const done = useRef(false);

  useEffect(() => {
    if (done.current) return;
    done.current = true;
    const code = params.get('code');
    const state = params.get('state');
    if (!code || !state) {
      setError(true);
      return;
    }
    slackCallback({ code, state })
      .then(() => router.replace('/settings'))
      .catch(() => setError(true));
  }, [params, router]);

  return (
    <div className="flex justify-center p-12">
      {error ? (
        <Alert status="danger"><Alert.Content><Alert.Description>{t('slackFailed')}</Alert.Description></Alert.Content></Alert>
      ) : (
        <Spinner aria-label={t('connectingSlack')} />
      )}
    </div>
  );
}
