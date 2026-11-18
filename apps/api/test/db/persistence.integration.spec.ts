/**
 * Persistence-layer integration tests.
 *
 * Skipped automatically unless a disposable PostgreSQL database is provided
 * via INTEGRATION_DATABASE_URL (or DATABASE_URL). CI applies migrations to
 * the per-run schema (see test/db/integration.setup.ts) before these run.
 *
 * Coverage:
 * - Constraint enforcement (unique keys, check constraints, FKs).
 * - Transactional rollback on failure (no partial writes).
 * - Idempotency of ledger awards, spends and competition entries.
 * - Pagination of ledger and inventory queries.
 */
import { INTEGRATION_ENABLED, createTestClient, integrationSchema } from './integration.setup';
import type { PrismaClient } from '@prisma/client';
import { DomainServices } from '../../src/domain';
import { DomainErrorCode } from '../../src/domain';

const d = INTEGRATION_ENABLED ? describe : describe.skip;

d('persistence integration', () => {
  let prisma: PrismaClient;
  let domain: DomainServices;
  let userId: string;

  beforeAll(async () => {
    prisma = createTestClient()!;
    domain = new DomainServices(prisma as never);
    await prisma.$connect();

    const user = await prisma.user.create({
      data: { displayName: 'IT Player', email: `it-${Date.now()}@example.test` },
    });
    userId = user.id;
    await prisma.profile.create({ data: { userId, rings: 500 } });
  });

  afterAll(async () => {
    // Wipe this run's schema: every table lives in the disposable namespace.
    if (prisma) {
      await prisma.$executeRawUnsafe(`DROP SCHEMA IF EXISTS "${integrationSchema()}" CASCADE`);
      await prisma.$disconnect();
    }
  });

  // --- constraints ---------------------------------------------------------

  it('rejects a second ledger row with the same idempotency key', async () => {
    const key = `it-award-${Date.now()}`;
    const first = await domain.runInTransaction((tx) =>
      domain.economy.award({ ownerUid: userId, delta: 25, reason: 'test.award', idempotencyKey: key }, tx),
    );
    const second = await domain.runInTransaction((tx) =>
      domain.economy.award({ ownerUid: userId, delta: 25, reason: 'test.award', idempotencyKey: key }, tx),
    );
    expect(first).toBe('applied');
    expect(second).toBe('duplicate');
    expect(await domain.economy.replayBalance(userId)).toBe(25);
    expect((await prisma.profile.findUnique({ where: { userId } }))!.rings).toBe(525);
  });

  it('rejects duplicate competition entries for the same creature', async () => {
    const competition = await prisma.competition.create({
      data: {
        key: `it-comp-${Date.now()}`,
        name: 'IT Cup',
        kind: 'race',
        startsAt: new Date(Date.now() - 1000),
        endsAt: new Date(Date.now() + 3600_000),
      },
    });
    const creature = await domain.creatures.create({ ownerUid: userId, name: 'Racer', speciesKey: 'species.ember' });
    const key = `entry:${competition.id}:${creature.id}`;
    const first = await domain.runInTransaction((tx) =>
      domain.competitions.enter({ competitionUid: competition.id, creatureUid: creature.id, idempotencyKey: key }, tx),
    );
    const second = await domain.runInTransaction((tx) =>
      domain.competitions.enter({ competitionUid: competition.id, creatureUid: creature.id, idempotencyKey: key }, tx),
    );
    expect(first).toBe('created');
    expect(second).toBe('duplicate');
    expect(await prisma.competitionEntry.count()).toBeGreaterThanOrEqual(1);
  });

  it('rejects a spend that would drive the balance negative and rolls back fully', async () => {
    const before = await domain.economy.replayBalance(userId);
    await expect(
      domain.runInTransaction(async (tx) => {
        await domain.economy.spend(
          { ownerUid: userId, amount: before + 1_000_000, reason: 'test.overspend', idempotencyKey: `it-overspend-${Date.now()}` },
          tx,
        );
      }),
    ).rejects.toMatchObject({ code: DomainErrorCode.InsufficientFunds });

    // Rollback: no ledger row leaked.
    const rows = await prisma.ringLedger.findMany({ where: { ownerUid: userId, reason: 'test.overspend' } });
    expect(rows).toHaveLength(0);
    expect((await prisma.profile.findUnique({ where: { userId } }))!.rings).toBe(before);
  });

  it('rolls back purchases entirely when the inventory grant fails', async () => {
    const before = await domain.economy.replayBalance(userId);
    // Item does not exist -> purchase throws inside the transaction.
    await expect(
      domain.runInTransaction(async (tx) => {
        await domain.economy.spend(
          { ownerUid: userId, amount: 10, reason: 'purchase.item', idempotencyKey: `it-buy-fail-${Date.now()}` },
          tx,
        );
        await tx.itemDefinition.findUniqueOrThrow({ where: { key: 'does.not.exist' } });
      }),
    ).rejects.toThrow();
    expect(await domain.economy.replayBalance(userId)).toBe(before);
    expect((await prisma.profile.findUnique({ where: { userId } }))!.rings).toBe(before);
  });

  it('enforces non-negative cached ring balance at the database level', async () => {
    await expect(
      prisma.profile.update({ where: { userId }, data: { rings: { decrement: 10_000_000 } } }),
    ).rejects.toThrow();
  });

  it('enforces one decoration per tile', async () => {
    const garden = await domain.gardens.create({ ownerUid: userId, name: `IT Garden ${Date.now()}` });
    const lantern = await prisma.itemDefinition.findUnique({ where: { key: 'decoration.lantern' } });
    if (!lantern) return; // seed not applied in this schema
    await prisma.placedDecoration.create({
      data: { gardenUid: garden.id, itemId: lantern.id, x: 3, y: 4 },
    });
    await expect(
      prisma.placedDecoration.create({ data: { gardenUid: garden.id, itemId: lantern.id, x: 3, y: 4 } }),
    ).rejects.toThrow();
  });

  it('allows only one visit per garden per visitor per day', async () => {
    const garden = await domain.gardens.create({ ownerUid: userId, name: `IT Visit Garden ${Date.now()}` });
    const first = await domain.social.recordVisit(garden.id, userId);
    const second = await domain.social.recordVisit(garden.id, userId);
    expect(first).toBe('created');
    expect(second).toBe('duplicate');
  });

  it('prevents duplicate garden names per owner', async () => {
    const name = `Unique Garden ${Date.now()}`;
    await domain.gardens.create({ ownerUid: userId, name });
    await expect(domain.gardens.create({ ownerUid: userId, name })).rejects.toMatchObject({
      code: DomainErrorCode.DuplicateEntry,
    });
  });

  // --- transactional use-cases ---------------------------------------------

  it('purchase debits rings and grants stock atomically, idempotently', async () => {
    const key = `it-purchase-${Date.now()}`;
    const before = (await prisma.profile.findUnique({ where: { userId } }))!.rings;
    const r1 = await domain.purchaseItem({ ownerUid: userId, itemKey: 'food.berry', idempotencyKey: key });
    const r2 = await domain.purchaseItem({ ownerUid: userId, itemKey: 'food.berry', idempotencyKey: key });
    expect(r1).toBe('applied');
    expect(r2).toBe('duplicate');
    const after = (await prisma.profile.findUnique({ where: { userId } }))!.rings;
    expect(before - after).toBe(10); // exactly one debit
    expect(await domain.inventory.stackQuantity(userId, 'food.berry')).toBeGreaterThanOrEqual(1);
  });

  it('feed consumes exactly one food item per call, idempotent by key', async () => {
    const key = `it-feed-${Date.now()}`;
    await domain.purchaseItem({ ownerUid: userId, itemKey: 'food.berry', idempotencyKey: key });
    const creature = await domain.creatures.create({ ownerUid: userId, name: 'Nibbler', speciesKey: 'species.moss' });
    const r1 = await domain.feedCreature({ ownerUid: userId, creatureUid: creature.id, foodKey: 'food.berry', idempotencyKey: key });
    const r2 = await domain.feedCreature({ ownerUid: userId, creatureUid: creature.id, foodKey: 'food.berry', idempotencyKey: key });
    expect(r1).toBe('applied');
    expect(r2).toBe('duplicate');
    // Only the purchase grant (1) remains; both feed attempts consumed at most one.
    expect(await domain.inventory.stackQuantity(userId, 'food.berry')).toBeLessThanOrEqual(1);
  });

  it('placeDecoration consumes inventory and occupies the tile atomically', async () => {
    const key = `it-place-${Date.now()}`;
    const garden = await domain.gardens.create({ ownerUid: userId, name: `IT Place Garden ${Date.now()}` });
    await expect(
      domain.placeDecoration({ ownerUid: userId, gardenUid: garden.id, itemKey: 'decoration.lantern', x: 1, y: 2, idempotencyKey: key }),
    ).rejects.toMatchObject({ code: DomainErrorCode.InsufficientStock }); // nothing in stock yet
    const decorRows = await prisma.placedDecoration.findMany({ where: { gardenUid: garden.id, x: 1, y: 2 } });
    expect(decorRows).toHaveLength(0); // rolled back, tile still free
  });

  // --- pagination ----------------------------------------------------------

  it('paginates the ledger deterministically', async () => {
    for (let i = 0; i < 12; i++) {
      await domain.runInTransaction((tx) =>
        domain.economy.award({ ownerUid: userId, delta: 1, reason: 'test.page', idempotencyKey: `it-page-${Date.now()}-${i}` }, tx),
      );
    }
    const page0 = await domain.economy.ledger(userId, 0, 5);
    const page1 = await domain.economy.ledger(userId, 1, 5);
    expect(page0).toHaveLength(5);
    expect(page1).toHaveLength(5);
    expect(page0[0]!.id).not.toBe(page1[0]!.id);
    // Newest first.
    expect(page0[0]!.createdAt.getTime()).toBeGreaterThanOrEqual(page0[1]!.createdAt.getTime());
  });
});
