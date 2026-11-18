import { Inject, Injectable } from '@nestjs/common';
import Redis from 'ioredis';
import { REDIS_CLIENT } from '../auth/throttle/throttle.service';
import { loadConfig } from '../config/configuration';

/** Creates the shared Redis client used for throttling and (optionally) pub/sub. */
export function createRedisClient(url = loadConfig().redisUrl): Redis {
  const client = new Redis(url, {
    maxRetriesPerRequest: 2,
    lazyConnect: false,
  });
  // Swallow idle errors; Nest handles reconnects.
  client.on('error', () => undefined);
  return client;
}

export const REDIS_PROVIDER = {
  provide: REDIS_CLIENT,
  useFactory: createRedisClient,
};
