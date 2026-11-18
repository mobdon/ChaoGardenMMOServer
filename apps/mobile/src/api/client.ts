import {
  endpoints,
  validateSnapshot,
  validateSession,
  type ApiError,
  type Snapshot,
  type SessionResponse,
} from '@chaogarden/contracts';

/**
 * Typed API client for the Rust authority.
 *
 * - Bearer auth with transparent refresh-token rotation and request replay.
 * - AbortController cancellation for every request (React Query passes signals).
 * - Idempotency keys on mutating requests so retried POSTs cannot double-charge
 *   inventory or care actions.
 * - Bounded exponential-backoff retries for transport failures, with offline
 *   detection surfaced as a typed `OfflineError`.
 */

export class OfflineError extends Error {
  name = 'OfflineError';
}
export class AuthExpiredError extends Error {
  name = 'AuthExpiredError';
}

export class ApiRequestError extends Error {
  constructor(public error: ApiError) {
    super(error.message);
    this.name = 'ApiRequestError';
  }
}

export type TokenStorage = {
  getTokens: () => Promise<{ token: string; refreshToken: string; expiresAt: number } | null>;
  setTokens: (t: { token: string; refreshToken: string; expiresAt: number } | null) => Promise<void>;
};

type SessionResult = { token: string; refreshToken: string; expiresAt: number };

export type RequestOptions = {
  method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  body?: unknown;
  signal?: AbortSignal;
  /** Generated per unique mutation; stable across optimistic-update retries. */
  idempotencyKey?: string;
  /** Skip the retry loop (used by the auth refresh call itself). */
  skipRetry?: boolean;
};

const RETRYABLE_CODES = new Set(['ECONNRESET', 'ETIMEDOUT', 'EAI_AGAIN', 'NETWORK']);

function backoff(attempt: number) {
  return Math.min(400 * 2 ** attempt, 3000);
}

async function waitFor(signal: AbortSignal | undefined, ms: number) {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, ms);
    signal?.addEventListener('abort', () => {
      clearTimeout(t);
      reject(new DOMException('Aborted', 'AbortError'));
    }, { once: true });
  });
}

export function createApiClient(
  baseUrl: string,
  storage: TokenStorage,
  fetchImpl: typeof fetch = fetch,
) {
  let refreshInFlight: Promise<boolean> | null = null;

  async function raw(path: string, opts: RequestOptions, token: string | null): Promise<Response> {
    const headers: Record<string, string> = { 'content-type': 'application/json' };
    if (token) headers.authorization = `Bearer ${token}`;
    if (opts.idempotencyKey) headers['idempotency-key'] = opts.idempotencyKey;
    return fetchImpl(`${baseUrl}${path}`, {
      method: opts.method ?? 'GET',
      headers,
      body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
      signal: opts.signal,
    });
  }

  async function refreshTokens(): Promise<boolean> {
    if (refreshInFlight) return refreshInFlight;
    refreshInFlight = (async () => {
      const current = await storage.getTokens();
      if (!current) return false;
      const res = await raw(endpoints.refresh, {
        method: 'POST',
        body: { refreshToken: current.refreshToken },
        skipRetry: true,
      }, null);
      if (!res.ok) {
        await storage.setTokens(null);
        return false;
      }
      const session = validateSession(await res.json());
      await storage.setTokens(session);
      return true;
    })().finally(() => {
      refreshInFlight = null;
    });
    return refreshInFlight;
  }

  async function request<T>(
    path: string,
    opts: RequestOptions,
    parse: (value: unknown) => T,
  ): Promise<T> {
    const stored = await storage.getTokens();
    let token = stored && stored.expiresAt > Date.now() ? stored.token : null;

    let attempt = 0;
    for (;;) {
      let res: Response;
      try {
        res = await raw(path, opts, token);
      } catch (err) {
        if (err instanceof DOMException && err.name === 'AbortError') throw err;
        // Transport failure: retry with backoff, then surface an offline error.
        if (attempt < 2 && !opts.skipRetry) {
          await waitFor(opts.signal, backoff(attempt));
          attempt += 1;
          continue;
        }
        throw new OfflineError('Network unavailable');
      }

      if (res.status === 401) {
        const ok = await refreshTokens();
        if (!ok) throw new AuthExpiredError('Session expired');
        token = (await storage.getTokens())?.token ?? null;
        continue;
      }

      if (res.status >= 500 && attempt < 2 && !opts.skipRetry) {
        await waitFor(opts.signal, backoff(attempt));
        attempt += 1;
        continue;
      }

      if (!res.ok) {
        const err = (await res.json().catch(() => ({ status: res.status, code: 'UNKNOWN', message: res.statusText }))) as ApiError;
        err.status = res.status;
        if (RETRYABLE_CODES.has(err.code) && attempt < 2 && !opts.skipRetry) {
          await waitFor(opts.signal, backoff(attempt));
          attempt += 1;
          continue;
        }
        throw new ApiRequestError(err);
      }

      return parse(await res.json());
    }
  }

  return {
    /** Guest or returning-player login. */
    async login(payload: { token?: string; guest: boolean; name?: string }): Promise<SessionResponse> {
      const session = await request<SessionResponse>(
        endpoints.session,
        { method: 'POST', body: payload, skipRetry: true },
        validateSession,
      );
      await storage.setTokens(session);
      return session;
    },

    async logout(): Promise<void> {
      await storage.setTokens(null);
    },

    async refresh(): Promise<boolean> {
      return refreshTokens();
    },

    snapshot: (signal?: AbortSignal) =>
      request<Snapshot>(endpoints.snapshot, { signal }, validateSnapshot),

    adopt: (payload: { name: string; color: string }, signal?: AbortSignal) =>
      request<Snapshot>(
        endpoints.adopt,
        { method: 'POST', body: payload, signal, idempotencyKey: `adopt-${payload.name}-${payload.color}` },
        validateSnapshot,
      ),

    care: (creatureId: string, body: Record<string, unknown>, idempotencyKey: string, signal?: AbortSignal) =>
      request<Snapshot>(
        endpoints.care(creatureId),
        { method: 'POST', body, signal, idempotencyKey },
        validateSnapshot,
      ),

    useItem: (body: { itemId: string; creatureId?: string }, idempotencyKey: string, signal?: AbortSignal) =>
      request<Snapshot>(
        endpoints.useItem,
        { method: 'POST', body, signal, idempotencyKey },
        validateSnapshot,
      ),

    moveDecoration: (body: { id: string; x: number; y: number; rotation: number }, idempotencyKey: string, signal?: AbortSignal) =>
      request<Snapshot>(
        endpoints.decorations,
        { method: 'PATCH', body, signal, idempotencyKey },
        validateSnapshot,
      ),

    startTraining: (body: { creatureId: string; stat: string }, idempotencyKey: string, signal?: AbortSignal) =>
      request<Snapshot>(
        endpoints.training,
        { method: 'POST', body, signal, idempotencyKey },
        validateSnapshot,
      ),
  };
}

export type ApiClient = ReturnType<typeof createApiClient>;
