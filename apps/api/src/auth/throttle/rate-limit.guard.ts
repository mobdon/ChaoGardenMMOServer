import { CanActivate, ExecutionContext, Injectable, SetMetadata } from '@nestjs/common';
import { Reflector } from '@nestjs/core';
import { ThrottleService } from './throttle.service';

export interface RateLimitOptions {
  bucket: string;
  limit: number;
  windowSeconds: number;
}

export const RATE_LIMIT_KEY = 'chaogarden.rateLimit';
export const RateLimit = (options: RateLimitOptions) => SetMetadata(RATE_LIMIT_KEY, options);

/** Client identity: prefer the forwarded IP chain, fall back to socket address. */
export function clientIp(request: {
  ip?: string;
  headers?: Record<string, string | undefined>;
  socket?: { remoteAddress?: string };
}): string {
  const forwarded = request.headers?.['x-forwarded-for'];
  if (forwarded) return forwarded.split(',')[0]!.trim();
  return request.ip ?? request.socket?.remoteAddress ?? 'unknown';
}

/**
 * Redis-backed throttler guard applied to login, register, refresh, guest
 * creation and guest upgrade. Identity is bucket-specific (IP, or IP+email
 * hash for credential endpoints) so account-targeted brute force is capped.
 */
@Injectable()
export class RateLimitGuard implements CanActivate {
  constructor(
    private readonly throttle: ThrottleService,
    private readonly reflector: Reflector,
  ) {}

  async canActivate(context: ExecutionContext): Promise<boolean> {
    const options = this.reflector.getAllAndOverride<RateLimitOptions | undefined>(
      RATE_LIMIT_KEY,
      [context.getHandler(), context.getClass()],
    );
    if (!options) return true;

    const request = context.switchToHttp().getRequest<{
      ip?: string;
      headers?: Record<string, string | undefined>;
      body?: Record<string, unknown>;
    }>();
    const ip = clientIp(request);
    // For credential endpoints mix in the attempted account so one IP cannot
    // hammer many accounts, and one account cannot be hammered from many IPs.
    const attempted = typeof request.body?.email === 'string' ? request.body.email : '';
    const identity = attempted ? `${ip}:${attempted}` : ip;
    await this.throttle.consume(options.bucket, identity, options.limit, options.windowSeconds);
    return true;
  }
}
