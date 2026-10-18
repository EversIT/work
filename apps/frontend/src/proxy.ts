import { NextResponse } from 'next/server';
import type { NextRequest } from 'next/server';
import createIntlMiddleware from 'next-intl/middleware';
import { routing } from './i18n/routing';

const BACKEND_URL = process.env.BACKEND_URL || 'http://localhost:3001';

const handleI18nRouting = createIntlMiddleware(routing);

export default function proxy(request: NextRequest) {
  const { pathname, search } = request.nextUrl;

  if (pathname.startsWith('/api/v1/')) {
    const target = new URL(pathname + search, BACKEND_URL);
    return NextResponse.rewrite(target);
  }

  return handleI18nRouting(request);
}

export const config = {
  matcher: [
    '/api/v1/:path*',
    '/((?!api|trpc|_next|_vercel|.*\\..*).*)',
  ],
};