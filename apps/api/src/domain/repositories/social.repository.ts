import type { DbExecutor } from './types';
import { DomainError, DomainErrorCode, isUniqueViolation } from '../domain.errors';

/** Friendships and garden visits. */
export class SocialRepository {
  constructor(private readonly db: DbExecutor) {}

  /** Sends a friend request; idempotent via the (requester, addressee) key. */
  async sendRequest(requesterUid: string, addresseeUid: string, db: DbExecutor = this.db): Promise<'created' | 'duplicate'> {
    if (requesterUid === addresseeUid) {
      throw new DomainError(DomainErrorCode.DuplicateEntry, 'Cannot befriend yourself');
    }
    try {
      await db.friendship.create({ data: { requesterUid, addresseeUid } });
      return 'created';
    } catch (err) {
      if (isUniqueViolation(err)) return 'duplicate';
      throw err;
    }
  }

  async respond(friendshipId: string, addresseeUid: string, accept: boolean, db: DbExecutor = this.db): Promise<void> {
    const f = await db.friendship.findUnique({ where: { id: friendshipId } });
    if (!f || f.addresseeUid !== addresseeUid) {
      throw new DomainError(DomainErrorCode.NotFound, 'Friend request not found');
    }
    if (f.status !== 'PENDING') return; // Idempotent double-response.
    await db.friendship.update({
      where: { id: friendshipId },
      data: { status: accept ? 'ACCEPTED' : 'REJECTED', respondedAt: new Date() },
    });
  }

  async listFriends(userId: string): Promise<string[]> {
    const rows = await this.db.friendship.findMany({
      where: {
        status: 'ACCEPTED',
        OR: [{ requesterUid: userId }, { addresseeUid: userId }],
      },
    });
    return rows.map((r) => (r.requesterUid === userId ? r.addresseeUid : r.requesterUid));
  }

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
}
