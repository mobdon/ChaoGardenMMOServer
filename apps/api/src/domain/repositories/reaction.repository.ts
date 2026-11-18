import type { DbExecutor } from './types';
import { DomainError, DomainErrorCode, isUniqueViolation } from '../domain.errors';

/** Server-approved reaction presets — no freeform content in the MVP. */
export const REACTION_PRESETS = [
  { key: 'wave', label: 'Wave', requiresFriendship: false },
  { key: 'cheer', label: 'Cheer', requiresFriendship: false },
  { key: 'sparkle', label: 'Sparkle', requiresFriendship: true },
  { key: 'high_five', label: 'High five', requiresFriendship: true },
  { key: 'heart', label: 'Heart', requiresFriendship: true },
] as const;

export type ReactionPreset = (typeof REACTION_PRESETS)[number];

export function presetByKey(key: string): ReactionPreset | undefined {
  return REACTION_PRESETS.find((p) => p.key === key);
}

/**
 * Preset reactions left in gardens. Only catalogued presets are accepted
 * (validated here), friendship-gated presets are checked, and blocked pairs
 * are always rejected. Delivery itself is a notification row + WS event.
 */
export class ReactionRepository {
  constructor(private readonly db: DbExecutor) {}

  async send(
    input: { senderUid: string; targetUid: string; reactionKey: string; creatureUid?: string | null },
    isFriend: boolean,
    db: DbExecutor = this.db,
  ): Promise<void> {
    const preset = presetByKey(input.reactionKey);
    if (!preset) {
      throw new DomainError(DomainErrorCode.NotFound, 'Unknown reaction preset');
    }
    if (input.senderUid === input.targetUid) {
      throw new DomainError(DomainErrorCode.DuplicateEntry, 'Cannot react to yourself');
    }
    if (preset.requiresFriendship && !isFriend) {
      throw new DomainError(DomainErrorCode.NotFound, 'Reaction requires friendship');
    }
    try {
      await db.reaction.create({
        data: {
          senderUid: input.senderUid,
          targetUid: input.targetUid,
          reactionKey: preset.key,
          creatureUid: input.creatureUid ?? null,
        },
      });
    } catch (err) {
      if (isUniqueViolation(err)) return; // idempotent replay
      throw err;
    }
  }

  async listForTarget(targetUid: string, limit = 30, db: DbExecutor = this.db) {
    return db.reaction.findMany({
      where: { targetUid },
      orderBy: { createdAt: 'desc' },
      take: limit,
    });
  }
}
