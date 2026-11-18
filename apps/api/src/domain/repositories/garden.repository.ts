import type { DbExecutor, GardenView } from './types';
import { DomainError, DomainErrorCode, isUniqueViolation } from '../domain.errors';

/** Gardens and their placed decorations. */
export class GardenRepository {
  constructor(private readonly db: DbExecutor) {}

  async listByOwner(ownerUid: string): Promise<GardenView[]> {
    return this.db.garden.findMany({
      where: { ownerUid, deletedAt: null },
      orderBy: { createdAt: 'asc' },
    });
  }

  async findById(id: string): Promise<GardenView | null> {
    const g = await this.db.garden.findUnique({ where: { id } });
    if (!g) return null;
    return { id: g.id, ownerUid: g.ownerUid, name: g.name, theme: g.theme, deletedAt: g.deletedAt };
  }

  async create(
    input: { ownerUid: string; name: string; theme?: string },
    db: DbExecutor = this.db,
  ): Promise<GardenView> {
    try {
      const g = await db.garden.create({
        data: { ownerUid: input.ownerUid, name: input.name, theme: input.theme ?? 'neutral' },
      });
      return { id: g.id, ownerUid: g.ownerUid, name: g.name, theme: g.theme, deletedAt: g.deletedAt };
    } catch (err) {
      if (isUniqueViolation(err)) {
        throw new DomainError(DomainErrorCode.DuplicateEntry, 'Garden name already in use');
      }
      throw err;
    }
  }

  async listDecorations(gardenUid: string): Promise<Array<{ id: string; itemKey: string; x: number; y: number; rotation: number }>> {
    const rows = await this.db.placedDecoration.findMany({
      where: { gardenUid },
      include: { item: true },
      orderBy: [{ y: 'asc' }, { x: 'asc' }],
    });
    return rows.map((r) => ({ id: r.id, itemKey: r.item.key, x: r.x, y: r.y, rotation: r.rotation }));
  }

  /**
   * Places a decoration on a free tile, consuming one from the owner's
   * inventory. Runs inside a transaction (see GardenService) so a placement
   * and its inventory decrement commit or roll back together.
   */
  async place(
    input: { gardenUid: string; ownerUid: string; itemKey: string; x: number; y: number; rotation?: number },
    db: DbExecutor = this.db,
  ): Promise<void> {
    const item = await db.itemDefinition.findUniqueOrThrow({ where: { key: input.itemKey } });
    try {
      await db.placedDecoration.create({
        data: {
          gardenUid: input.gardenUid,
          itemId: item.id,
          x: input.x,
          y: input.y,
          rotation: input.rotation ?? 0,
        },
      });
    } catch (err) {
      if (isUniqueViolation(err)) {
        throw new DomainError(DomainErrorCode.TileOccupied, 'Tile already occupied', { x: input.x, y: input.y });
      }
      throw err;
    }
  }

  /** Removes a decoration and returns its item key so inventory can be restored. */
  async remove(gardenUid: string, x: number, y: number, db: DbExecutor = this.db): Promise<string | null> {
    const row = await db.placedDecoration.findUnique({
      where: { gardenUid_x_y: { gardenUid, x, y } },
      include: { item: true },
    });
    if (!row) return null;
    await db.placedDecoration.delete({ where: { id: row.id } });
    return row.item.key;
  }
}
