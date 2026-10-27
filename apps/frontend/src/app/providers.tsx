'use client';

import { Toast } from '@heroui/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider } from 'next-themes';
import { ApiError } from '../api/fetcher';
import { useState } from 'react';

export function Providers({
  children,
  nonce,
}: {
  children: React.ReactNode;
  nonce?: string;
}) {
  const [client] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            // Do not hammer the API on auth/permission errors.
            retry: (count, error) =>
              !(
                error instanceof ApiError &&
                error.status >= 400 &&
                error.status < 500
              ) && count < 2,
            refetchOnWindowFocus: false,
          },
        },
      }),
  );
  return (
    <ThemeProvider
      attribute={['class', 'data-theme']}
      defaultTheme="system"
      enableSystem
      disableTransitionOnChange
      nonce={nonce}
    >
      <QueryClientProvider client={client}>
        {children}
        <Toast.Provider />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
