import type { DbExecutor } from './types';
import { DomainError, DomainErrorCode, isUniqueViolation } from '../domain.errors';

/**
 * Friendships, blocks and reports.
 *
 * Conventions follow the other repositories: every method takes an optional
 * transaction `DbExecutor` so it composes with `DomainServices.runInTransaction`,
 * and mutating operations are idempotent at the DB level (unique keys or
 * explicit status checks) so retried requests can never double-apply.
 */
export class SocialRepository {
  constructor(private readonly db: DbExecutor) {}

  /**
   * Sends a friend request. Idempotent via the (requester, addressee) unique
   * key. A *mutual* race — both players request each other concurrently —
   * resolves inside the caller's transaction: when the mirror row exists
   * (in any state) both rows collapse into a single ACCEPTED friendship.
   */
  async sendRequest(
    requesterUid: string,
    addresseeUid: string,
    db: DbExecutor = this.db,
  ): Promise<'created' | 'duplicate' | 'accepted'> {
    if (requesterUid === addresseeUid) {
      throw new DomainError(DomainErrorCode.DuplicateEntry, 'Cannot befriend yourself');
    }
    if (await this.isBlockedEither(requesterUid, addresseeUid, db)) {
      throw new DomainError(DomainErrorCode.NotFound, 'Profile unavailable');
    }
    try {
      // Mutual race: if the other player already (or concurrently) requested
      // us, satisfy that request instead of queueing a second pending row.
      const mutual = await db.friendship.findUnique({
        where: { requesterUid_addresseeUid: { requesterUid: addresseeUid, addresseeUid: requesterUid } },
      });
      if (mutual) {
        if (mutual.status === 'ACCEPTED') return 'accepted';
        await db.friendship.update({
          where: { id: mutual.id },
          data: { status: 'ACCEPTED', respondedAt: new Date() },
        });
        return 'accepted';
      }
      const existing = await db.friendship.findUnique({
        where: { requesterUid_addresseeUid: { requesterUid, addresseeUid } },
      });
      if (existing) return 'duplicate'; // Idempotent replay.
      await db.friendship.create({ data: { requesterUid, addresseeUid } });
      return 'created';
    } catch (err) {
      if (isUniqueViolation(err)) return 'duplicate';
      throw err;
    }
  }

  async respond(
    friendshipId: string,
    addresseeUid: string,
    accept: boolean,
    db: DbExecutor = this.db,
  ): Promise<void> {
    const f = await db.friendship.findUnique({ where: { id: friendshipId } });
    if (!f || f.addresseeUid !== addresseeUid) {
      throw new DomainError(DomainErrorCode.NotFound, 'Friend request not found');
    }
    if (f.status !== 'PENDING') return; // Idempotent double-response.
    if (accept && (await this.isBlockedEither(addresseeUid, f.requesterUid, db))) {
      // A later block overrides an earlier friendship; accepting is a no-op.
      return;
    }
    await db.friendship.update({
      where: { id: friendshipId },
      data: { status: accept ? 'ACCEPTED' : 'REJECTED', respondedAt: new Date() },
    });
  }

  async listFriends(userId: string, db: DbExecutor = this.db): Promise<string[]> {
    const rows = await db.friendship.findMany({
      where: {
        status: 'ACCEPTED',
        OR: [{ requesterUid: userId }, { addresseeUid: userId }],
      },
    });
    return rows.map((r) => (r.requesterUid === userId ? r.addresseeUid : r.requesterUid));
  }

  async listPendingIncoming(userId: string, db: DbExecutor = this.db): Promise<string[]> {
    const rows = await db.friendship.findMany({
      where: { status: 'PENDING', addresseeUid: userId },
      orderBy: { createdAt: 'asc' },
    });
    return rows.map((r) => r.requesterUid);
  }

  async removeFriend(userId: string, otherUid: string, db: DbExecutor = this.db): Promise<void> {
    await db.friendship.deleteMany({
      where: {
        status: 'ACCEPTED',
        OR: [
          { requesterUid: userId, addresseeUid: otherUid },
          { requesterUid: otherUid, addresseeUid: userId },
        ],
      },
    });
  }

  async isFriend(a: string, b: string, db: DbExecutor = this.db): Promise<boolean> {
    const row = await db.friendship.findFirst({
      where: {
        status: 'ACCEPTED',
        OR: [
          { requesterUid: a, addresseeUid: b },
          { requesterUid: b, addresseeUid: a },
        ],
      },
      select: { id: true },
    });
    return Boolean(row);
  }

  // -- Blocks ---------------------------------------------------------------

  /**
   * Blocking always wins over friendship: the write removes any friendship
   * and pending requests between the pair in the same transaction, so blocks
   * "override" friendship for search, visits, reactions and matchmaking.
   */
  async block(blockerUid: string, blockedUid: string, db: DbExecutor = this.db): Promise<void> {
    if (blockerUid === blockedUid) return;
    await db.socialBlock.upsert({
      where: { blockerUid_blockedUid: { blockerUid, blockedUid } },
      create: { blockerUid, blockedUid },
      update: {},
    });
    await db.friendship.deleteMany({
      where: {
        OR: [
          { requesterUid: blockerUid, addresseeUid: blockedUid },
          { requesterUid: blockedUid, addresseeUid: blockerUid },
        ],
      },
    });
  }

  async unblock(blockerUid: string, blockedUid: string, db: DbExecutor = this.db): Promise<void> {
    await db.socialBlock.deleteMany({ where: { blockerUid, blockedUid } });
  }

  async listBlocked(blockerUid: string, db: DbExecutor = this.db): Promise<string[]> {
    const rows = await db.socialBlock.findMany({ where: { blockerUid } });
    return rows.map((r) => r.blockedUid);
  }

  /** Blocked in *either* direction — hidden profiles are symmetric. */
  async isBlockedEither(a: string, b: string, db: DbExecutor = this.db): Promise<boolean> {
    const row = await db.socialBlock.findFirst({
      where: {
        OR: [
          { blockerUid: a, blockedUid: b },
          { blockerUid: b, blockedUid: a },
        ],
      },
      select: { id: true },
    });
    return Boolean(row);
  }

  // -- Reports --------------------------------------------------------------

  /** Reports are append-only; a reporter may file repeatedly (rate limited upstream). */
  async createReport(
    input: { reporterUid: string; targetUid: string; category: string; reason?: string | null },
    db: DbExecutor = this.db,
  ): Promise<void> {
    await db.report.create({
      data: {
        reporterUid: input.reporterUid,
        targetUid: input.targetUid,
        category: input.category,
        reason: input.reason ?? null,
      },
    });
  }

  async listReports(reporterUid: string, db: DbExecutor = this.db) {
    return db.report.findMany({
      where: { reporterUid },
      orderBy: { createdAt: 'desc' },
      take: 50,
    });
  }

  // -- Visits ---------------------------------------------------------------

  /**
   * Logs a visit. One visit per garden/visitor/day; repeats are no-ops, so a
   * "daily visit reward" hook can fire only on 'created'.
   */
  async recordVisit(gardenUid: string, visitorUid: string, db: DbExecutor = this.db): Promise<'created' | 'duplicate'> {
    const today = new Date();
    const visitDay = new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate()));
    try {
      await db.gardenVisit.create({ data: { gardenUid, visitorUid, visitDay } });
      return 'created';
    } catch (err) {
      if (isUniqueViolation(err)) return 'duplicate';
      throw err;
    }
  }

  async listRecentVisitors(gardenUid: string, db: DbExecutor = this.db): Promise<string[]> {
    const rows = await db.gardenVisit.findMany({
      where: { gardenUid },
      orderBy: { createdAt: 'desc' },
      distinct: ['visitorUid'],
      take: 10,
    });
    return rows.map((r) => r.visitorUid);
  }
}
