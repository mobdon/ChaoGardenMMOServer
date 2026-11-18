import { REALTIME_LIMITS } from '@chaogarden/contracts';

/** Runtime bounds for the realtime layer; every value can be tuned per env. */
export class RealtimeGatewayConfig {
  readonly maxPayloadBytes = REALTIME_LIMITS.maxPayloadBytes;
  readonly maxGardensPerSocket = REALTIME_LIMITS.maxGardensPerSocket;
  readonly maxGardenOccupants = REALTIME_LIMITS.maxGardenOccupants;
  readonly commandsPerWindow = REALTIME_LIMITS.commandsPerWindow;
  readonly commandWindowMs = REALTIME_LIMITS.commandWindowMs;
  readonly reactionsPerWindow = REALTIME_LIMITS.reactionsPerWindow;
  readonly reactionWindowMs = REALTIME_LIMITS.reactionWindowMs;
  readonly maxRecentEventsRetained = REALTIME_LIMITS.maxRecentEventsRetained;
  readonly redisUrl = process.env.REDIS_URL ?? 'redis://127.0.0.1:6379';
}
