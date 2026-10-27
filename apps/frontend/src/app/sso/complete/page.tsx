import { Suspense } from 'react';
import { SsoComplete } from '../../../components/auth-pages';

export default function Page() {
  return (
    <Suspense>
      <SsoComplete />
    </Suspense>
  );
}
