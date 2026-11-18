/**
 * Lightweight in-memory token storage with an optional SecureStore-backed
 * implementation. Kept synchronous + swappable so tests can inject storage.
 */
let tokens: { token: string; refreshToken: string; expiresAt: number } | null = null;

export function hasTokens() {
  return tokens !== null;
}

export function getTokens() {
  return tokens;
}

export async function setTokens(next: typeof tokens) {
  tokens = next;
}

export const tokenStorage = { hasTokens, getTokens, setTokens };
