/** A short "Browser · OS" label from a User-Agent string; falls back to the raw value. */
export function deviceLabel(userAgent: string | null | undefined): string {
  if (!userAgent) return '';
  const browser = /Edg\//.test(userAgent)
    ? 'Edge'
    : /OPR\/|Opera/.test(userAgent)
      ? 'Opera'
      : /Firefox\//.test(userAgent)
        ? 'Firefox'
        : /Chrome\//.test(userAgent)
          ? 'Chrome'
          : /Safari\//.test(userAgent)
            ? 'Safari'
            : null;
  const os = /Windows/.test(userAgent)
    ? 'Windows'
    : /iPhone|iPad|iOS/.test(userAgent)
      ? 'iOS'
      : /Android/.test(userAgent)
        ? 'Android'
        : /Mac OS X|Macintosh/.test(userAgent)
          ? 'macOS'
          : /Linux/.test(userAgent)
            ? 'Linux'
            : null;
  if (!browser && !os) return userAgent.slice(0, 60);
  return [browser, os].filter(Boolean).join(' · ');
}
