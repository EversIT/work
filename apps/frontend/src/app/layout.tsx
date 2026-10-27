import type { Metadata } from 'next';
import { NextIntlClientProvider } from 'next-intl';
import { connection } from 'next/server';
import { getLocale } from 'next-intl/server';
import './global.css';
import { BrandingProvider } from '../components/brand';
import { headers } from 'next/headers';
import { DEFAULT_BRANDING, getBranding } from '../lib/branding';
import { accentVars } from '../lib/theme';
import { DemoFlag } from '../components/demo-flag';
import { Providers } from './providers';

/** Title, description and favicon are the administrator's (Admin > Appearance). */
export async function generateMetadata(): Promise<Metadata> {
  const b = await getBranding();
  return {
    title: b.title,
    description: b.description,
    icons: {
      icon: b.has_favicon
        ? `/api/v1/branding/favicon?v=${b.version}`
        : '/icon.svg',
    },
  };
}

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // Pages are rendered per request: the CSP nonce differs every time, so nothing may be prerendered.
  await connection();
  const locale = await getLocale();
  const branding = await getBranding();
  const nonce = (await headers()).get('x-nonce') ?? undefined;
  return (
    <html
      lang={locale}
      suppressHydrationWarning
      style={
        branding.accent.toLowerCase() === DEFAULT_BRANDING.accent.toLowerCase()
          ? undefined
          : (accentVars(branding.accent) ?? undefined)
      }
    >
      <body>
        <DemoFlag nonce={nonce} />
        <NextIntlClientProvider locale={locale}>
          <BrandingProvider branding={branding}>
            <Providers nonce={nonce}>{children}</Providers>
          </BrandingProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
