'use client';

import { useLocale, useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { SimpleSelect } from '../ui/inputs';

export const LOCALES = ['en', 'de'] as const;

/** Stores the choice in the NEXT_LOCALE cookie (not in the URL) and re-renders in the new language. */
export function LanguageSwitcher() {
  const t = useTranslations('settings');
  const locale = useLocale();
  const router = useRouter();
  return (
    <SimpleSelect
      label={t('language')}
      value={locale}
      testId="language-select"
      options={[
        { value: 'en', label: 'English' },
        { value: 'de', label: 'Deutsch' },
      ]}
      onChange={(v) => {
        if (!v || !(LOCALES as readonly string[]).includes(v)) return;
        const secure = window.location.protocol === 'https:' ? '; Secure' : '';
        document.cookie = `NEXT_LOCALE=${v}; Path=/; Max-Age=31536000; SameSite=Lax${secure}`;
        router.refresh();
      }}
    />
  );
}
