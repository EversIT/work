import { Suspense } from 'react';
import { VerifyEmail } from '../../components/auth-pages';

export default function Page() {
  return (
    <Suspense>
      <VerifyEmail />
    </Suspense>
  );
}
