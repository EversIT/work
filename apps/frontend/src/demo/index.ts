/**
 * Demo mode (`DEMO_MODE=true` in the server's environment, read at runtime): the app runs without a
 * backend. Every API call is answered in the browser from made-up data (see `server.ts`), nothing
 * is sent anywhere and nothing is stored: a reload starts over.
 *
 * The server reads the variable; the root layout hands the answer to the browser as
 * `window.__DEMO__` (see `components/demo-flag.tsx`), so one image serves both the real app and the demo.
 */
export function isDemo(): boolean {
  if (typeof window === 'undefined') return process.env.DEMO_MODE === 'true';
  return (window as { __DEMO__?: boolean }).__DEMO__ === true;
}
