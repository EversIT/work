export function fullName(u: { first_name: string; last_name: string }) {
  return `${u.first_name} ${u.last_name}`.trim();
}

export function initials(u: { first_name: string; last_name: string }) {
  return `${u.first_name.charAt(0)}${u.last_name.charAt(0)}`.toUpperCase();
}

export function formatDate(
  value: unknown,
  locale: string,
  withTime = false,
): string {
  if (typeof value !== 'string' || !value) return '';
  const d = new Date(value.length === 10 ? `${value}T00:00:00` : value);
  if (Number.isNaN(d.getTime())) return '';
  return new Intl.DateTimeFormat(
    locale,
    withTime
      ? { dateStyle: 'medium', timeStyle: 'short' }
      : { dateStyle: 'medium' },
  ).format(d);
}

export function relativeTime(iso: string, locale: string): string {
  const diff = (new Date(iso).getTime() - Date.now()) / 1000;
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ['year', 31536000],
    ['month', 2592000],
    ['day', 86400],
    ['hour', 3600],
    ['minute', 60],
  ];
  for (const [unit, secs] of units) {
    if (Math.abs(diff) >= secs)
      return rtf.format(Math.round(diff / secs), unit);
  }
  return rtf.format(Math.round(diff), 'second');
}

/** yyyy-mm-dd for date inputs, in local time. */
export function toDateInput(value: unknown): string {
  if (typeof value !== 'string' || !value) return '';
  return value.slice(0, 10);
}

export function toDateTimeInput(value: unknown): string {
  if (typeof value !== 'string' || !value) return '';
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return '';
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}
