/**
 * Domain types for the deterministic creature simulation.
 *
 * All structures are immutable: functions return fresh objects instead of
 * mutating inputs, so checkpoints can be safely shared and replayed.
 */

/** Fixed-point encoded numbers are plain bigints at runtime. */
export type Fixed = bigint;

export type StatName = 'swim' | 'fly' | 'run' | 'power' | 'stamina';
export type AllStatName = StatName | 'luck' | 'intelligence';

export type CreatureStage = 'egg' | 'baby' | 'child' | 'adult' | 'elder';
export type EvolutionTrait = 'neutral' | 'hero' | 'dark';

export type CommandType =
  | 'feed'
  | 'sleep'
  | 'wake'
  | 'clean'
  | 'play'
  | 'pet'
  | 'train'
  | 'check';

export interface CreatureSnapshot {
  readonly creatureId: string;
  readonly createdAt: number;
  /** Growth clock anchor; protected against clock rollback. */
  readonly bornAt: number;
  /** Highest wall-clock timestamp ever observed (anti-rollback watermark). */
  readonly highWatermark: number;
  /** Last simulated instant. */
  readonly lastTickedAt: number;
  readonly hunger: Fixed;      // 0..MAX_NEED
  readonly energy: Fixed;
  readonly hygiene: Fixed;
  readonly mood: Fixed;
  readonly sleeping: boolean;
  readonly neglectAccumMs: bigint;
  readonly sick: boolean;
  readonly stage: CreatureStage;
  readonly evolved: boolean;
  readonly trait: EvolutionTrait;
  readonly typeAdjective: string | null;
  readonly stats: Readonly<Record<AllStatName, StatValue>>;
  readonly heroPoints: number;
  readonly darkPoints: number;
  readonly lastActionAt: Readonly<Partial<Record<CommandType, number>>>;
  readonly rngState: bigint;
}

export interface StatValue {
  readonly grade: number;   // 1..5 for main stats, 1..255 for luck/int
  readonly level: number;   // 1..99
  readonly points: number;  // progress within level, 0..POINTS_PER_LEVEL-1 plus banked
}

export interface CreatureCheckpoint {
  readonly version: number;
  readonly snapshot: CreatureSnapshot;
  /** Index into the master command log the checkpoint corresponds to. */
  readonly commandIndex: number;
}

/** A single player-issued command. Timestamps are server-assigned. */
export interface SimCommand {
  readonly type: CommandType;
  readonly atMs: number;              // authoritative server receive time
  readonly stat?: AllStatName;
  readonly minutes?: number;          // train session length
  readonly item?: 'basic' | 'hero' | 'dark';
}

export type DomainEvent =
  | { kind: 'needsTicked'; atMs: number; hunger: Fixed; energy: Fixed; hygiene: Fixed; mood: Fixed }
  | { kind: 'stageAdvanced'; atMs: number; from: CreatureStage; to: CreatureStage }
  | { kind: 'evolved'; atMs: number; trait: EvolutionTrait; typeAdjective: string }
  | { kind: 'gradeUp'; atMs: number; stat: AllStatName; grade: number }
  | { kind: 'levelUp'; atMs: number; stat: AllStatName; level: number }
  | { kind: 'actionRejected'; atMs: number; type: CommandType; reason: 'cooldown' | 'invalid' }
  | { kind: 'neglectWarning'; atMs: number }
  | { kind: 'neglectSick'; atMs: number }
  | { kind: 'recovered'; atMs: number }
  | { kind: 'reward'; atMs: number; rings: number; reason: 'care' | 'training' | 'milestone' };

export interface SimResult {
  readonly snapshot: CreatureSnapshot;
  readonly events: readonly DomainEvent[];
}

/** Metadata exposed for versioning/migration by consumers. */
export const SIMULATION_VERSION = 1;
