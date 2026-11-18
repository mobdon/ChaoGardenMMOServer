/**
 * Domain layer — repositories and transactional services.
 *
 * Repositories expose domain-shaped types and accept a `DbExecutor` so the
 * same code path runs standalone or inside a Prisma `$transaction`. Prisma
 * details (models, delegation tables) stay behind this boundary; domain logic
 * and controllers depend only on the repository interfaces.
 */
export * from './domain.errors';
export * from './repositories/types';
export * from './repositories/profile.repository';
export * from './repositories/inventory.repository';
export * from './repositories/creature.repository';
export * from './repositories/garden.repository';
export * from './repositories/economy.repository';
export * from './repositories/social.repository';
export * from './repositories/competition.repository';
export * from './domain.services';
export * from './domain.module';
