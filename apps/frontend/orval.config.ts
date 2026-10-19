import { defineConfig } from 'orval';

// The backend serves its OpenAPI document (`GET /api/v1/openapi.json`) and can also print it
// without a database (`work-backend openapi`, the `backend:openapi` Nx target). The
// client is generated from that file, so request and response types never drift.
export default defineConfig({
  api: {
    input: { target: '../backend/openapi.json' },
    output: {
      mode: 'tags-split',
      target: 'src/api/generated/endpoints.ts',
      schemas: 'src/api/generated/model',
      client: 'react-query',
      httpClient: 'fetch',
      baseUrl: '/api/v1',
      clean: true,
      override: {
        mutator: { path: 'src/api/fetcher.ts', name: 'apiFetch' },
      },
    },
  },
});
