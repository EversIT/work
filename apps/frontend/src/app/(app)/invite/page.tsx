import { Suspense } from 'react';
import { InvitePage } from '../../../components/settings/invite-page';

export default function Page() {
  return (
    <Suspense>
      <InvitePage />
    </Suspense>
  );
}
