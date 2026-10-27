/**
 * Content-Security-Policy with a per-request nonce. Next adds the nonce to its own inline
 * bootstrap scripts when it finds it in the request's CSP header (see `proxy.ts`), so scripts need
 * neither `'unsafe-inline'` nor a hash list; `'strict-dynamic'` lets those trusted scripts load
 * their chunks.
 */
export function buildCsp(nonce: string, isDev: boolean): string {
  return [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${isDev ? " 'unsafe-eval'" : ''}`,
    // Styles stay inline: the UI kit sets style attributes, which cannot carry a nonce. Inline
    // styles cannot run script.
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob:",
    "font-src 'self'",
    "connect-src 'self'",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
  ].join('; ');
}

/** 128 random bits, base64. */
export function newNonce(): string {
  return btoa(
    String.fromCharCode(...crypto.getRandomValues(new Uint8Array(16))),
  );
}
