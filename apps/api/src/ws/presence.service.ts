/**
 * In-memory presence and rate-limit state used by the gateway. This module is
 * deliberately free of framework dependencies so gateway behavior can be tested
 * without Redis or PostgreSQL. Redis-backed counters (multi-instance fan-out,
 * cross-process rate limits) attach through the same public surface.
 */
import { RealtimeGatewayConfig } from './gateway.config';

type Actor = { accountId: string; handle: string };

const BUCKET_MS = 1_000;

interface SlidingWindow {
  windowStart: number;
  count: number;
}

export class PresenceService {
  private readonly occupants = new Map<string, Set<string>>(); // roomId -> socket ids
  private readonly actorBySocket = new Map<string, Actor>();
  private readonly heartbeatAt = new Map<string, number>(); // socket id -> last heartbeat
  private readonly rateBuckets = new Map<string, SlidingWindow>(); // key -> window
  private readonly unread = new Map<string, number>(); // accountId -> unread notifications
  private readonly roomsOfSocket = new Map<string, Set<string>>();

  constructor(private readonly config: RealtimeGatewayConfig) {}

  trackSocket(socketId: string, actor: Actor): void {
    this.actorBySocket.set(socketId, actor);
    this.roomsOfSocket.set(socketId, new Set());
  }

  forgetSocket(socketId: string): Array<string> {
    const rooms = this.roomsOfSocket.get(socketId) ?? new Set<string>();
    for (const room of rooms) {
      this.occupants.get(room)?.delete(socketId);
    }
    this.roomsOfSocket.delete(socketId);
    this.actorBySocket.delete(socketId);
    this.heartbeatAt.delete(socketId);
    return [...rooms];
  }

  actorOf(socketId: string): Actor | undefined {
    return this.actorBySocket.get(socketId);
  }

  roomsOf(socketId: string): Array<string> {
    return [...(this.roomsOfSocket.get(socketId) ?? [])];
  }

  inRoom(roomId: string, socketId: string): boolean {
    return this.roomsOfSocket.get(socketId)?.has(roomId) ?? false;
  }

  joinRoom(roomId: string, socketId: string): void {
    let set = this.roomsOfSocket.get(socketId);
    if (!set) {
      set = new Set();
      this.roomsOfSocket.set(socketId, set);
    }
    set.add(roomId);
    let occupants = this.occupants.get(roomId);
    if (!occupants) {
      occupants = new Set();
      this.occupants.set(roomId, occupants);
    }
    occupants.add(socketId);
  }

  leaveRoom(roomId: string, socketId: string): void {
    this.roomsOfSocket.get(socketId)?.delete(roomId);
    this.occupants.get(roomId)?.delete(socketId);
  }

  occupantsOf(roomId: string): Array<Actor> {
    const out: Array<Actor> = [];
    for (const socketId of this.occupants.get(roomId) ?? []) {
      const actor = this.actorBySocket.get(socketId);
      if (actor) out.push(actor);
    }
    return out;
  }

  occupantCount(roomId: string): number {
    return this.occupants.get(roomId)?.size ?? 0;
  }

  isRoomFull(roomId: string): boolean {
    return this.occupantCount(roomId) >= this.config.maxGardenOccupants;
  }

  touchHeartbeat(socketId: string): void {
    this.heartbeatAt.set(socketId, Date.now());
  }

  /** Room ids that appear abandoned (no heartbeat within the interval). */
  staleSocketIds(intervalMs: number): Array<string> {
    const now = Date.now();
    const out: Array<string> = [];
    for (const [socketId, at] of this.heartbeatAt) {
      if (now - at > intervalMs) out.push(socketId);
    }
    return out;
  }

  /** Sliding-window rate limit, shared key namespace so Redis can mirror it. */
  allow(key: string, limit: number, windowMs: number): boolean {
    const now = Date.now();
    const bucket = this.rateBuckets.get(key);
    if (!bucket || now - bucket.windowStart >= windowMs) {
      this.rateBuckets.set(key, { windowStart: now, count: 1 });
      return true;
    }
    bucket.count += 1;
    return bucket.count <= limit;
  }

  commandAllowed(socketId: string): boolean {
    return this.allow(`cmd:${socketId}`, this.config.commandsPerWindow, this.config.commandWindowMs);
  }

  reactionAllowed(accountId: string): boolean {
    return this.allow(`react:${accountId}`, this.config.reactionsPerWindow, this.config.reactionWindowMs);
  }

  pushNotification(accountId: string, _kind: string): number {
    const next = (this.unread.get(accountId) ?? 0) + 1;
    this.unread.set(accountId, next);
    return next;
  }

  resetUnread(accountId: string): void {
    this.unread.set(accountId, 0);
  }

  unreadCount(accountId: string): number {
    return this.unread.get(accountId) ?? 0;
  }
}

/** Coarse fixed window used for limiter buckets. */
export function bucketOf(now: number): number {
  return Math.floor(now / BUCKET_MS);
}
