/**
 * Realtime presence, garden visit, and notification contracts shared between the
 * mobile app and the API WebSocket gateway.
 *
 * Client → server messages are COMMANDS. They are validated, rate-limited, and
 * persisted before any event is fanned out. The gateway never relays client
 * stats, currency, or freeform text.
 */

import { ReactionKey, VisitedGarden, NotificationCategory } from './social';

export const REALTIME_NAMESPACES = { game: '/game' } as const;

/** Hard realtime bounds (mirrored by the gateway configuration). */
export const REALTIME_LIMITS = {
  maxPayloadBytes: 64 * 1024,
  maxGardensPerSocket: 8,
  maxGardenOccupants: 8,
  maxPresenceHeartbeatIntervalMs: 30_000,
  commandsPerWindow: 30,
  commandWindowMs: 10_000,
  reactionsPerWindow: 12,
  reactionWindowMs: 60_000,
  maxReactionKeyLength: 32,
  maxRecentEventsRetained: 128,
} as const;

export type GardenRoomId = `garden:${string}`;
export type UserRoomId = `user:${string}`;

export const gardenRoom = (ownerId: string): GardenRoomId => `garden:${ownerId}`;
export const userRoom = (accountId: string): UserRoomId => `user:${accountId}`;

/* ------------------------------------------------------------------ */
/* Server → client events                                              */
/* ------------------------------------------------------------------ */

export type PresenceSnapshotEntry = {
  accountId: string;
  handle: string;
  online: boolean;
};

export type RealtimeEventBase = { seq: number; at: number };

export type GardenEventType =
  | 'visitor_join'
  | 'visitor_leave'
  | 'reaction'
  | 'decoration_update'
  | 'presence'
  | 'competition_update';

export type GardenEventPayload =
  | { type: 'visitor_join'; visitor: { accountId: string; handle: string } }
  | { type: 'visitor_leave'; visitorId: string }
  | { type: 'reaction'; from: { accountId: string; handle: string }; reactionKey: ReactionKey }
  | { type: 'decoration_update'; decoration: { id: string; slot: string; catalogId: string } }
  | { type: 'presence'; presence: PresenceSnapshotEntry }
  | { type: 'competition_update'; competitionId: string; status: 'open' | 'closed'; closesAt: string };

export type GardenEventEnvelope = RealtimeEventBase & {
  gardenId: string;
  event: GardenEventPayload;
};

export type NotificationEnvelopeEvent = RealtimeEventBase & {
  category: NotificationCategory;
  kind: string;
  actor: { accountId: string; handle: string } | null;
  payload: Record<string, unknown>;
  /** Running unread counter after this notification was applied. */
  unreadCount: number;
};

export type PresenceChangeEvent = {
  accountId: string;
  online: boolean;
  at: number;
};

export type RealtimeSnapshot = {
  seq: number;
  garden: VisitedGarden | null;
  presence: PresenceSnapshotEntry[];
  unreadCount: number;
  competition: { id: string; status: 'open' | 'closed'; closesAt: string | null } | null;
};

export type SyncResponse =
  | { recovered: true; seq: number; events: Array<Omit<GardenEventEnvelope, 'gardenId'>>; snapshot: null }
  | { recovered: false; seq: number; events: []; snapshot: RealtimeSnapshot };

export type RealtimeErrorCode =
  | 'UNAUTHENTICATED'
  | 'FORBIDDEN'
  | 'ROOM_FULL'
  | 'ROOM_LIMIT'
  | 'NOT_IN_GARDEN'
  | 'RATE_LIMITED'
  | 'INVALID_PAYLOAD'
  | 'VISIT_NOT_ALLOWED'
  | 'REDIS_UNAVAILABLE'
  | 'UNKNOWN';

export type RealtimeError = { code: RealtimeErrorCode; message?: string };

/* ------------------------------------------------------------------ */
/* Client → server commands                                            */
/* ------------------------------------------------------------------ */

export type JoinGardenCommand = { type: 'join_garden'; ownerId: string; visitId?: string };
export type LeaveGardenCommand = { type: 'leave_garden'; ownerId: string };
export type SendReactionCommand = { type: 'send_reaction'; ownerId: string; reactionKey: string };
export type MoveDecorationCommand = { type: 'move_decoration'; ownerId: string; decorationId: string; slot: string };
export type HeartbeatCommand = { type: 'heartbeat' };
export type SyncCommand = { type: 'sync'; gardenId?: string; lastSeq: number };

export type RealtimeCommand =
  | JoinGardenCommand
  | LeaveGardenCommand
  | SendReactionCommand
  | MoveDecorationCommand
  | HeartbeatCommand
  | SyncCommand;

/* ------------------------------------------------------------------ */
/* Validators                                                          */
/* ------------------------------------------------------------------ */

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function str(value: unknown, max = 128): string | null {
  if (typeof value !== 'string' || value.length === 0 || value.length > max) return null;
  return value;
}

/** Server-side id shape: opaque identifiers, no user-authored content. */
function isId(value: unknown): boolean {
  return typeof value === 'string' && /^[A-Za-z0-9_-]{1,64}$/.test(value);
}

export function validateCommand(value: unknown): RealtimeCommand {
  if (!isRecord(value) || typeof value.type !== 'string') {
    throw new Error('Invalid realtime command');
  }
  switch (value.type) {
    case 'join_garden':
      if (!isId(value.ownerId)) throw new Error('Invalid join_garden');
      return { type: 'join_garden', ownerId: value.ownerId as string, visitId: isId(value.visitId) ? (value.visitId as string) : undefined };
    case 'leave_garden':
      if (!isId(value.ownerId)) throw new Error('Invalid leave_garden');
      return { type: 'leave_garden', ownerId: value.ownerId as string };
    case 'send_reaction':
      if (!isId(value.ownerId) || !isId(value.reactionKey)) throw new Error('Invalid send_reaction');
      return { type: 'send_reaction', ownerId: value.ownerId as string, reactionKey: value.reactionKey as string };
    case 'move_decoration':
      if (!isId(value.ownerId) || !isId(value.decorationId) || !isId(value.slot)) throw new Error('Invalid move_decoration');
      return {
        type: 'move_decoration',
        ownerId: value.ownerId as string,
        decorationId: value.decorationId as string,
        slot: value.slot as string,
      };
    case 'heartbeat':
      return { type: 'heartbeat' };
    case 'sync':
      if (typeof value.lastSeq !== 'number' || !Number.isFinite(value.lastSeq) || value.lastSeq < 0) {
        throw new Error('Invalid sync');
      }
      return {
        type: 'sync',
        gardenId: str(value.gardenId, 80) ?? undefined,
        lastSeq: value.lastSeq,
      };
    default:
      throw new Error('Unknown realtime command');
  }
}

export function validateRealtimeError(value: unknown): RealtimeError {
  if (!isRecord(value) || typeof value.code !== 'string') return { code: 'UNKNOWN' };
  return value as RealtimeError;
}
