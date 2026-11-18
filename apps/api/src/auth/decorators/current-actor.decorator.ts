import { createParamDecorator, ExecutionContext } from '@nestjs/common';
import type { AuthenticatedActor } from '../auth.service';
import { ACTOR_KEY } from '../guards/jwt-auth.guard';

/** Parameter decorator exposing the authenticated actor (user + session). */
export const CurrentActor = createParamDecorator(
  (_data: unknown, ctx: ExecutionContext): AuthenticatedActor => {
    const request =
      ctx.getType() === 'ws'
        ? (ctx.switchToWs().getData() as Record<string, unknown>)
        : (ctx.switchToHttp().getRequest() as Record<string, unknown>);
    return request[ACTOR_KEY] as AuthenticatedActor;
  },
);
