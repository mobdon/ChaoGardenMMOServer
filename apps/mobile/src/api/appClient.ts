import { createApiClient, type ApiClient } from './client';
import { tokenStorage } from './tokenStorage';
import { createUiStore } from '@/state/uiStore';
import { ConnectionBannerModel } from '@/state/connection';

/**
 * App-wide singletons wired for the Expo runtime.
 * Tests construct their own instances with injected storage/fetch.
 */
export const connection = new ConnectionBannerModel();

export const api: ApiClient = createApiClient(
  process.env.EXPO_PUBLIC_API_BASE_URL ?? 'http://localhost:3001',
  // Access tokens stay in memory; the refresh token is kept in SecureStore.
  tokenStorage,
);

export function getUiStore() {
  return createUiStore;
}

export { createApiClient } from './client';
export { tokenStorage, loadPersistedRefresh } from './tokenStorage';
