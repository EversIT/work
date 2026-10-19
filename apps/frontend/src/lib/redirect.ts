/** Only same-site relative paths are accepted as post-login targets (no open redirects). */
export function safeNext(value: string | null | undefined): string {
  if (!value || !value.startsWith('/') || value.startsWith('//') || value.startsWith('/\\') || /[\u0000-\u001f]/.test(value)) return '/';
  return value;
}
