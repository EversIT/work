'use client';

import { useTranslations } from 'next-intl';
import { LanguageSwitcher } from '../shell/language-switcher';
import { ThemeSwitcher } from '../shell/theme-switcher';
import { Panel, SettingsRow } from '../ui/settings';

/** Language and theme. Both are kept in this browser, not on the server. */
export function AccountAppearance() {
  const t = useTranslations('settings');
  return (
    <Panel>
      <SettingsRow label={t('language')} help={t('languageHelp')}>
        <LanguageSwitcher />
      </SettingsRow>
      <SettingsRow label={t('appearance')} help={t('appearanceHelp')}>
        <ThemeSwitcher />
      </SettingsRow>
    </Panel>
  );
}
