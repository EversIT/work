import { Suspense } from 'react';
import { SlackCallback } from '../../../../../components/settings/slack-callback';

export default function Page() {
  return (
    <Suspense>
      <SlackCallback />
    </Suspense>
  );
}
