/**
 * The single authorization + validation + rate-limit + persistence path for
 * WebSocket commands. Realtime commands are treated exactly like REST
 * requests: they must pass through this service — the gateway never mutates
 * state directly, so multiplayer messages cannot bypass REST invariants.
 *
 * Hard rules enforced here:
 * - Only the garden owner may change decorations.
 * - Decoration placements reference catalog item keys the user already owns;
 *   the inventory is consumed server-side (client never sends stats/currency).
 * - Visits and daily rewards are recorded idempotently in PostgreSQL.
 * - Reactions come from a fixed vocabulary; no client-authored text is ever
 *   relayed to other clients.
 * - Message frequency is rate limited per socket identity.
 */
import { Injectable } from '@nestjs/common';
import {
  isReaction,
  DecorationPlacement,
  GardenSnapshot,
  PresenceRecord,
  Reaction,
  roomForUser,
} from '@chaogarden/contracts';
import type { AuthenticatedActor } from '../auth/auth.service';
import { AuthErrorCode } from '@chaogarden/contracts';
import { AuthException } from '../auth/errors';
import { ThrottleService } from '../auth/throttle/throttle.service';
import { DomainServices } from '../domain/domain.services';
import { DomainError, DomainErrorCode } from '../domain/domain.errors';
import { RealtimeService } from './realtime.service';

/** Errors surfaced to sockets as structured acks, never raw stack traces. */
export class RealtimeCommandError extends Error {
  constructor(
    public readonly code:
      | 'FORBIDDEN'
      | 'GARDEN_FULL'
      | 'RATE_LIMITED'
      | 'PAYLOAD_TOO_LARGE'
      | 'INVALID_COMMAND'
      | 'NOT_FOUND',
    message: string,
  ) {
    super(message);
  }
}

const GARDEN_GRID = 8; // 8x8 tiles; matches the mobile garden renderer
const VISIT_REWARD_RINGS = 2;

export function isValidPlacement(p: unknown): p is DecorationPlacement {
  if (typeof p !== 'object' || p === null) return false;
  const v = p as DecorationPlacement;
  return (
    typeof v.id === 'string' &&
    typeof v.catalogId === 'string' &&
    Number.isInteger(v.x) &&
    Number.isInteger(v.y) &&
    v.x >= 0 &&
    v.x < GARDEN_GRID &&
    v.y >= 0 &&
    v.y < GARDEN_GRID &&
    Number.isFinite(v.rotation)
  );
}

@Injectable()
export class GardenCommandService {
  constructor(
    private readonly domain: DomainServices,
    private readonly realtime: RealtimeService,
    private readonly throttle: ThrottleService,
  ) {}

  /** Shared per-socket command rate limit; mirrors the REST throttler keying. */
  async assertCommandRate(socketId: string): Promise<void> {
    try {
      await this.throttle.consume('ws-command', socketId, 30, 10);
    } catch {
      throw new RealtimeCommandError('RATE_LIMITED', 'Too many commands');
    }
  }

  /** Reactions have their own, tighter bucket. */
  async assertReactionRate(socketId: string): Promise<void> {
    try {
      await this.throttle.consume('ws-reaction', socketId, 10, 10);
    } catch {
      throw new RealtimeCommandError('RATE_LIMITED', 'Too many reactions');
    }
  }

  /**
   * Joins a garden room: authorizes (garden exists), records the visit with
   * its daily reward, seeds presence, and returns a snapshot. Guest visitors
   * are allowed to watch but never receive the reward until upgraded.
   */
  async joinGarden(actor: AuthenticatedActor, gardenId: string, socketId: string, displayName: string): Promise<GardenSnapshot> {
    const garden = await this.domain.gardens.findById(gardenId);
    if (!garden || garden.deletedAt) {
      throw new RealtimeCommandError('NOT_FOUND', 'Garden not found');
    }

    const visitor: PresenceRecord = {
      userId: actor.userId,
      profileName: displayName,
      socketId,
      gardenId,
      joinedAt: Date.now(),
    };
    const joined = await this.realtime.joinPresence(visitor);
    if (!joined) {
      throw new RealtimeCommandError('GARDEN_FULL', 'Garden is full');
    }

    if (garden.ownerUid !== actor.userId && !actor.guest) {
      // Daily visit reward; duplicate visits are a no-op in the domain layer.
      await this.domain.recordVisitWithReward({ gardenUid: gardenId, visitorUid: actor.userId, reward: VISIT_REWARD_RINGS }).catch(
        () => 'duplicate' as const, // reward failure never blocks a visit
      );
      this.domain.economy.audit(actor.userId, 'realtime.visit', gardenId, { gardenId }).catch(() => undefined);
    }

    const seq = await this.realtime.nextSeq(gardenId);
    return this.buildSnapshot(gardenId, seq);
  }

  /** Places a decoration. Owner-only; consumes inventory atomically. */
  async placeDecoration(actor: AuthenticatedActor, gardenId: string, placement: DecorationPlacement, idempotencyKey: string): Promise<void> {
    await this.assertOwner(actor, gardenId);
    if (!isValidPlacement(placement)) {
      throw new RealtimeCommandError('INVALID_COMMAND', 'Placement out of bounds');
    }
    try {
      await this.domain.placeDecoration({
        ownerUid: actor.userId,
        gardenUid: gardenId,
        itemKey: placement.catalogId,
        x: placement.x,
        y: placement.y,
        idempotencyKey,
      });
    } catch (err) {
      if (err instanceof DomainError && err.code === DomainErrorCode.TileOccupied) {
        throw new RealtimeCommandError('INVALID_COMMAND', 'Tile occupied');
      }
      throw err;
    }
    await this.realtime.emitToGarden(gardenId, 'garden:decoration:update', { placement });
  }

  /** Lifts a decoration back into inventory. Owner-only. */
  async removeDecoration(actor: AuthenticatedActor, gardenId: string, placementId: string): Promise<void> {
    await this.assertOwner(actor, gardenId);
    if (typeof placementId !== 'string' || placementId.length === 0 || placementId.length > 64) {
      throw new RealtimeCommandError('INVALID_COMMAND', 'Bad placement id');
    }
    const rows = await this.domain.gardens.listDecorations(gardenId);
    const row = rows.find((r) => r.id === placementId);
    if (!row) throw new RealtimeCommandError('NOT_FOUND', 'Placement not found');
    await this.domain.liftDecoration({ ownerUid: actor.userId, gardenUid: gardenId, x: row.x, y: row.y });
    await this.realtime.emitToGarden(gardenId, 'garden:decoration:update', { placement: null, placementId });
  }

  /** Relays an approved reaction from the fixed vocabulary. */
  async sendReaction(actor: AuthenticatedActor, gardenId: string, reaction: unknown): Promise<Reaction> {
    if (!isReaction(reaction)) {
      throw new RealtimeCommandError('INVALID_COMMAND', 'Unknown reaction');
    }
    await this.assertReactionRate(`${actor.userId}:${gardenId}`);
    await this.domain.economy.audit(actor.userId, 'realtime.reaction', gardenId, { reaction }).catch(() => undefined);
    await this.realtime.emitToGarden(gardenId, 'garden:reaction', { userId: actor.userId, reaction });
    return reaction;
  }

  /** Builds the public garden snapshot (trimmed creature data, no stats). */
  async buildSnapshot(gardenId: string, seq: number): Promise<GardenSnapshot> {
    const garden = await this.domain.gardens.findById(gardenId);
    if (!garden) throw new RealtimeCommandError('NOT_FOUND', 'Garden not found');
    const decorations = await this.domain.gardens.listDecorations(gardenId);
    const visitors = await this.realtime.listPresence(gardenId);
    // Public view only: trimmed creature fields, never raw stat blocks.
    const creatures = await this.domain.creatures.listByGarden(gardenId, 50);
    return {
      seq,
      serverTime: Date.now(),
      gardenId,
      ownerId: garden.ownerUid,
      decorations: decorations.map((d) => ({ id: d.id, catalogId: d.itemKey, x: d.x, y: d.y, rotation: d.rotation })),
      creatures: creatures.slice(0, 50).map((c) => ({ id: c.id, name: c.name, color: c.speciesKey, mood: c.mood })),
      visitors,
    };
  }

  /** Notifies a user's private room about notification counters. */
  notifyCounters(userId: string, counters: { unreadNotifications: number; pendingFriendVisits: number }): void {
    this.realtime.emitToUser(userId, 'notifications:counters', counters);
  }

  /** Room identifier for a user's private room (used by the gateway). */
  userRoom(userId: string): string {
    return roomForUser(userId);
  }

  private async assertOwner(actor: AuthenticatedActor, gardenId: string): Promise<void> {
    const garden = await this.domain.gardens.findById(gardenId);
    if (!garden || garden.deletedAt) {
      throw new RealtimeCommandError('NOT_FOUND', 'Garden not found');
    }
    if (garden.ownerUid !== actor.userId) {
      throw new AuthException(AuthErrorCode.Forbidden, 'Not your garden');
    }
  }
}
