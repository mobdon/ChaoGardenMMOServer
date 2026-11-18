import { Inject, Injectable, OnModuleDestroy } from '@nestjs/common';
import Redis from 'ioredis';
import { AuthErrorCode } from '@chaogarden/contracts';
import { AuthException } from '../auth/errors';

export const REDIS_CLIENT = 'REDIS_CLIENT';

/**
 * Redis-backed fixed-window throttling for sensitive auth endpoints.
 * Keys include the route bucket plus the client identity (IP and, where
 * available, attempted account id) so an attacker cannot reset a window by
 * rotating identities.
 */
@Injectable()
export class ThrottleService implements OnModuleDestroy {
  constructor(@Inject(REDIS_CLIENT) private readonly redis: Redis) {}

  async onModuleDestroy() {
    void this.redis.quit();
  }

  /**
   * Consumes one attempt from the bucket. Returns remaining attempts; throws
   * `AUTH_RATE_LIMITED` when exhausted.
   */
  async consume(
    bucket: string,
    identity: string,
    limit: number,
    windowSeconds: number,
  ): Promise<number> {
    const key = `throttle:${bucket}:${identity}`;
    const count = await this.redis.incr(key);
    if (count === 1) {
      await this.redis.expire(key, windowSeconds);
    }
    if (count > limit) {
      const ttl = await this.redis.ttl(key);
      throw new AuthException(
        AuthErrorCode.RateLimited,
        'Too many attempts, try again later',
        ttl > 0 ? ttl : windowSeconds,
      );
    }
    return limit - count;
  }

  /** Clears a bucket (e.g. after a successful login). */
  async reset(bucket: string, identity: string): Promise<void> {
    await this.redis.del(`throttle:${bucket}:${identity}`);
  }
}
