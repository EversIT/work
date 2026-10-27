import { defineConfig } from '@playwright/test';

// The tests expect a running stack (frontend + backend + database). Point them at it with
// E2E_BASE_URL. Locally you can use your installed Chrome: PW_CHANNEL=chrome. CI uses the
// bundled Chromium (`npx playwright install chromium`).
export default defineConfig({
  testDir: './src',
  timeout: 60_000,
  expect: { timeout: 10_000 },
  // Files run side by side in CI (E2E_WORKERS overrides it); the tests inside one file stay in order.
  fullyParallel: false,
  workers: Number(process.env.E2E_WORKERS ?? (process.env.CI ? 3 : 1)),
  retries: process.env.CI ? 1 : 0,
  reporter: [['list']],
  use: {
    baseURL: process.env.E2E_BASE_URL ?? 'http://localhost:3077',
    channel: process.env.PW_CHANNEL || undefined,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  outputDir: '../../dist/e2e-results',
  projects: [
    // The first account of a fresh instance is its administrator and admin.spec.ts relies on being
    // that account, so it runs alone before everything else.
    { name: 'admin', testMatch: 'admin.spec.ts' },
    { name: 'rest', testIgnore: 'admin.spec.ts', dependencies: ['admin'] },
  ],
});
