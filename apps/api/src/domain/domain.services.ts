import { Injectable } from '@nestjs/common';
import type { Prisma } from '@prisma/client';
import { PrismaService } from '../database/prisma.service';
import { DomainError, DomainErrorCode } from './domain.errors';
import type { DbExecutor, CreatureStats } from './repositories/types';
import { ProfileRepository } from './repositories/profile.repository';
import { InventoryRepository } from './repositories/inventory.repository';
import { CreatureRepository } from './repositories/creature.repository';
import { GardenRepository } from './repositories/garden.repository';
import { EconomyRepository } from './repositories/economy.repository';
import { SocialRepository } from './repositories/social.repository';
import { CompetitionRepository } from './repositories/competition.repository';

/**
 * Facade exposing repositories plus transactional use-cases. Domain logic
 * depends on this class, never on Prisma directly.
 */
@Injectable()
export class DomainServices {
  readonly profiles: ProfileRepository;
  readonly inventory: InventoryRepository;
  readonly creatures: CreatureRepository;
  readonly gardens: GardenRepository;
  readonly economy: EconomyRepository;
  readonly social: SocialRepository;
  readonly competitions: CompetitionRepository;

  constructor(private readonly prisma: PrismaService) {
    this.profiles = new ProfileRepository(prisma);
    this.inventory = new InventoryRepository(prisma);
    this.creatures = new CreatureRepository(prisma);
    this.gardens = new GardenRepository(prisma);
    this.economy = new EconomyRepository(prisma);
    this.social = new SocialRepository(prisma);
    this.competitions = new CompetitionRepository(prisma);
  }

  /**
   * Runs `fn` inside a serializable-leaning interactive transaction. Nested
   * repositories receive the transaction client so every statement in `fn`
   * commits or rolls back atomically.
   */
  runInTransaction<T>(fn: (tx: DbExecutor) => Promise<T>, options?: Prisma.TransactionOptions): Promise<T> {
    return this.prisma.$transaction(async (tx: Prisma.TransactionClient) => fn(tx), options) as Promise<T>;
  }

  /** Purchase flow: debit rings and grant the item in one atomic unit. */
  async purchaseItem(input: { ownerUid: string; itemKey: string; idempotencyKey: string }) {
    return this.runInTransaction(async (tx) => {
      const item = await tx.itemDefinition.findUnique({ where: { key: input.itemKey } });
      if (!item) throw new DomainError(DomainErrorCode.NotFound, 'Unknown item');
      if (item.price === null) {
        throw new DomainError(DomainErrorCode.IdempotencyConflict, 'Item is not purchasable');
      }
      const spent = await this.economy.spend(
        { ownerUid: input.ownerUid, amount: item.price, reason: 'purchase.item', idempotencyKey: input.idempotencyKey, meta: { itemKey: item.key } },
        tx,
      );
      if (spent === 'duplicate') return 'duplicate' as const;
      await this.inventory.grant(input.ownerUid, input.itemKey, 1, tx);
      await this.economy.audit(input.ownerUid, 'economy.purchase', input.ownerUid, { itemKey: item.key, price: item.price }, tx);
      return 'applied' as const;
    });
  }

  /** Feeding flow: consume food, nudge mood/hunger, atomic. */
  async feedCreature(input: { ownerUid: string; creatureUid: string; foodKey: string; idempotencyKey: string }) {
    return this.runInTransaction(async (tx) => {
      const creature = await tx.creature.findUnique({ where: { id: input.creatureUid } });
      if (!creature || creature.retiredAt) {
        throw new DomainError(DomainErrorCode.NotFound, 'Creature not found');
      }
      if (creature.ownerUid !== input.ownerUid) {
        throw new DomainError(DomainErrorCode.NotFound, 'Not your creature');
      }
      const item = await tx.itemDefinition.findUnique({ where: { key: input.foodKey } });
      if (!item || item.kind !== 'FOOD') {
        throw new DomainError(DomainErrorCode.IdempotencyConflict, 'Not a food item');
      }
      // Guard the consume with a consumption marker so re-runs don't double-feed.
      const marker = await tx.auditLog.create({
        data: {
          userId: input.ownerUid,
          action: 'creature.feed',
          targetUid: input.creatureUid,
          meta: { idempotencyKey: input.idempotencyKey, foodKey: input.foodKey },
        },
      });
      const prior = await tx.auditLog.count({
        where: {
          userId: input.ownerUid,
          action: 'creature.feed',
          meta: { path: ['idempotencyKey'], equals: input.idempotencyKey },
        },
      });
      if (prior > 1) {
        await tx.auditLog.delete({ where: { id: marker.id } });
        return 'duplicate' as const;
      }
      await this.inventory.consume(input.ownerUid, input.foodKey, 1, tx);
      const data = item.data as { nutrition?: number; mood?: number };
      await tx.creature.update({
        where: { id: input.creatureUid },
        data: {
          hunger: { decrement: data.nutrition ?? 0 },
          mood: { increment: data.mood ?? 0 },
          experience: { increment: Math.ceil((data.nutrition ?? 0) / 2) },
        },
      });
      return 'applied' as const;
    });
  }

  /** Training flow: consume a training item, apply stat gain. */
  async trainCreature(input: {
    ownerUid: string;
    creatureUid: string;
    trainingItemKey: string;
    idempotencyKey: string;
  }) {
    return this.runInTransaction(async (tx) => {
      const creature = await tx.creature.findUnique({ where: { id: input.creatureUid } });
      if (!creature || creature.retiredAt || creature.ownerUid !== input.ownerUid) {
        throw new DomainError(DomainErrorCode.NotFound, 'Creature not found');
      }
      const item = await tx.itemDefinition.findUnique({ where: { key: input.trainingItemKey } });
      if (!item || item.kind !== 'TRAINING') {
        throw new DomainError(DomainErrorCode.IdempotencyConflict, 'Not a training item');
      }
      const applied = await this.inventory.consume(input.ownerUid, input.trainingItemKey, 1, tx).then(
        () => 'applied' as const,
        () => 'insufficient' as const,
      );
      if (applied === 'insufficient') {
        throw new DomainError(DomainErrorCode.InsufficientStock, 'No training item in stock');
      }
      const payload = item.data as { stat?: keyof CreatureStats; gain?: number };
      await this.creatures.applyTraining(
        input.creatureUid,
        { stat: payload.stat ?? 'power', gain: payload.gain ?? 1, experience: 10, mood: 2 },
        tx,
      );
      await this.economy.audit(
        input.ownerUid,
        'creature.train',
        input.creatureUid,
        { idempotencyKey: input.idempotencyKey, itemKey: item.key },
        tx,
      );
      return 'applied' as const;
    });
  }

  /** Garden placement flow: decrement inventory + occupy tile atomically. */
  async placeDecoration(input: {
    ownerUid: string;
    gardenUid: string;
    itemKey: string;
    x: number;
    y: number;
    idempotencyKey: string;
  }) {
    return this.runInTransaction(async (tx) => {
      const garden = await tx.garden.findUnique({ where: { id: input.gardenUid } });
      if (!garden || garden.deletedAt || garden.ownerUid !== input.ownerUid) {
        throw new DomainError(DomainErrorCode.NotFound, 'Garden not found');
      }
      await this.inventory.consume(input.ownerUid, input.itemKey, 1, tx);
      await this.gardens.place(
        { gardenUid: input.gardenUid, ownerUid: input.ownerUid, itemKey: input.itemKey, x: input.x, y: input.y },
        tx,
      );
      await this.economy.audit(
        input.ownerUid,
        'garden.place',
        input.gardenUid,
        { idempotencyKey: input.idempotencyKey, itemKey: input.itemKey, x: input.x, y: input.y },
        tx,
      );
      return 'applied' as const;
    });
  }

  /** Lifting a decoration returns it to inventory atomically. */
  async liftDecoration(input: { ownerUid: string; gardenUid: string; x: number; y: number }) {
    return this.runInTransaction(async (tx) => {
      const itemKey = await this.gardens.remove(input.gardenUid, input.x, input.y, tx);
      if (!itemKey) throw new DomainError(DomainErrorCode.NotFound, 'Nothing placed on that tile');
      await this.inventory.grant(input.ownerUid, itemKey, 1, tx);
      await this.economy.audit(input.ownerUid, 'garden.lift', input.gardenUid, { itemKey, x: input.x, y: input.y }, tx);
      return 'applied' as const;
    });
  }

  /** Daily visit reward: at most once per visitor/garden/day, atomic. */
  async recordVisitWithReward(input: { gardenUid: string; visitorUid: string; reward: number }) {
    return this.runInTransaction(async (tx) => {
      const result = await this.social.recordVisit(input.gardenUid, input.visitorUid, tx);
      if (result === 'duplicate') return 'duplicate' as const;
      await this.economy.award(
        {
          ownerUid: input.visitorUid,
          delta: input.reward,
          reason: 'reward.visit',
          idempotencyKey: `visit:${input.gardenUid}:${input.visitorUid}:${new Date().toISOString().slice(0, 10)}`,
        },
        tx,
      );
      return 'created' as const;
    });
  }

  /** Enters a competition and charges the entry fee in one transaction. */
  async enterCompetition(input: {
    ownerUid: string;
    competitionUid: string;
    creatureUid: string;
    entryFee: number;
  }) {
    return this.runInTransaction(async (tx) => {
      const idempotencyKey = `entry:${input.competitionUid}:${input.creatureUid}`;
      if (input.entryFee > 0) {
        const spent = await this.economy.spend(
          { ownerUid: input.ownerUid, amount: input.entryFee, reason: 'competition.entry-fee', idempotencyKey },
          tx,
        );
        if (spent === 'duplicate') return 'duplicate' as const;
      }
      const entered = await this.competitions.enter(
        { competitionUid: input.competitionUid, creatureUid: input.creatureUid, idempotencyKey },
        tx,
      );
      if (entered === 'duplicate') return 'duplicate' as const;
      return 'applied' as const;
    });
  }

  /** Finalises a competition and pays the prize through the ledger. */
  async finaliseCompetition(input: { competitionUid: string }) {
    return this.runInTransaction(async (tx) => {
      const winner = await this.competitions.finalise(input.competitionUid, tx);
      const competition = await tx.competition.findUniqueOrThrow({ where: { id: input.competitionUid } });
      if (!winner || competition.prize === 0) return 'no-winner' as const;
      const winnerProfile = await tx.creature.findUniqueOrThrow({ where: { id: winner.creatureUid } });
      await this.economy.award(
        {
          ownerUid: winnerProfile.ownerUid,
          delta: competition.prize,
          reason: 'reward.competition',
          idempotencyKey: `prize:${input.competitionUid}`,
          meta: { competitionKey: competition.key },
        },
        tx,
      );
      return 'applied' as const;
    });
  }
}
