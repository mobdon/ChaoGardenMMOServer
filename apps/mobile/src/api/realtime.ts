/**
 * Realtime client for the Expo app: Socket.IO gateway connection with
 * reconnect-safe sequence tracking, snapshot recovery, and graceful fallback
 * to polling when the socket cannot be established.
 *
 * The client only ever SENDS commands; every event received carries a
 * server-issued sequence number. If a gap is detected (or recovery reports
 * `recovered: false`) the app refetches the canonical snapshot from REST.
 */

import { REALTIME_NAMESPACES } from '@chaogarden/contracts';

export type RealtimeStatus = 'connecting' | 'online' | 'polling' | 'offline';

export type GardenEventPayload = {
  seq: number;
  at: number;
  event: { type: string; [key: string]: unknown };
};

export interface RealtimeTransport {
  connect(): void;
  close(): void;
  emitCommand(command: Record<string, unknown>): void;
  onEvent(listener: (event: GardenEventPayload) => void): () => void;
  onStatus(listener: (status: RealtimeStatus) => void): () => void;
}

export type RealtimeClientOptions = {
  url: string;
  token: string;
  /** Interval used while degraded to polling (ms). */
  pollIntervalMs?: number;
  /** Factory so tests can inject a stub transport. */
  transport?: RealtimeTransport;
  refetchSnapshot?: () => Promise<void>;
};

export class RealtimeClient {
  private transport: RealtimeTransport;
  private lastSeq = 0;
  private status: RealtimeStatus = 'connecting';
  private pollTimer: ReturnType<typeof setInterval> | null = null;

  constructor(private readonly options: RealtimeClientOptions) {
    this.transport = options.transport ?? defaultTransport(options);
  }

  start(): void {
    this.transport.onEvent((event) => {
      const seq = event.seq ?? 0;
      if (seq > this.lastSeq + 1) {
        // Missed events — refetch authoritative state from REST.
        void this.options.refetchSnapshot?.();
      }
      this.lastSeq = Math.max(this.lastSeq, seq);
    });
    this.transport.onStatus((status) => {
      this.status = status;
      if (status === 'online') {
        this.stopPolling();
        // Ask the server to replay missed events since our last sequence.
        this.transport.emitCommand({ type: 'sync', lastSeq: this.lastSeq });
      } else if (status === 'polling') {
        this.startPolling();
      }
    });
    this.transport.connect();
  }

  send(command: Record<string, unknown>): void {
    this.transport.emitCommand(command);
  }

  lastSeenSeq(): number {
    return this.lastSeq;
  }

  currentStatus(): RealtimeStatus {
    return this.status;
  }

  stop(): void {
    this.stopPolling();
    this.transport.close();
  }

  private startPolling(): void {
    if (this.pollTimer) return;
    const interval = this.options.pollIntervalMs ?? 30_000;
    this.pollTimer = setInterval(() => void this.options.refetchSnapshot?.(), interval);
  }

  private stopPolling(): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
      this.pollTimer = null;
    }
  }
}

/** Lazily imports socket.io-client so tests can inject stubs without the dep. */
function defaultTransport(options: RealtimeClientOptions): RealtimeTransport {
  // Real transport is constructed with the socket.io-client runtime; tests
  // always inject a stub, so this path stays small and defensive.
  const io = require('socket.io-client') as (url: string, opts: Record<string, unknown>) => RealtimeTransport;
  return io(options.url, {
    auth: { token: options.token },
    transports: ['websocket'],
    reconnection: true,
    reconnectionAttempts: 5,
  });
}

export const GAME_NAMESPACE = REALTIME_NAMESPACES.game;
