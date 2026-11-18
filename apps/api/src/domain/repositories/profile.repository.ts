import type { DbExecutor } from './types';
import type { Profile } from '@prisma/client';

/**
 * Read-mostly profile access. Currency *mutations* never go through here —
 * they must use EconomyService so ledger + cached balance stay consistent.
 *
 * Social/privacy extensions:
 * - `friendCode` is the exact-match discovery channel (MVP has no fuzzy
 *   handle search beyond opted-in discoverable profiles).
 * - Privacy flags (`discoverable`, `visitPermission`, `presenceVisible`,
 *   notification categories) are read here and enforced by the service layer.
 */
export class ProfileRepository {
  constructor(private readonly db: DbExecutor) {}

  async findByUserId(userId: string): Promise<Profile | null> {
    return this.db.profile.findUnique({ where: { userId } });
  }

  async requireByUserId(userId: string): Promise<Profile> {
    const profile = await this.findByUserId(userId);
    if (!profile) throw new Error(`Profile missing for user ${userId}`);
    return profile;
  }

  /** Leaderboard by level, then experience. Keyset-free simple take/skip. */
  async leaderboard(page: number, pageSize: number): Promise<Profile[]> {
    return this.db.profile.findMany({
      orderBy: [{ level: 'desc' }, { experience: 'desc' }],
      skip: page * pageSize,
      take: pageSize,
    });
  }

  // -- Privacy settings -----------------------------------------------------

  async updatePrivacy(
    userId: string,
    patch: {
      discoverable?: boolean;
      visitPermission?: 'FRIENDS' | 'OPEN' | 'CLOSED';
      presenceVisible?: boolean;
      notifyFriendRequests?: boolean;
      notifyVisits?: boolean;
      notifyCompetitions?: boolean;
    },
    db: DbExecutor = this.db,
  ): Promise<Profile> {
    return this.db.profile.update({ where: { userId }, data: patch });
  }

  /**
   * Exact friend-code lookup. Friend codes work regardless of the
   * `discoverable` flag — sharing a code is an explicit act of discovery —
   * but blocked pairs (either direction) are excluded by the service layer.
   */
  async findByFriendCode(code: string, db: DbExecutor = this.db): Promise<Profile | null> {
    const normalized = code.trim().toUpperCase();
    if (!/^[A-Z0-9]{4}-[A-Z0-9]{4}$/.test(normalized)) return null;
    return db.profile.findUnique({ where: { friendCode: normalized } });
  }

  /**
   * Search: exact friend-code match first, then handle prefix matches against
   * discoverable profiles only. Blocked accounts are filtered by the caller
   * via `SocialRepository.isBlockedEither` (kept out of Prisma for
   * composability inside transactions).
   */
  async search(
    query: string,
    db: DbExecutor = this.db,
  ): Promise<{ byCode: Profile | null; byHandle: Profile[] }> {
    const byCode = await this.findByFriendCode(query, db);
    const trimmed = query.trim();
    if (trimmed.length < 2) return { byCode, byHandle: [] };
    const byHandle = await db.profile.findMany({
      where: {
        user: { displayName: { startsWith: trimmed, mode: 'insensitive' } },
        discoverable: true,
      },
      take: 20,
      orderBy: { user: { displayName: 'asc' } },
    });
    return { byCode, byHandle };
  }
}
