/**
 * Idempotent development seed.
 *
 * Usage: `npm run prisma:seed` (requires DATABASE_URL).
 *
 * All content is original placeholder art/naming — no Sega IP.
 * Safe to re-run: item and competition definitions are upserted by key, and
 * demo players are keyed off a fixed idempotency ledger namespace.
 */
import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();

/** Original placeholder species — intentionally whimsical, non-infringing. */
const SPECIES = ['species.balloon', 'species.ember', 'species.tide', 'species.moss'];

const ITEMS = [
  { key: 'food.berry', kind: 'FOOD', name: 'Sun Berry', price: 10, data: { nutrition: 15, mood: 5 } },
  { key: 'food.porridge', kind: 'FOOD', name: 'Cloud Porridge', price: 20, data: { nutrition: 30, mood: 3 } },
  { key: 'food.honeyfig', kind: 'FOOD', name: 'Honey Fig', price: 15, data: { nutrition: 20, mood: 8 } },
  { key: 'training.dumbbell', kind: 'TRAINING', name: 'Pebble Dumbbell', price: 40, data: { stat: 'power', gain: 2 } },
  { key: 'training.flute', kind: 'TRAINING', name: 'Wind Flute', price: 40, data: { stat: 'fly', gain: 2 } },
  { key: 'training.paddle', kind: 'TRAINING', name: 'Coral Paddle', price: 40, data: { stat: 'swim', gain: 2 } },
  { key: 'decoration.lantern', kind: 'DECORATION', name: 'Paper Lantern', price: 60, data: { mesh: 'lantern', light: 0.6 } },
  { key: 'decoration.fountain', kind: 'DECORATION', name: 'Bubble Fountain', price: 120, data: { mesh: 'fountain', light: 0.0 } },
] as const;

const DEMO_USERS = [
  { email: 'alice@example.dev', displayName: 'Alice' },
  { email: 'bob@example.dev', displayName: 'Bob' },
];

async function main() {
  // --- item definitions ----------------------------------------------------
  for (const item of ITEMS) {
    await prisma.itemDefinition.upsert({
      where: { key: item.key },
      create: {
        key: item.key,
        kind: item.kind,
        name: item.name,
        price: item.price,
        data: item.data,
        description: `Development placeholder ${item.kind.toLowerCase()} item.`,
      },
      update: { name: item.name, price: item.price, data: item.data, kind: item.kind },
    });
  }

  // --- weekly competition shell --------------------------------------------
  const weekStart = new Date();
  weekStart.setUTCHours(0, 0, 0, 0);
  const weekEnd = new Date(weekStart.getTime() + 7 * 24 * 3600 * 1000);
  await prisma.competition.upsert({
    where: { key: 'karate.dev-week' },
    create: {
      key: 'karate.dev-week',
      name: 'Dev Dojo Scrimmage',
      kind: 'karate',
      startsAt: weekStart,
      endsAt: weekEnd,
      prize: 500,
    },
    update: { startsAt: weekStart, endsAt: weekEnd, prize: 500 },
  });

  // --- demo players ---------------------------------------------------------
  for (const [i, u] of DEMO_USERS.entries()) {
    const user = await prisma.user.upsert({
      where: { email: u.email },
      create: { email: u.email, displayName: u.displayName, passwordHash: null },
      update: { displayName: u.displayName },
    });

    const profile = await prisma.profile.upsert({
      where: { userId: user.id },
      create: { userId: user.id, rings: 1000 },
      update: {},
    });

    const garden = await prisma.garden.upsert({
      where: { ownerUid_name: { ownerUid: user.id, name: `${u.displayName}'s Meadow` } },
      create: { ownerUid: user.id, name: `${u.displayName}'s Meadow`, theme: 'neutral' },
      update: {},
    });

    await prisma.creature.upsert({
      where: { id: `00000000-0000-4000-8000-00000000000${i + 1}` },
      create: {
        id: `00000000-0000-4000-8000-00000000000${i + 1}`,
        ownerUid: user.id,
        gardenUid: garden.id,
        name: i === 0 ? 'Puff' : 'Sprig',
        speciesKey: SPECIES[i % SPECIES.length],
        stats: { swim: 8, fly: 6, run: 7, power: 9, stamina: 5 },
        traits: [{ key: 'curious' }, { key: 'hardy' }],
      },
      update: {},
    });

    // Starter inventory, seeded through the ledger for auditability.
    const berry = await prisma.itemDefinition.findUniqueOrThrow({ where: { key: 'food.berry' } });
    await prisma.inventoryStack.upsert({
      where: { ownerUid_itemId: { ownerUid: user.id, itemId: berry.id } },
      create: { ownerUid: user.id, itemId: berry.id, quantity: 20 },
      update: { quantity: 20 },
    });

    await prisma.ringLedger.upsert({
      where: {
        idempotencyKey: `seed.starter-grant.${user.id}`,
      },
      create: {
        ownerUid: user.id,
        delta: 1000,
        reason: 'seed.starter-grant',
        idempotencyKey: `seed.starter-grant.${user.id}`,
      },
      update: {},
    });
  }

  console.log('Seed complete (idempotent).');
}

main()
  .catch((e) => {
    console.error(e);
    process.exit(1);
  })
  .finally(() => prisma.$disconnect());
