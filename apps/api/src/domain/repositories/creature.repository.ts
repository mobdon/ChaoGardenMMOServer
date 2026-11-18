import type { DbExecutor, CreatureView, CreatureStats } from './types';
import type { Creature } from '@prisma/client';

/** Creature reads and lifecycle transitions. No currency/item logic here. */
export class CreatureRepository {
  constructor(private readonly db: DbExecutor) {}

  private static toView(c: Creature): CreatureView {
    return {
      id: c.id,
      ownerUid: c.ownerUid,
      gardenUid: c.gardenUid,
      name: c.name,
      speciesKey: c.speciesKey,
      alignment: c.alignment,
      stats: c.stats as CreatureStats,
      traits: c.traits as Array<Record<string, unknown>>,
      level: c.level,
      experience: c.experience,
      mood: c.mood,
      hunger: c.hunger,
      retiredAt: c.retiredAt,
    };
  }

  async listByOwner(ownerUid: string, page = 0, pageSize = 50): Promise<CreatureView[]> {
    const rows = await this.db.creature.findMany({
      where: { ownerUid, retiredAt: null },
      orderBy: { createdAt: 'asc' },
      skip: page * pageSize,
      take: pageSize,
    });
    return rows.map(CreatureRepository.toView);
  }

  async findById(id: string): Promise<CreatureView | null> {
    const c = await this.db.creature.findUnique({ where: { id } });
    return c ? CreatureRepository.toView(c) : null;
  }

  async create(
    input: {
      ownerUid: string;
      gardenUid?: string | null;
      name: string;
      speciesKey: string;
      alignment?: 'HERO' | 'NEUTRAL' | 'DARK';
      stats?: CreatureStats;
      traits?: Array<Record<string, unknown>>;
    },
    db: DbExecutor = this.db,
  ): Promise<CreatureView> {
    const c = await db.creature.create({
      data: {
        ownerUid: input.ownerUid,
        gardenUid: input.gardenUid ?? null,
        name: input.name,
        speciesKey: input.speciesKey,
        alignment: input.alignment ?? 'NEUTRAL',
        stats: input.stats ?? { swim: 5, fly: 5, run: 5, power: 5, stamina: 5 },
        traits: input.traits ?? [],
      },
    });
    return CreatureRepository.toView(c);
  }

  /** Applies a training result. Guarded so retired creatures cannot train. */
  async applyTraining(
    id: string,
    delta: { stat: keyof CreatureStats; gain: number; experience?: number; mood?: number },
    db: DbExecutor = this.db,
  ): Promise<CreatureView | null> {
    const c = await db.creature.findUnique({ where: { id } });
    if (!c || c.retiredAt) return null;
    const stats = { ...(c.stats as CreatureStats) };
    stats[delta.stat] += delta.gain;
    const updated = await db.creature.update({
      where: { id },
      data: {
        stats,
        experience: { increment: delta.experience ?? 0 },
        mood: delta.mood !== undefined ? { increment: delta.mood } : undefined,
      },
    });
    return CreatureRepository.toView(updated);
  }

  /** Soft deletion: creatures are retired, never destroyed. */
  async retire(id: string, db: DbExecutor = this.db): Promise<void> {
    await db.creature.update({ where: { id }, data: { retiredAt: new Date() } });
  }
}
