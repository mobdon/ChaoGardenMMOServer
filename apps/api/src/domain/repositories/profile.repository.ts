import type { Profile } from '@prisma/client';
import type { DbExecutor } from './types';

/**
 * Read-mostly profile access. Currency *mutations* never go through here —
 * they must use EconomyService so ledger + cached balance stay consistent.
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
}
