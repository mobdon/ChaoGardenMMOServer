/**
 * Sequence numbering, bounded event buffer, and Redis-backed fan-out.
 *
 * Sequence numbers are per-room and monotonically increasing. A bounded ring of
 * recent events per room allows reconnecting clients to replay missed events
 * without a full snapshot refetch. `publish` is the ONLY way events reach
 * rooms — clients cannot author broadcast payloads.
 */
import { Logger } from '@nestjs/common';
import Redis from 'ioredis';
import { RealtimeGatewayConfig } from './gateway.config';
import { PresenceService } from './presence.service';
import { REACTION_CATALOG } from '@chaogarden/contracts';

export interface RealtimeEventRecord {
  seq: number;
  at: number;
  type: string;
  payload: Record<string, unknown>;
}

export class RealtimeEventService {
  private readonly logger = new Logger(RealtimeEventService.name);
  /** room -> ring buffer of recent events */
  private readonly buffers = new Map<string, Array<RealtimeEventRecord>>();
  /** room -> last emitted sequence */
  private readonly sequences = new Map<string, number>();
  /** Optional Redis client; when present, events are also fanned out cross-instance. */
  private redis: Redis | null = null;

  constructor(
    private readonly presence: PresenceService,
    private readonly config: RealtimeGatewayConfig,
  ) {}

  attachRedis(client: Redis | null): void {
    this.redis = client;
  }

  get lastSeq(roomId: string): number {
    return this.sequences.get(roomId) ?? 0;
  }

  /**
   * Persist-first: callers must have completed authorization and persistence
   * before invoking publish. The payload is server-authoritative.
   */
  async publish(roomId: string, type: string, payload: Record<string, unknown>, emit: (room: string, event: string, body: unknown) => void): Promise<void> {
    const seq = (this.sequences.get(roomId) ?? 0) + 1;
    this.sequences.set(roomId, seq);
    const record: RealtimeEventRecord = { seq, at: Date.now(), type, payload };
    const buffer = this.buffers.get(roomId) ?? [];
    buffer.push(record);
    if (buffer.length > this.config.maxRecentEventsRetained) buffer.splice(0, buffer.length - this.config.maxRecentEventsRetained);
    this.buffers.set(roomId, buffer);

    emit(roomId, 'garden_event', { seq, at: record.at, event: { type, ...payload } });

    if (this.redis) {
      try {
        await this.redis.publish(`room:${roomId}`, JSON.stringify({ seq, at: record.at, type, payload }));
      } catch (err) {
        this.logger.warn(`redis fan-out failed room=${roomId} err=${(err as Error).message}`);
        // local delivery already succeeded; degradation is graceful.
      }
    }
  }

  /**
   * Reconnect recovery. Returns buffered events after `afterSeq` when the gap
   * is small enough, otherwise a snapshot refetch is required. A client that
   * has never observed this room (lastSeq 0) must take the snapshot path.
   */
  replay(roomId: string, afterSeq: number): Array<RealtimeEventRecord> | null {
    if (afterSeq < 1) return null;
    const buffer = this.buffers.get(roomId);
    if (!buffer) return null;
    const earliest = buffer[0]?.seq ?? this.lastSeq(roomId) + 1;
    if (afterSeq + 1 < earliest) return null; // gap too large — snapshot needed
    return buffer.filter((event) => event.seq > afterSeq);
  }

  /** Server-approved reaction keys only. */
  isApprovedReaction(key: string): boolean {
    return REACTION_CATALOG.some((entry) => entry.key === key);
  }

  /**
   * Sockets with no heartbeat within the configured interval are considered
   * gone; callers drop them and emit presence_leave.
   */
  reapStale(intervalMs: number): Array<string> {
    return this.presence.staleSocketIds(intervalMs);
  }
}
