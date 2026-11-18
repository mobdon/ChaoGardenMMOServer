/**
 * Real-time presence, sequencing, and fan-out for garden rooms.
 *
 * Responsibilities:
 * - Presence registry keyed by (garden, user, socket) with a TTL so presence
 *   is cleaned up even if a process dies mid-session (crash safety).
 * - Monotonic per-room sequence numbers (`seq`) stored in Redis so all API
 *   replicas agree; falls back to in-memory counters in single-node dev.
 * - Fan-out through Redis pub/sub so sockets connected to any replica receive
 *   garden events. Without Redis, events stay local (single-node fallback).
 *
 * This service never persists gameplay state — that stays in PostgreSQL. It
 * only holds ephemeral presence and sequence metadata that is safe to lose.
 */
import { Inject, Injectable, OnModuleDestroy } from '@nestjs/common';
import type { Server } from 'socket.io';
import Redis from 'ioredis';
import {
  REALTIME_LIMITS,
  PresenceRecord,
  Sequenced,
  roomForGarden,
  roomForUser,
} from '@chaogarden/contracts';
import { REDIS_CLIENT } from '../auth/throttle/throttle.service';

const PRESENCE_TTL_SECONDS = 60; // heartbeat must refresh at least this often
const PRESENCE_KEY_PREFIX = 'presence:garden:';
const SEQ_KEY_PREFIX = 'seq:garden:';
const FANOUT_CHANNEL = 'chaogarden:realtime';

interface FanoutMessage {
  gardenId: string;
  event: string;
  seq?: number;
  payload: unknown;
  /** Room-scoped recipients; when omitted the event goes to the garden room. */
  alsoUserRooms?: string[];
}

@Injectable()
export class RealtimeService implements OnModuleDestroy {
  private server: Server | null = null;
  private subscriber: Redis | null = null;
  /** In-memory fallbacks when Redis is unavailable. */
  private readonly localPresence = new Map<string, PresenceRecord>();
  private readonly localSeq = new Map<string, number>();

  constructor(@Inject(REDIS_CLIENT) private readonly redis: Redis) {}

  /** Called by the gateway once the Socket.IO server is ready. */
  attachServer(server: Server): void {
    this.server = server;
    // Subscribe only when Redis is actually reachable; otherwise we degrade to
    // in-process fan-out without failing startup.
    this.redis
      .duplicate()
      .then((sub) => {
        this.subscriber = sub;
        sub.subscribe(FANOUT_CHANNEL).catch(() => {
          this.subscriber = null;
        });
        sub.on('message', (_channel: string, raw: string) => {
          void this.onFanoutMessage(raw);
        });
      })
      .catch(() => {
        this.subscriber = null;
      });
  }

  async onModuleDestroy(): Promise<void> {
    this.subscriber?.disconnect();
    this.localPresence.clear();
    this.localSeq.clear();
  }

  private fanoutEnabled(): boolean {
    return this.subscriber !== null && this.subscriber.status === 'subscribe';
  }

  /* ------------------------- presence ------------------------- */

  /**
   * Records presence for a socket inside a garden room. Returns false when the
   * garden is full (bounded room size).
   */
  async joinPresence(record: PresenceRecord): Promise<boolean> {
    const key = `${PRESENCE_KEY_PREFIX}${record.gardenId}`;
    if (this.redis.status === 'ready') {
      const count = await this.redis.hlen(key);
      if (count >= REALTIME_LIMITS.maxVisitorsPerGarden && !(await this.redis.hexists(key, record.socketId))) {
        return false;
      }
      await this.redis
        .multi()
        .hset(key, record.socketId, JSON.stringify(record))
        .expire(key, PRESENCE_TTL_SECONDS)
        .exec();
    } else {
      const room = [...this.localPresence.values()].filter((p) => p.gardenId === record.gardenId);
      if (room.length >= REALTIME_LIMITS.maxVisitorsPerGarden && !this.localPresence.has(record.socketId)) {
        return false;
      }
      this.localPresence.set(record.socketId, record);
    }
    return true;
  }

  /** Refreshes the presence TTL; returns false when the record was evicted. */
  async heartbeat(gardenId: string, socketId: string): Promise<boolean> {
    const key = `${PRESENCE_KEY_PREFIX}${gardenId}`;
    if (this.redis.status === 'ready') {
      const n = await this.redis.hexpire(key, PRESENCE_TTL_SECONDS, socketId).catch(() => 0);
      return n > 0;
    }
    return this.localPresence.has(socketId);
  }

  /** Removes one socket from presence and reports the user id (if any). */
  async leavePresence(gardenId: string, socketId: string): Promise<PresenceRecord | null> {
    const key = `${PRESENCE_KEY_PREFIX}${gardenId}`;
    if (this.redis.status === 'ready') {
      const raw = await this.redis.hget(key, socketId);
      if (raw === null) return null;
      await this.redis.hdel(key, socketId);
      return JSON.parse(raw) as PresenceRecord;
    }
    const record = this.localPresence.get(socketId) ?? null;
    if (record && record.gardenId === gardenId) {
      this.localPresence.delete(socketId);
      return record;
    }
    return null;
  }

  async listPresence(gardenId: string): Promise<PresenceRecord[]> {
    const key = `${PRESENCE_KEY_PREFIX}${gardenId}`;
    if (this.redis.status === 'ready') {
      const raw = await this.redis.hvals(key).catch(() => []);
      return raw.map((v) => JSON.parse(v) as PresenceRecord);
    }
    return [...this.localPresence.values()].filter((p) => p.gardenId === gardenId);
  }

  /** Sweeps expired Redis entries; called periodically by the gateway. */
  async sweepPresence(): Promise<void> {
    if (this.redis.status !== 'ready') return;
    const keys = await this.redis.keys(`${PRESENCE_KEY_PREFIX}*`);
    for (const key of keys) {
      const fields = await this.redis.hkeys(key);
      const values = await this.redis.hmget(key, fields);
      for (let i = 0; i < fields.length; i += 1) {
        if (values[i] === null) await this.redis.hdel(key, fields[i]);
      }
    }
  }

  /* ------------------------- sequencing ------------------------- */

  /** Next monotonic `seq` for a garden room. Redis-backed across replicas. */
  async nextSeq(gardenId: string): Promise<number> {
    const key = `${SEQ_KEY_PREFIX}${gardenId}`;
    if (this.redis.status === 'ready') {
      return this.redis.incr(key);
    }
    const next = (this.localSeq.get(gardenId) ?? 0) + 1;
    this.localSeq.set(gardenId, next);
    return next;
  }

  async currentSeq(gardenId: string): Promise<number> {
    const key = `${SEQ_KEY_PREFIX}${gardenId}`;
    if (this.redis.status === 'ready') {
      return Number((await this.redis.get(key)) ?? 0);
    }
    return this.localSeq.get(gardenId) ?? 0;
  }

  /**
   * Snapshot recovery: a reconnecting client sends its last known `seq`; if
   * the room has advanced past it the caller must send a full snapshot
   * instead of deltas.
   */
  async isBehind(gardenId: string, lastSeq: number | undefined): Promise<boolean> {
    if (lastSeq === undefined) return true; // no baseline -> full snapshot
    return (await this.currentSeq(gardenId)) > lastSeq;
  }

  /* ------------------------- fan-out ------------------------- */

  /**
   * Broadcasts a sequenced event to a garden room. With Redis available the
   * message goes through pub/sub so every replica's local sockets receive it.
   */
  async emitToGarden<T>(
    gardenId: string,
    event: string,
    data: T,
    options?: { alsoUserRooms?: string[] },
  ): Promise<void> {
    const seq = await this.nextSeq(gardenId);
    const message: FanoutMessage = {
      gardenId,
      event,
      seq,
      payload: data,
      alsoUserRooms: options?.alsoUserRooms,
    };
    if (this.fanoutEnabled()) {
      await this.redis.publish(FANOUT_CHANNEL, JSON.stringify(message));
      return;
    }
    this.deliver(message);
    return;
  }

  private async onFanoutMessage(raw: string): Promise<void> {
    try {
      this.deliver(JSON.parse(raw) as FanoutMessage);
    } catch {
      // Malformed pub/sub payload: ignore, state recovery is via snapshot.
    }
  }

  private deliver(message: FanoutMessage): void {
    if (!this.server) return;
    const envelope: Sequenced<unknown> = { seq: message.seq ?? 0, data: message.payload };
    this.server.to(roomForGarden(message.gardenId)).emit(message.event, envelope);
    for (const userId of message.alsoUserRooms ?? []) {
      this.server.to(roomForUser(userId)).emit(message.event, envelope);
    }
  }

  /** Emits a single-recipient event without sequencing (e.g. counters). */
  emitToUser(userId: string, event: string, payload: unknown): void {
    this.server?.to(roomForUser(userId)).emit(event, payload);
  }
}
