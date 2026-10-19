import { Suspense } from 'react';
import { ResetPassword } from '../../components/auth-pages';

export default function Page() {
  return (
    <Suspense>
      <ResetPassword />
    </Suspense>
  );
}
