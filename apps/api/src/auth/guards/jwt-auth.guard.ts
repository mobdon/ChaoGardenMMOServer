/**
 * NestJS guard for both REST and WebSocket contexts.
 *
 * Verifies the short-lived access token, confirms the backing device session
 * is still live, and attaches an `AuthenticatedActor` to the execution
 * context. Failures throw `AuthException`s carrying stable machine-readable
 * codes (`AUTH_UNAUTHENTICATED`, `AUTH_EXPIRED_TOKEN`, `AUTH_SESSION_REVOKED`).
 */
import { CanActivate, ExecutionContext, Injectable } from '@nestjs/common';
import { Reflector } from '@nestjs/core';
import { Observable } from 'rxjs';
import { AuthErrorCode } from '@chaogarden/contracts';
import type { AuthenticatedActor } from '../auth.service';
import { AuthService } from '../auth.service';
import { TokenService } from '../token.service';
import { AuthException } from '../errors';
import { IS_PUBLIC_KEY } from '../decorators/public.decorator';

export const ACTOR_KEY = 'chaogarden.actor';

export function extractBearer(header: string | undefined | null): string | null {
  if (!header || !header.startsWith('Bearer ')) return null;
  const token = header.slice('Bearer '.length).trim();
  return token || null;
}

/** REST-only helper: read the actor attached by the guard. */
export function getActor(request: Record<string, unknown>): AuthenticatedActor {
  return request[ACTOR_KEY] as AuthenticatedActor;
}

@Injectable()
export class JwtAuthGuard implements CanActivate {
  constructor(
    private readonly tokens: TokenService,
    private readonly auth: AuthService,
    private readonly reflector: Reflector,
  ) {}

  canActivate(context: ExecutionContext): boolean | Promise<boolean> | Observable<boolean> {
    if (
      this.reflector.getAllAndOverride<boolean>(IS_PUBLIC_KEY, [
        context.getHandler(),
        context.getClass(),
      ])
    ) {
      return true;
    }
    const promise = this.authorize(context);
    return promise;
  }

  private async authorize(context: ExecutionContext): Promise<boolean> {
    const rawToken = context.getType() === 'ws' ? this.wsToken(context) : this.httpToken(context);
    if (!rawToken) {
      throw new AuthException(AuthErrorCode.Unauthenticated, 'Authentication required');
    }

    const claims = await this.tokens.verifyAccessToken(rawToken);
    // Defense in depth: a revoked device session invalidates live access tokens.
    await this.auth.assertSessionLive(claims.sub, claims.sid);

    const actor: AuthenticatedActor = {
      userId: claims.sub,
      sessionId: claims.sid,
      guest: claims.guest,
    };

    if (context.getType() === 'ws') {
      const data = context.switchToWs().getData() as Record<string, unknown>;
      data[ACTOR_KEY] = actor;
    } else {
      const request = context.switchToHttp().getRequest() as Record<string, unknown>;
      request[ACTOR_KEY] = actor;
    }
    return true;
  }

  /** WebSocket token from the handshake (`auth.token` or Authorization header). */
  private wsToken(context: ExecutionContext): string | null {
    const client = context.switchToWs().getClient<{
      handshake?: { auth?: { token?: string }; headers?: { authorization?: string } };
    }>();
    const fromAuth = client?.handshake?.auth?.token ?? null;
    if (fromAuth) return fromAuth;
    return extractBearer(client?.handshake?.headers?.authorization);
  }

  private httpToken(context: ExecutionContext): string | null {
    const request = context.switchToHttp().getRequest<{ headers?: Record<string, string | undefined> }>();
    return extractBearer(request?.headers?.authorization);
  }
}
