import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // jsdom gives the few browser APIs the helpers touch (localStorage); nothing renders here.
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
  },
});
