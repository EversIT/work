'use client';

import { useTranslations } from 'next-intl';
import { useTheme } from 'next-themes';
import { useSyncExternalStore } from 'react';
import { SimpleSelect } from '../ui/inputs';

const THEMES = ['system', 'light', 'dark'] as const;

/** Light, dark or follow the device. The choice is kept in this browser (next-themes). */
export function ThemeSwitcher() {
  const t = useTranslations('settings');
  const { theme, setTheme } = useTheme();
  // The stored theme is only known in the browser; until then the select shows the default.
  const mounted = useSyncExternalStore(
    () => () => undefined,
    () => true,
    () => false,
  );
  return (
    <SimpleSelect
      label={t('theme')}
      value={mounted ? (theme ?? 'system') : 'system'}
      testId="theme-select"
      options={THEMES.map((v) => ({ value: v, label: t(`theme_${v}`) }))}
      onChange={(v) => {
        if (v && (THEMES as readonly string[]).includes(v)) setTheme(v);
      }}
    />
  );
}
