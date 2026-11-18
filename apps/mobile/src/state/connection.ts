import { create } from 'zustand';

export type ConnectionState = 'online' | 'offline' | 'reconnecting' | 'expired';

/**
 * Connection status store: drives the always-visible status banner.
 * - `offline` while requests fail with OfflineError
 * - `reconnecting` while a retry loop is active
 * - `expired` after an unrefreshable 401, prompting re-authentication
 */
type State = {
  status: ConnectionState;
  lastSuccessAt: number | null;
  message: string | null;
  set: (status: ConnectionState, message?: string) => void;
  markSuccess: () => void;
};

export const createConnectionStore = create<State>((set) => ({
  status: 'online',
  lastSuccessAt: null,
  message: null,
  set: (status, message) => set({ status, message: message ?? null }),
  markSuccess: () => set({ status: 'online', message: null, lastSuccessAt: Date.now() }),
}));

/** Named export used by the app shell and by tests. */
export const connectionStore = createConnectionStore;
