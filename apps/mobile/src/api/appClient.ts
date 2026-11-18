import { createApiClient, type ApiClient } from './client';
import { hasTokens, getTokens, setTokens } from './tokenStorage';
import { createUiStore } from '@/state/uiStore';
import { ConnectionBannerModel } from '@/state/connection';

/**
 * App-wide singletons wired for the Expo runtime.
 * Tests construct their own instances with injected storage/fetch.
 */
export const connection = new ConnectionBannerModel();

export const api: ApiClient = createApiClient(
  process.env.EXPO_PUBLIC_API_BASE_URL ?? 'http://localhost:3001',
  {
    getTokens: async () => (hasTokens() ? getTokens() : null),
    setTokens: setTokens,
  },
);

export function getUiStore() {
  return createUiStore;
}

export { createApiClient } from './client';
export { tokenStorage } from './tokenStorage';
