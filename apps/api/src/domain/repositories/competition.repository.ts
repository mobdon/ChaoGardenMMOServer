import type { DbExecutor } from './types';
import { DomainError, DomainErrorCode, isUniqueViolation } from '../domain.errors';

export interface CompetitionView {
  id: string;
  key: string;
  name: string;
  kind: string;
  startsAt: Date;
  endsAt: Date;
  prize: number;
}

/**
 * Deterministic seeded RNG (mulberry32). Same seed → same sequence, so
 * competition scoring is reproducible on any server, replay or audit.
 */
export function seededRng(seed: string): () => number {
  let h = 1779033703 ^ seed.length;
  for (let i = 0; i < seed.length; i += 1) {
    h = Math.imul(h ^ seed.charCodeAt(i), 3432918353);
    h = (h << 13) | (h >>> 19);
  }
  let state = h >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * Competition schedule, entries and deterministic scoring.
 *
 * Scoring is server-generated from the creature's **checkpointed** stats plus
 * seeded randomness bound to `${competitionKey}:${creatureUid}`. Clients never
 * submit scores, and a creature's score within a round is stable, so standings
 * are identical no matter when or how often they are recomputed.
 */
export class CompetitionRepository {
  constructor(private readonly db: DbExecutor) {}

  /** Seeded stat score. Stat training dominates; jitter bounded to ±8%. */
  computeScore(
    roundSeed: string,
    creatureUid: string,
    stats: { power: number; stamina: number; swim: number; run: number; mood: number },
  ): number {
    const rng = seededRng(`${roundSeed}:${creatureUid}`);
    const weight =
      stats.power * 0.25 +
      stats.stamina * 0.25 +
      stats.swim * 0.15 +
      stats.run * 0.15 +
      stats.mood * 0.2;
    const jitter = 0.92 + rng() * 0.16;
    return Math.round(weight * jitter * 100);
  }

  async listOpen(now: Date, page = 0, pageSize = 20): Promise<CompetitionView[]> {
    const rows = await this.db.competition.findMany({
      where: { startsAt: { lte: now }, endsAt: { gte: now } },
      orderBy: { startsAt: 'asc' },
      skip: page * pageSize,
      take: pageSize,
    });
    return rows.map(CompetitionRepository.toView);
  }

  private static toView(c: {
    id: string;
    key: string;
    name: string;
    kind: string;
    startsAt: Date;
    endsAt: Date;
    prize: number;
  }): CompetitionView {
    return { id: c.id, key: c.key, name: c.name, kind: c.kind, startsAt: c.startsAt, endsAt: c.endsAt, prize: c.prize };
  }

  async findById(id: string): Promise<CompetitionView | null> {
    const c = await this.db.competition.findUnique({ where: { id } });
    return c ? CompetitionRepository.toView(c) : null;
  }

  /**
   * Enters a creature. `idempotencyKey` is typically
   * `${competitionUid}:${creatureUid}` — the unique index makes a re-entry a
   * no-op and makes concurrent double-entry impossible. The score is computed
   * server-side at entry time from checkpointed stats + seeded randomness.
   */
  async enter(
    input: {
      competitionUid: string;
      creatureUid: string;
      ownerUid: string;
      itemId?: string;
      idempotencyKey: string;
    },
    db: DbExecutor = this.db,
  ): Promise<{ status: 'created' | 'duplicate'; score: number }> {
    const competition = await db.competition.findUnique({ where: { id: input.competitionUid } });
    if (!competition) throw new DomainError(DomainErrorCode.NotFound, 'Competition not found');
    const now = new Date();
    if (now < competition.startsAt || now > competition.endsAt) {
      throw new DomainError(DomainErrorCode.DuplicateEntry, 'Competition is not open for entries');
    }
    const creature = await db.creature.findFirst({
      where: { id: input.creatureUid, ownerUid: input.ownerUid, retiredAt: null },
    });
    if (!creature) throw new DomainError(DomainErrorCode.NotFound, 'Creature not found');
    const stats = (creature.stats ?? {}) as Record<string, number>;
    const score = this.computeScore(competition.key, creature.id, {
      power: stats.power ?? 0,
      stamina: stats.stamina ?? 0,
      swim: stats.swim ?? 0,
      run: stats.run ?? 0,
      mood: creature.mood,
    });
    try {
      await db.competitionEntry.create({
        data: {
          competitionUid: input.competitionUid,
          creatureUid: input.creatureUid,
          itemId: input.itemId ?? null,
          idempotencyKey: input.idempotencyKey,
          score,
        },
      });
      return { status: 'created', score };
    } catch (err) {
      if (isUniqueViolation(err)) return { status: 'duplicate', score };
      throw err;
    }
  }

  async submitScore(entryId: string, score: number, db: DbExecutor = this.db): Promise<void> {
    await db.competitionEntry.update({ where: { id: entryId }, data: { score } });
  }

  /** Standings sorted by score (desc) then entry time (asc) for ties. */
  async standings(
    competitionUid: string,
    db: DbExecutor = this.db,
  ): Promise<Array<{ entryId: string; creatureUid: string; ownerUid: string; creatureName: string; score: number; placement: number }>> {
    const entries = await db.competitionEntry.findMany({
      where: { competitionUid },
      include: { creature: true },
      orderBy: [{ score: 'desc' }, { createdAt: 'asc' }],
    });
    let rank = 1;
    return entries.map((e, i) => {
      if (i > 0 && e.score < entries[i - 1]!.score) rank = i + 1;
      return {
        entryId: e.id,
        creatureUid: e.creatureUid,
        ownerUid: e.creature.ownerUid,
        creatureName: e.creature.name,
        score: e.score,
        placement: rank,
      };
    });
  }

  /** Finalises placements and returns the winning creature (if any). */
  async finalise(
    competitionUid: string,
    db: DbExecutor = this.db,
  ): Promise<{ creatureUid: string; placement: number; score: number } | null> {
    const entries = await db.competitionEntry.findMany({
      where: { competitionUid },
      orderBy: [{ score: 'desc' }, { createdAt: 'asc' }],
    });
    if (entries.length === 0) return null;
    let rank = 1;
    for (const [i, e] of entries.entries()) {
      if (i > 0 && e.score < entries[i - 1]!.score) rank = i + 1;
      await db.competitionEntry.update({ where: { id: e.id }, data: { placement: rank } });
    }
    const top = entries[0]!;
    return { creatureUid: top.creatureUid, placement: 1, score: top.score };
  }

  async listEntries(competitionUid: string, page = 0, pageSize = 50) {
    return this.db.competitionEntry.findMany({
      where: { competitionUid },
      orderBy: [{ score: 'desc' }],
      skip: page * pageSize,
      take: pageSize,
    });
  }
}
