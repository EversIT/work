import type { NextConfig } from 'next';
import createNextIntlPlugin from 'next-intl/plugin';
const withNextIntl = createNextIntlPlugin();

// Set for the demo, which is served below another site (`/demo` under the documentation); empty
// everywhere else. It is fixed when the app is built.
const basePath = process.env.NEXT_BASE_PATH || '';

const nextConfig: NextConfig = {
  poweredByHeader: false,
  basePath: basePath || undefined,
  env: { NEXT_PUBLIC_BASE_PATH: basePath },
  // Next generates an AGENTS.md next to the app on `next dev`; this repo manages its own agent docs.
  agentRules: false,
  // The demo build (see apps/demo) writes next to the real one instead of over it.
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
