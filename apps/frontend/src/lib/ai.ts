import { useGetAiStatus } from '../api/generated/ai/ai';
import { body } from './api';

/**
 * The AI status while the AI features are on, `undefined` while they are off or not yet known.
 * They are on exactly when an admin has set a self-hosted language model (Administration → AI) or
 * the server has one in its environment; every AI control in the app renders nothing without it.
 */
export function useAiStatus() {
  const status = body(useGetAiStatus({ query: { staleTime: 60_000 } }).data);
  return status?.enabled ? status : undefined;
}
