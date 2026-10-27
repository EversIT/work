import type { NextConfig } from 'next';
import createNextIntlPlugin from 'next-intl/plugin';
const withNextIntl = createNextIntlPlugin();

// Optional path the app is served under; empty by default. It is fixed when the app is built.
const basePath = process.env.NEXT_BASE_PATH || '';

const nextConfig: NextConfig = {
  poweredByHeader: false,
  basePath: basePath || undefined,
  env: { NEXT_PUBLIC_BASE_PATH: basePath },
  // Next generates an AGENTS.md next to the app on `next dev`; this repo manages its own agent docs.
  agentRules: false,
  distDir: process.env.NEXT_DIST_DIR ?? '../../dist/apps/frontend',
  output: 'standalone',
  cacheMaxMemorySize: 12 * 1024 * 1024,
  async headers() {
    return [
      {
        // The Content-Security-Policy is set per request in `src/proxy.ts` (it carries a nonce).
        source: '/:path*',
        headers: [
          { key: 'X-Frame-Options', value: 'DENY' },
          { key: 'X-Content-Type-Options', value: 'nosniff' },
          { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
          {
            key: 'Permissions-Policy',
            value: 'camera=(), microphone=(), geolocation=()',
          },
          {
            key: 'Strict-Transport-Security',
            value: 'max-age=63072000; includeSubDomains',
          },
        ],
      },
    ];
  },
};

export default withNextIntl(nextConfig);
