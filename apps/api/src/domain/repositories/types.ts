import type { PrismaClient, Prisma } from '@prisma/client';

/**
 * Anything that can execute queries: the shared client, or a transaction
 * client supplied by `$transaction`. Repositories depend on this type so
 * callers can compose multi-repository operations atomically.
 */
export type DbExecutor = PrismaClient | Prisma.TransactionClient;

export type { Prisma };

/** Shared repository row shapes (no Prisma model types leak past this layer). */

export interface CreatureView {
  id: string;
  ownerUid: string;
  gardenUid: string | null;
  name: string;
  speciesKey: string;
  alignment: 'HERO' | 'NEUTRAL' | 'DARK';
  stats: Record<string, number>;
  traits: Array<Record<string, unknown>>;
  level: number;
  experience: number;
  mood: number;
  hunger: number;
  retiredAt: Date | null;
}

export interface GardenView {
  id: string;
  ownerUid: string;
  name: string;
  theme: string;
  deletedAt: Date | null;
}

export interface ItemStackView {
  itemId: string;
  itemKey: string;
  kind: 'FOOD' | 'TRAINING' | 'DECORATION';
  name: string;
  quantity: number;
}

export interface CreatureStats {
  swim: number;
  fly: number;
  run: number;
  power: number;
  stamina: number;
}
