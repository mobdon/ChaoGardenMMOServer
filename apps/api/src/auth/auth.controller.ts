/**
 * Auth controller. All routes are guarded unless marked `@Public()`.
 * Every route has a Redis-backed rate limit; logs never contain
 * credentials or raw tokens.
 */
import {
  Body,
  Controller,
  Get,
  HttpCode,
  Param,
  Post,
  Request,
  UseGuards,
} from '@nestjs/common';
import {
  guestCreateSchema,
  guestUpgradeSchema,
  loginSchema,
  logoutSchema,
  refreshSchema,
  registerSchema,
  type AuthSession,
  type CurrentUser,
} from '@chaogarden/contracts';
import { AuthService, type AuthenticatedActor } from './auth.service';
import { JwtAuthGuard } from './guards/jwt-auth.guard';
import { Public } from './decorators/public.decorator';
import { CurrentActor } from './decorators/current-actor.decorator';
import { ZodValidationPipe } from '../common/zod-validation.pipe';
import { RateLimit, RateLimitGuard, clientIp } from './throttle/rate-limit.guard';
import { ThrottleService } from './throttle/throttle.service';
import { TokenService } from './token.service';

interface AuthRequest {
  headers?: Record<string, string | undefined>;
  body?: Record<string, unknown>;
}

@Controller('api/auth')
@UseGuards(RateLimitGuard)
export class AuthController {
  constructor(
    private readonly auth: AuthService,
    private readonly tokens: TokenService,
    private readonly throttle: ThrottleService,
  ) {}

  private deviceName(req: AuthRequest): string {
    // Device name is supplied by the client in the body; the header is a fallback.
    const ua = req.headers?.['user-agent'];
    return typeof ua === 'string' ? ua.slice(0, 80) : 'Unknown device';
  }

  /** Best-effort authenticated context (used for guest upgrade ownership checks). */
  private async optionalActor(req: AuthRequest): Promise<AuthenticatedActor | null> {
    const header = req.headers?.authorization;
    if (!header?.startsWith('Bearer ')) return null;
    try {
      const claims = await this.tokens.verifyAccessToken(header.slice('Bearer '.length).trim());
      return { userId: claims.sub, sessionId: claims.sid, guest: claims.guest };
    } catch {
      return null;
    }
  }

  @Public()
  @Post('register')
  @RateLimit({ bucket: 'register', limit: 5, windowSeconds: 3600 })
  register(
    @Body(new ZodValidationPipe(registerSchema))
    input: { email: string; password: string; displayName?: string; guestToken?: string },
    @Request() req: AuthRequest,
  ): Promise<AuthSession> {
    return this.auth.register(input, this.deviceName(req));
  }

  @Public()
  @Post('login')
  @HttpCode(200)
  @RateLimit({ bucket: 'login', limit: 10, windowSeconds: 900 })
  async login(
    @Body(new ZodValidationPipe(loginSchema))
    input: { email: string; password: string; deviceName: string },
    @Request() req: AuthRequest,
  ): Promise<AuthSession> {
    const session = await this.auth.login(input, input.deviceName || this.deviceName(req));
    await this.throttle.reset('login-ip', clientIp(req));
    return session;
  }

  @Public()
  @Post('refresh')
  @HttpCode(200)
  @RateLimit({ bucket: 'refresh', limit: 30, windowSeconds: 300 })
  refresh(
    @Body(new ZodValidationPipe(refreshSchema)) input: { refreshToken: string },
  ): Promise<AuthSession> {
    return this.auth.refresh(input.refreshToken);
  }

  @Public()
  @Post('logout')
  @HttpCode(200)
  async logout(
    @Body(new ZodValidationPipe(logoutSchema)) input: { refreshToken: string },
  ): Promise<{ ok: true }> {
    await this.auth.logout(input.refreshToken);
    return { ok: true };
  }

  @UseGuards(JwtAuthGuard)
  @Post('logout-all')
  @HttpCode(200)
  logoutAll(@CurrentActor() actor: AuthenticatedActor): Promise<{ ok: true }> {
    return this.auth.logoutAll(actor.userId).then(() => ({ ok: true }));
  }

  @UseGuards(JwtAuthGuard)
  @Get('me')
  me(@CurrentActor() actor: AuthenticatedActor): Promise<CurrentUser> {
    return this.auth.currentUser(actor.userId, actor.sessionId);
  }

  @Public()
  @Post('guest')
  @RateLimit({ bucket: 'guest-create', limit: 5, windowSeconds: 3600 })
  createGuest(
    @Body(new ZodValidationPipe(guestCreateSchema)) input: { deviceName: string },
    @Request() req: AuthRequest,
  ): Promise<AuthSession> {
    return this.auth.createGuest(input.deviceName || this.deviceName(req));
  }

  @Public()
  @Post('guest/upgrade')
  @RateLimit({ bucket: 'guest-upgrade', limit: 10, windowSeconds: 3600 })
  upgradeGuest(
    @Body(new ZodValidationPipe(guestUpgradeSchema))
    input: { email: string; password: string; displayName?: string; guestToken: string },
    @Request() req: AuthRequest,
  ): Promise<AuthSession> {
    return this.optionalActor(req).then((actor) => this.auth.upgradeGuest(input, actor));
  }

  @UseGuards(JwtAuthGuard)
  @Get('sessions')
  async sessions(@CurrentActor() actor: AuthenticatedActor): Promise<{ sessions: CurrentUser['sessions'] }> {
    const { sessions } = await this.auth.currentUser(actor.userId, actor.sessionId);
    return { sessions };
  }

  @UseGuards(JwtAuthGuard)
  @Post('sessions/:sessionId/revoke')
  async revoke(
    @CurrentActor() actor: AuthenticatedActor,
    @Param('sessionId') sessionId: string,
  ): Promise<{ ok: true }> {
    await this.auth.revokeSession(actor, sessionId);
    return { ok: true };
  }
}
