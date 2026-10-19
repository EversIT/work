import { NextIntlClientProvider } from 'next-intl';
import { getLocale } from 'next-intl/server';
import './global.css';
import { Providers } from './providers';

export const metadata = {
  title: 'work',
  description: 'Open-source work management',
};

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const locale = await getLocale();
  return (
    <html lang={locale} suppressHydrationWarning>
      <body>
        <NextIntlClientProvider locale={locale}>
          <Providers>{children}</Providers>
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
