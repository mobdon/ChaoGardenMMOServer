/**
 * Token storage for the Expo client.
 *
 * Security contract:
 * - Access tokens live in memory only (never persisted) so a stolen device
 *   backup or JS leak cannot yield a usable bearer token for long.
 * - Refresh tokens are persisted in `expo-secure-store` (Keychain /
 *   Keystore-backed encryption). We store only the refresh token + expiry.
 * - If SecureStore is unavailable (e.g. tests, web) we degrade to memory so
 *   rotation still works, refresh tokens simply do not survive a cold start.
 * - `clear()` is called on revocation/logout so no identity remains on disk.
 */
import type { TokenStore } from './client';

// `expo-secure-store` is optional at runtime: keep the import lazy so the
// in-memory fallback works in Jest and on web builds without the native module.
type SecureStoreLike = {
  getItemAsync(key: string): Promise<string | null>;
  setItemAsync(key: string, value: string): Promise<void>;
  deleteItemAsync(key: string): Promise<void>;
};

let secureStore: SecureStoreLike | null = null;
try {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const mod = require('expo-secure-store');
  secureStore = (mod.default ?? mod) as SecureStoreLike;
} catch {
  secureStore = null;
}

const REFRESH_KEY = 'chaogarden.refresh.v1';

export type StoredTokens = {
  /** Short-lived access token. Memory only. */
  token: string;
  /** Rotating refresh token. Persisted in SecureStore when available. */
  refreshToken: string;
  /** Epoch seconds at which the access token expires. */
  expiresAt: number;
};

let memory: { token: string; expiresAt: number } | null = null;

async function persistRefresh(refreshToken: string): Promise<void> {
  if (!secureStore) return;
  try {
    await secureStore.setItemAsync(REFRESH_KEY, refreshToken);
  } catch {
    // Never crash the app because secure storage failed; session is memory-only.
  }
}

async function clearPersistedRefresh(): Promise<void> {
  if (!secureStore) return;
  try {
    await secureStore.deleteItemAsync(REFRESH_KEY);
  } catch {
    // Best effort.
  }
}

export async function loadPersistedRefresh(): Promise<string | null> {
  if (!secureStore) return null;
  try {
    return await secureStore.getItemAsync(REFRESH_KEY);
  } catch {
    return null;
  }
}

export const tokenStorage: TokenStore = {
  async getTokens() {
    if (!memory) return null;
    const refreshToken = await loadPersistedRefresh();
    if (!refreshToken) return null;
    return { token: memory.token, refreshToken, expiresAt: memory.expiresAt };
  },
  async setTokens(next) {
    if (!next) {
      memory = null;
      await clearPersistedRefresh();
      return;
    }
    memory = { token: next.token, expiresAt: next.expiresAt };
    await persistRefresh(next.refreshToken);
  },
};

/** Test-only: swap the SecureStore implementation (or null to force memory). */
export function setSecureStoreForTesting(impl: SecureStoreLike | null) {
  secureStore = impl;
}

export function hasTokens(): boolean {
  return memory !== null;
}
