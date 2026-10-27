import { isDemo } from '../demo';

/** Sets `window.__DEMO__` before any app script runs. Renders nothing outside demo mode. */
export function DemoFlag({ nonce }: { nonce?: string }) {
  if (!isDemo()) return null;
  return (
    <script
      nonce={nonce}
      dangerouslySetInnerHTML={{ __html: 'window.__DEMO__=true' }}
    />
  );
}
