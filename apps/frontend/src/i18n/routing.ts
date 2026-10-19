/**
 * The language is never part of the URL and there is no `[locale]` route segment. It is stored in
 * the `NEXT_LOCALE` cookie (set by the language switcher, otherwise taken from Accept-Language).
 */
export const LOCALES = ['en', 'de'] as const;
export type Locale = (typeof LOCALES)[number];
export const DEFAULT_LOCALE: Locale = 'en';
export const LOCALE_COOKIE = 'NEXT_LOCALE';

export function isLocale(value: string | undefined | null): value is Locale {
  return !!value && (LOCALES as readonly string[]).includes(value);
}
