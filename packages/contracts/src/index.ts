/**
 * Shared API contracts for the Chao Garden MMO mobile clients.
 *
 * Types mirror the Rust authority (`crates/tiny-garden-core`) snapshot and
 * action payloads. Every server response passes through a small hand-rolled
 * validator so an unexpected payload becomes a typed error instead of a crash.
 * All visuals referenced here are original placeholder geometry — no
 * copyrighted Sonic/Chao characters, logos, models or sounds.
 */

export type CreatureStat = {
  name: 'swim' | 'fly' | 'run' | 'power' | 'stamina' | 'luck' | 'intelligence';
  level: number;
  value: number;
};

export type CreatureAlignment = 'neutral' | 'hero' | 'dark';

export type Creature = {
  id: string;
  name: string;
  color: string;
  mood: number;
  hunger: number;
  belly: number;
  energy: number;
  hygiene: number;
  boredom: number;
  alignment: CreatureAlignment;
  stats: CreatureStat[];
  rings: number;
};

export type CareAction =
  | { kind: 'feed'; foodId: string }
  | { kind: 'pet' }
  | { kind: 'clean' }
  | { kind: 'sleep' }
  | { kind: 'play'; toyId: string };

export type InventoryItem = {
  id: string;
  /** Catalog id; resolves through apps/mobile asset registry placeholders. */
  catalogId: 'fruit' | 'vegetable' | 'toy' | 'training-equipment' | 'decoration';
  name: string;
  quantity: number;
};

export type DecorationPlacement = {
  id: string;
  catalogId: string;
  x: number;
  y: number;
  rotation: number;
};

export type TrainingSession = {
  id: string;
  stat: CreatureStat['name'];
  startedAt: number;
  durationMs: number;
};

export type Snapshot = {
  serverTime: number;
  creature: Creature | null;
  inventory: InventoryItem[];
  decorations: DecorationPlacement[];
  training: TrainingSession | null;
  rings: number;
};

export type SessionResponse = {
  token: string;
  refreshToken: string;
  expiresAt: number;
  profile: { id: string; name: string; guest: boolean };
};

export type AdoptionRequest = {
  name: string;
  color: string;
};

export type ApiError = {
  status: number;
  code: string;
  message: string;
};

/** Narrow runtime check helper; throws `ApiError` shaped failures. */
export function isApiError(value: unknown): value is ApiError {
  const v = value as ApiError | undefined;
  return (
    typeof v === 'object' &&
    v !== null &&
    typeof (v as ApiError).status === 'number' &&
    typeof (v as ApiError).code === 'string' &&
    typeof (v as ApiError).message === 'string'
  );
}

function hasFields<T extends object>(value: unknown, fields: (keyof T)[]): value is T {
  if (typeof value !== 'object' || value === null) return false;
  return fields.every((f) => f in (value as Record<string, unknown>));
}

export function validateSnapshot(value: unknown): Snapshot {
  if (
    !hasFields<Snapshot>(value, ['serverTime', 'creature', 'inventory', 'decorations', 'rings'])
  ) {
    throw new Error('Invalid snapshot payload');
  }
  return value as Snapshot;
}

export function validateSession(value: unknown): SessionResponse {
  if (!hasFields<SessionResponse>(value, ['token', 'refreshToken', 'expiresAt', 'profile'])) {
    throw new Error('Invalid session payload');
  }
  return value as SessionResponse;
}

export function validateCreature(value: unknown): Creature {
  if (!hasFields<Creature>(value, ['id', 'name', 'hunger', 'mood', 'stats'])) {
    throw new Error('Invalid creature payload');
  }
  return value as Creature;
}

/** Endpoint catalogue shared by the mobile app and tests. */
export const endpoints = {
  session: '/api/session',
  refresh: '/api/session/refresh',
  snapshot: '/api/snapshot',
  adopt: '/api/creatures',
  care: (creatureId: string) => `/api/creatures/${creatureId}/care`,
  inventory: '/api/inventory',
  useItem: '/api/inventory/use',
  decorations: '/api/decorations',
  training: '/api/training',
  progression: '/api/progression',
} as const;
