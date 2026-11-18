import type { DbExecutor } from './types';
import { DomainError, DomainErrorCode } from '../domain.errors';

export interface LedgerEntry {
  id: string;
  delta: number;
  reason: string;
  meta: Record<string, unknown>;
  createdAt: Date;
}

/**
 * Append-only ring ledger service.
 *
 * Invariants:
 * - The ledger is immutable: rows are only ever inserted, never updated or
 *   deleted. Balance changes are auditable by replaying the ledger.
 * - Every mutating call carries an `idempotencyKey`; a repeat call with the
 *   same key is a no-op (DB unique index is the source of truth).
 * - Cached `Profile.rings` is updated in the same transaction as the ledger
 *   insert, with a SQL guard rejecting negative balances.
 */
export class EconomyRepository {
  constructor(private readonly db: DbExecutor) {}

  /** Awards rings. Re-running the same idempotencyKey is a silent no-op. */
  async award(
    input: {
      ownerUid: string;
      delta: number;
      reason: string;
      idempotencyKey: string;
      meta?: Record<string, unknown>;
    },
    db: DbExecutor = this.db,
  ): Promise<'applied' | 'duplicate'> {
    if (input.delta === 0) throw new DomainError(DomainErrorCode.IdempotencyConflict, 'Ledger delta must be non-zero');
    if (input.delta < 0) throw new DomainError(DomainErrorCode.IdempotencyConflict, 'Use spend() for deductions');

    const inserted = await db.ringLedger.createMany({
      data: {
        ownerUid: input.ownerUid,
        delta: input.delta,
        reason: input.reason,
        idempotencyKey: input.idempotencyKey,
        meta: input.meta ?? {},
      },
      skipDuplicates: true,
    });

    if (inserted.count === 0) return 'duplicate';

    try {
      await db.profile.update({
        where: { userId: input.ownerUid },
        data: { rings: { increment: input.delta } },
      });
    } catch (err) {
      // P2025: profile row missing — nothing else to do, rethrow for context.
      throw err;
    }
    return 'applied';
  }

  /**
   * Spends rings atomically. The balance guard is enforced in SQL so two
   * concurrent spends on the last rings cannot both succeed: the conditional
   * update only fires while `rings >= amount`.
   */
  async spend(
    input: {
      ownerUid: string;
      amount: number;
      reason: string;
      idempotencyKey: string;
      meta?: Record<string, unknown>;
    },
    db: DbExecutor = this.db,
  ): Promise<'applied' | 'duplicate' | 'insufficient'> {
    if (input.amount <= 0) throw new DomainError(DomainErrorCode.IdempotencyConflict, 'Amount must be positive');

    const inserted = await db.ringLedger.createMany({
      data: {
        ownerUid: input.ownerUid,
        delta: -input.amount,
        reason: input.reason,
        idempotencyKey: input.idempotencyKey,
        meta: input.meta ?? {},
      },
      skipDuplicates: true,
    });
    if (inserted.count === 0) return 'duplicate';

    const debited = await db.profile.updateMany({
      where: { userId: input.ownerUid, rings: { gte: input.amount } },
      data: { rings: { decrement: input.amount } },
    });

    if (debited.count !== 1) {
      // Roll the ledger row back by throwing; the caller's transaction will
      // undo the insert. (Outside a transaction this leaves an orphan row, so
      // always call spend() inside runInTransaction.)
      throw new DomainError(DomainErrorCode.InsufficientFunds, 'Insufficient rings', { amount: input.amount });
    }
    return 'applied';
  }

  async ledger(ownerUid: string, page = 0, pageSize = 50): Promise<LedgerEntry[]> {
    const rows = await this.db.ringLedger.findMany({
      where: { ownerUid },
      orderBy: { createdAt: 'desc' },
      skip: page * pageSize,
      take: pageSize,
    });
    return rows.map((r) => ({
      id: r.id,
      delta: r.delta,
      reason: r.reason,
      meta: r.meta as Record<string, unknown>,
      createdAt: r.createdAt,
    }));
  }

  /** Replays the ledger; diverges from the cached balance only on corruption. */
  async replayBalance(ownerUid: string): Promise<number> {
    const agg = await this.db.ringLedger.aggregate({
      where: { ownerUid },
      _sum: { delta: true },
    });
    return agg._sum.delta ?? 0;
  }

  /** Records an audit line. Never throws on duplicate actions (append-only). */
  async audit(
    userId: string,
    action: string,
    targetUid?: string,
    meta?: Record<string, unknown>,
    db: DbExecutor = this.db,
  ): Promise<void> {
    await db.auditLog.create({ data: { userId, action, targetUid, meta: meta ?? {} } });
  }
}
