import { NextResponse } from 'next/server';
import type { NextRequest } from 'next/server';
import { buildCsp, newNonce } from './lib/csp';

const BACKEND_URL = process.env.BACKEND_URL || 'http://localhost:3001';
const isDev = process.env.NODE_ENV !== 'production';

export default function proxy(request: NextRequest) {
  const { pathname, search } = request.nextUrl;

  // The demo has no backend: its API calls never leave the browser.
  if (pathname.startsWith('/api/v1/') && process.env.DEMO_MODE !== 'true') {
    const target = new URL(pathname + search, BACKEND_URL);
    return NextResponse.rewrite(target);
  }

  // Every page response gets its own nonce. It travels to the renderer in the request headers
  // (that is where Next looks for it) and to the browser in the response header.
  const nonce = newNonce();
  const csp = buildCsp(nonce, isDev);
  const headers = new Headers(request.headers);
  headers.set('x-nonce', nonce);
  headers.set('Content-Security-Policy', csp);
  const response = NextResponse.next({ request: { headers } });
  response.headers.set('Content-Security-Policy', csp);
  return response;
}

export const config = {
  matcher: [
    '/api/v1/:path*',
    {
      source:
        '/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp|ico)$).*)',
      missing: [
        { type: 'header', key: 'next-router-prefetch' },
        { type: 'header', key: 'purpose', value: 'prefetch' },
      ],
    },
  ],
};
