import type { DbExecutor, ItemStackView } from './types';
import { DomainError, DomainErrorCode, isUniqueViolation } from '../domain.errors';

/**
 * Inventory reads and single-stack mutations.
 *
 * All quantity changes run inside transactions (`runInTransaction` passes a
 * transaction executor through) and use conditional `updateMany` guards so
 * concurrent consumers cannot oversell a stack.
 */
export class InventoryRepository {
  constructor(private readonly db: DbExecutor) {}

  async listStacks(ownerUid: string, page = 0, pageSize = 50): Promise<ItemStackView[]> {
    const rows = await this.db.inventoryStack.findMany({
      where: { ownerUid },
      include: { item: true },
      orderBy: [{ item: { kind: 'asc' } }, { item: { name: 'asc' } }],
      skip: page * pageSize,
      take: pageSize,
    });
    return rows.map((r) => ({
      itemId: r.itemId,
      itemKey: r.item.key,
      kind: r.item.kind,
      name: r.item.name,
      quantity: r.quantity,
    }));
  }

  async stackQuantity(ownerUid: string, itemKey: string): Promise<number> {
    const row = await this.db.inventoryStack.findFirst({
      where: { ownerUid, item: { key: itemKey } },
    });
    return row?.quantity ?? 0;
  }

  /**
   * Grants `quantity` of an item, merging into an existing stack capped by the
   * item's `maxStack`. Overflow spawns no rows — callers split grants.
   */
  async grant(ownerUid: string, itemKey: string, quantity: number, db: DbExecutor = this.db): Promise<void> {
    if (quantity <= 0) throw new DomainError(DomainErrorCode.IdempotencyConflict, 'Grant quantity must be positive');
    const item = await db.itemDefinition.findUniqueOrThrow({ where: { key: itemKey } });
    if (!item.stackable) quantity = Math.min(quantity, 1);

    await db.inventoryStack.upsert({
      where: { ownerUid_itemId: { ownerUid, itemId: item.id } },
      create: { ownerUid, itemId: item.id, quantity: Math.min(quantity, item.maxStack) },
      update: { quantity: { increment: quantity } },
    });
  }

  /**
   * Atomically consumes `quantity` from a stack. The guard clause in
   * `updateMany` makes consumption safe under concurrency: a race loser
   * observes quantity < requested and rolls back with InsufficientStock.
   */
  async consume(
    ownerUid: string,
    itemKey: string,
    quantity: number,
    db: DbExecutor = this.db,
  ): Promise<void> {
    if (quantity <= 0) throw new DomainError(DomainErrorCode.IdempotencyConflict, 'Consume quantity must be positive');
    const item = await db.itemDefinition.findUniqueOrThrow({ where: { key: itemKey } });
    const updated = await db.inventoryStack.updateMany({
      where: { ownerUid, itemId: item.id, quantity: { gte: quantity } },
      data: { quantity: { decrement: quantity } },
    });
    if (updated.count !== 1) {
      throw new DomainError(
        DomainErrorCode.InsufficientStock,
        `Not enough ${itemKey} in inventory`,
        { itemKey, requested: quantity },
      );
    }
    // Drop empty stacks to keep the table sparse.
    await db.inventoryStack.deleteMany({ where: { ownerUid, itemId: item.id, quantity: { lte: 0 } } });
  }

  /** Used by seed scripts; idempotent because of the composite unique key. */
  async upsertStack(ownerUid: string, itemKey: string, quantity: number, db: DbExecutor = this.db): Promise<void> {
    const item = await db.itemDefinition.findUniqueOrThrow({ where: { key: itemKey } });
    try {
      await db.inventoryStack.upsert({
        where: { ownerUid_itemId: { ownerUid, itemId: item.id } },
        create: { ownerUid, itemId: item.id, quantity },
        update: { quantity },
      });
    } catch (err) {
      if (!isUniqueViolation(err)) throw err;
      throw new DomainError(DomainErrorCode.IdempotencyConflict, 'Concurrent stack mutation');
    }
  }
}
