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

/** Competition schedule and entries. Duplicate entries are prevented by DB. */
export class CompetitionRepository {
  constructor(private readonly db: DbExecutor) {}

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
   * no-op and makes concurrent double-entry impossible.
   */
  async enter(
    input: { competitionUid: string; creatureUid: string; itemId?: string; idempotencyKey: string },
    db: DbExecutor = this.db,
  ): Promise<'created' | 'duplicate'> {
    const competition = await db.competition.findUnique({ where: { id: input.competitionUid } });
    if (!competition) throw new DomainError(DomainErrorCode.NotFound, 'Competition not found');
    const now = new Date();
    if (now < competition.startsAt || now > competition.endsAt) {
      throw new DomainError(DomainErrorCode.DuplicateEntry, 'Competition is not open for entries');
    }
    try {
      await db.competitionEntry.create({
        data: {
          competitionUid: input.competitionUid,
          creatureUid: input.creatureUid,
          itemId: input.itemId ?? null,
          idempotencyKey: input.idempotencyKey,
        },
      });
      return 'created';
    } catch (err) {
      if (isUniqueViolation(err)) return 'duplicate';
      throw err;
    }
  }

  async submitScore(entryId: string, score: number, db: DbExecutor = this.db): Promise<void> {
    await db.competitionEntry.update({ where: { id: entryId }, data: { score } });
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
