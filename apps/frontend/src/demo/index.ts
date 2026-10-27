/**
 * Demo mode (`NEXT_PUBLIC_DEMO=true` at build time): the app runs without a backend. Every API call
 * is answered in the browser from made-up data (see `server.ts`), nothing is sent anywhere and
 * nothing is stored: a reload starts over.
 */
export const DEMO = process.env.NEXT_PUBLIC_DEMO === 'true';
