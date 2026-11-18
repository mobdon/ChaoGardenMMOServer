import type { DbExecutor } from './types';
import { DomainError, DomainErrorCode } from '../domain.errors';

export interface VisitedGardenView {
  gardenUid: string;
  ownerUid: string;
  ownerName: string;
  name: string;
  theme: string;
  creatures: Array<{ id: string; name: string; mood: number; hunger: number }>;
  decorations: Array<{ id: string; x: number; y: number; itemId: string }>;
  /** Visitor mode is read-only: no care, decoration or economy mutations. */
  readonly: true;
}

/**
 * Garden visits: privacy enforcement plus the read-only snapshot handed to a
 * visitor. Visit permission is one of FRIENDS (default), OPEN or CLOSED; a
 * block in either direction always denies entry regardless of permission.
 */
export class VisitRepository {
  constructor(private readonly db: DbExecutor) {}

  /** Is `visitorUid` allowed into `ownerUid`'s garden right now? */
  async canVisit(
    visitorUid: string,
    ownerUid: string,
    db: DbExecutor = this.db,
  ): Promise<{ allowed: boolean; reason?: 'CLOSED' | 'FRIENDS_ONLY' | 'BLOCKED' }> {
    if (visitorUid === ownerUid) return { allowed: true };
    const blocked = await db.socialBlock.findFirst({
      where: {
        OR: [
          { blockerUid: visitorUid, blockedUid: ownerUid },
          { blockerUid: ownerUid, blockedUid: visitorUid },
        ],
      },
      select: { id: true },
    });
    if (blocked) return { allowed: false, reason: 'BLOCKED' };
    const owner = await db.profile.findUnique({
      where: { userId: ownerUid },
      select: { visitPermission: true },
    });
    if (!owner) return { allowed: false, reason: 'CLOSED' };
    if (owner.visitPermission === 'CLOSED') return { allowed: false, reason: 'CLOSED' };
    if (owner.visitPermission === 'FRIENDS') {
      const friend = await db.friendship.findFirst({
        where: {
          status: 'ACCEPTED',
          OR: [
            { requesterUid: visitorUid, addresseeUid: ownerUid },
            { requesterUid: ownerUid, addresseeUid: visitorUid },
          ],
        },
        select: { id: true },
      });
      if (!friend) return { allowed: false, reason: 'FRIENDS_ONLY' };
    }
    return { allowed: true };
  }

  /**
   * Read-only render of the owner's garden for the visitor. Creature stats and
   * owner economy are deliberately omitted — visitors see mood/needs only.
   */
  async snapshot(ownerUid: string, db: DbExecutor = this.db): Promise<VisitedGardenView> {
    const owner = await db.profile.findUnique({
      where: { userId: ownerUid },
      include: { user: true },
    });
    if (!owner) throw new DomainError(DomainErrorCode.NotFound, 'Garden not found');
    const garden = await db.garden.findFirst({
      where: { ownerUid, deletedAt: null },
      include: { creatures: { where: { retiredAt: null } }, decorations: true },
      orderBy: { createdAt: 'asc' },
    });
    if (!garden) throw new DomainError(DomainErrorCode.NotFound, 'Garden not found');
    return {
      gardenUid: garden.id,
      ownerUid,
      ownerName: owner.user.displayName,
      name: garden.name,
      theme: garden.theme,
      creatures: garden.creatures.map((c) => ({
        id: c.id,
        name: c.name,
        mood: c.mood,
        hunger: c.hunger,
      })),
      decorations: garden.decorations.map((d) => ({
        id: d.id,
        x: d.x,
        y: d.y,
        itemId: d.itemId,
      })),
      readonly: true,
    };
  }
}
