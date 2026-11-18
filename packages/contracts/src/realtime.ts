/**
 * Realtime contracts shared by the NestJS gateway and the Expo client.
 *
 * Design rules enforced here and on the server:
 * - Client -> server messages are *commands* only. They carry identifiers and
 *   catalog ids, never creature stats, currency values, or free-form text.
 * - Every server -> client event is stamped with a per-room monotonic `seq`
 *   so clients can detect missed events and refetch a snapshot.
 * - Payload sizes, reaction choices, and room sizes are bounded here.
 */
import { DecorationPlacement, Snapshot } from './index';

/** Hard caps (mirrored server-side; server values are authoritative). */
export const REALTIME_LIMITS = {
  /** Max concurrent visitors tracked in a single garden room. */
  maxVisitorsPerGarden: 24,
  /** Max sockets per user across all gardens. */
  maxSocketsPerUser: 4,
  /** Max garden rooms one socket may occupy. */
  maxRoomsPerSocket: 2,
  /** Client -> server commands per socket per window. */
  commandRateLimit: { windowMs: 10_000, max: 30 },
  /** Reaction events per socket per window. */
  reactionRateLimit: { windowMs: 10_000, max: 10 },
  /** Max JSON payload bytes accepted on any command. */
  maxCommandPayloadBytes: 2048,
} as const;

/** Fixed reaction vocabulary — no free-form emoji/text from clients. */
export const REACTIONS = ['wave', 'heart', 'star', 'laugh', 'sad'] as const;
export type Reaction = (typeof REACTIONS)[number];
export const isReaction = (v: unknown): v is Reaction =>
  typeof v === 'string' && (REACTIONS as readonly string[]).includes(v);

/** Rooms: one per user (private notifications) and one per garden. */
export const roomForUser = (userId: string) => `user:${userId}`;
export const roomForGarden = (gardenId: string) => `garden:${gardenId}`;

/** Envelope attached to every server -> client room event. */
export interface Sequenced<T> {
  seq: number;
  data: T;
}

/** Canonical real-time user presence record. */
export interface PresenceRecord {
  userId: string;
  profileName: string;
  socketId: string;
  gardenId: string | null;
  joinedAt: number;
}

/** Server computes a trimmed public snapshot of a garden for visitors. */
export interface GardenSnapshot {
  seq: number;
  serverTime: number;
  gardenId: string;
  ownerId: string;
  decorations: DecorationPlacement[];
  creatures: { id: string; name: string; color: string; mood: number }[];
  visitors: PresenceRecord[];
}

/** Minimal owner-visible view used for snapshot recovery after reconnect. */
export interface GardenDelta {
  seq: number;
  decorations?: DecorationPlacement[];
  visitors?: { joined: PresenceRecord[]; left: string[] };
}

/* ------------------------------------------------------------------ */
/* Client -> server (commands)                                         */
/* ------------------------------------------------------------------ */

export interface GardenJoinCommand {
  gardenId: string;
  /** Highest sequence the client has seen; server replays if higher exists. */
  lastSeq?: number;
}

export interface GardenLeaveCommand {
  gardenId: string;
}

export interface DecorationPlaceCommand {
  gardenId: string;
  placement: DecorationPlacement;
}

export interface DecorationRemoveCommand {
  gardenId: string;
  placementId: string;
}

export interface ReactionSendCommand {
  gardenId: string;
  reaction: Reaction;
}

/** A client message is always a namespaced command envelope. */
export type ClientCommand =
  | { type: 'garden:join'; payload: GardenJoinCommand }
  | { type: 'garden:leave'; payload: GardenLeaveCommand }
  | { type: 'garden:decoration:place'; payload: DecorationPlaceCommand }
  | { type: 'garden:decoration:remove'; payload: DecorationRemoveCommand }
  | { type: 'garden:reaction'; payload: ReactionSendCommand };

/* ------------------------------------------------------------------ */
/* Server -> client                                                    */
/* ------------------------------------------------------------------ */

export type PresenceEvent =
  | { kind: 'visitor-joined'; visitor: PresenceRecord }
  | { kind: 'visitor-left'; userId: string };

export type CompetitionStatusEvent = {
  competitionId: string;
  phase: 'open' | 'judging' | 'results';
  endsAt: number;
};

export type NotificationCountersEvent = {
  unreadNotifications: number;
  pendingFriendVisits: number;
};

export interface RealtimeError {
  code:
    | 'UNAUTHENTICATED'
    | 'FORBIDDEN'
    | 'GARDEN_FULL'
    | 'RATE_LIMITED'
    | 'PAYLOAD_TOO_LARGE'
    | 'INVALID_COMMAND'
    | 'GAP_DETECTED';
  message?: string;
}

/** Full event map used by the typed client. */
export interface ServerEvents {
  'presence:update': Sequenced<PresenceEvent>;
  'garden:snapshot': GardenSnapshot;
  'garden:delta': Sequenced<GardenDelta>;
  'garden:decoration:update': Sequenced<{ placement: DecorationPlacement | null; placementId?: string }>;
  'garden:reaction': Sequenced<{ userId: string; reaction: Reaction }>;
  'competition:status': Sequenced<CompetitionStatusEvent>;
  'notifications:counters': NotificationCountersEvent;
  'realtime:error': RealtimeError;
}

/** Socket.IO client emits these; the gateway validates before dispatch. */
export interface ClientEvents {
  command: (cmd: ClientCommand, ack: (res: { ok: true } | { ok: false; error: RealtimeError }) => void) => void;
  heartbeat: (ack: (res: { ok: true; seq: number }) => void) => void;
}
