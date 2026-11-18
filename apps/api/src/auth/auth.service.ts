/**
 * Account + session service.
 *
 * Security invariants:
 * - Passwords hashed with Argon2id (memory-cost 19 MiB, 2 iterations, 1 lane).
 * - Verification goes through `argon2.verify` which is constant-time in the
 *   hash comparison; we additionally pad failures with a dummy verify so
 *   unknown emails and known emails take a similar amount of time.
 * - Refresh tokens are opaque, stored only as SHA-256 hashes, single-use and
 *   rotated on every refresh. Reuse of a consumed token revokes the device
 *   session (replay detection).
 * - Device sessions are independently revocable; logout-all revokes every one.
 */
import { Injectable } from '@nestjs/common';
import * as argon2 from 'argon2';
import { AuthErrorCode } from '@chaogarden/contracts';
import type { DeviceSession, Profile, AuthSession } from '@chaogarden/contracts';
import { PrismaService } from '../database/prisma.service';
import { AppConfig } from '../config/configuration';
import { AuthException, sanitize } from './errors';
import { TokenService } from './token.service';

const ARGON2_OPTS: argon2.Options = {
  type: argon2.argon2id,
  memoryCost: 19456, // 19 MiB — OWASP baseline
  timeCost: 2,
  parallelism: 1,
};

// Pre-baked hash used to equalize timing when the email does not exist.
const DUMMY_HASH =
  '$argon2id$v=19$m=19456,t=2,p=1$ZGVhZGJlZWZkZWFkYmVlZg$c4V1sGk1IUYsoH26zZ0TLtR1nF3zZ6kT5zN7xQb0Y2A';

export interface AuthenticatedActor {
  userId: string;
  sessionId: string;
  guest: boolean;
}

@Injectable()
export class AuthService {
  constructor(
    private readonly prisma: PrismaService,
    private readonly tokens: TokenService,
    private readonly config: AppConfig,
  ) {}

  // -------------------------------------------------------------------------
  // Registration
  // -------------------------------------------------------------------------

  async register(
    input: { email: string; password: string; displayName?: string; guestToken?: string },
    deviceName: string,
  ): Promise<AuthSession> {
    const existing = await this.prisma.user.findUnique({ where: { email: input.email } });
    if (existing) {
      throw new AuthException(AuthErrorCode.EmailTaken, 'Email already registered');
    }

    let upgradedUserId: string | null = null;
    if (input.guestToken) {
      const guest = await this.resolveGuestByRefreshToken(input.guestToken);
      upgradedUserId = guest.userId;
      if (guest.user.email) {
        throw new AuthException(AuthErrorCode.GuestUpgradeCollision, 'Guest already has an account');
      }
      const clash = await this.prisma.user.findUnique({ where: { email: input.email } });
      if (clash && clash.id !== guest.userId) {
        throw new AuthException(
          AuthErrorCode.GuestUpgradeCollision,
          'An account with that email already exists; guest progress was preserved',
        );
      }
    }

    const passwordHash = await argon2.hash(input.password, ARGON2_OPTS);

    let user = upgradedUserId
      ? await this.prisma.user.update({
          where: { id: upgradedUserId },
          data: { email: input.email, passwordHash, guest: false, displayName: input.displayName ?? 'Player' },
        })
      : await this.prisma.user.create({
          data: {
            email: input.email,
            passwordHash,
            displayName: input.displayName ?? input.email.split('@')[0]!,
            guest: false,
          },
        });

    const session = await this.issueSession(user.id, user.guest, deviceName);
    void user;
    return session;
  }

  // -------------------------------------------------------------------------
  // Login
  // -------------------------------------------------------------------------

  async login(
    input: { email: string; password: string },
    deviceName: string,
  ): Promise<AuthSession> {
    const user = await this.prisma.user.findUnique({ where: { email: input.email } });

    if (!user) {
      // Timing-safe path: burn an equivalent Argon2 verify on a dummy hash so
      // response latency does not reveal whether the email exists.
      await argon2.verify(DUMMY_HASH, input.password).catch(() => false);
      throw new AuthException(AuthErrorCode.InvalidCredentials, 'Invalid email or password');
    }
    if (user.guest || !user.passwordHash) {
      await argon2.verify(DUMMY_HASH, input.password).catch(() => false);
      throw new AuthException(AuthErrorCode.InvalidCredentials, 'Invalid email or password');
    }

    // argon2.verify performs the digest comparison in constant time.
    const valid = await argon2.verify(user.passwordHash, input.password);
    if (!valid) {
      throw new AuthException(AuthErrorCode.InvalidCredentials, 'Invalid email or password');
    }

    return this.issueSession(user.id, user.guest, deviceName);
  }

  // -------------------------------------------------------------------------
  // Refresh rotation
  // -------------------------------------------------------------------------

  async refresh(rawRefreshToken: string): Promise<AuthSession> {
    const hash = TokenService.hash(rawRefreshToken);
    const record = await this.prisma.refreshToken.findUnique({
      where: { hash },
      include: { user: true, session: true },
    });

    if (!record) {
      throw new AuthException(AuthErrorCode.InvalidToken, 'Unknown refresh token');
    }

    if (record.consumedAt) {
      // Replay of an already-rotated token: assume theft and kill the session.
      await this.prisma.deviceSession.update({
        where: { id: record.sessionId },
        data: { revokedAt: new Date() },
      });
      throw new AuthException(AuthErrorCode.TokenReplay, 'Refresh token reuse detected; session revoked');
    }

    if (record.session.revokedAt) {
      throw new AuthException(AuthErrorCode.SessionRevoked, 'Device session revoked');
    }

    if (record.expiresAt.getTime() <= Date.now()) {
      throw new AuthException(AuthErrorCode.ExpiredToken, 'Refresh token expired');
    }

    // Atomically consume so two concurrent refreshes cannot both succeed.
    const claimed = await this.prisma.refreshToken.updateMany({
      where: { id: record.id, consumedAt: null },
      data: { consumedAt: new Date() },
    });
    if (claimed.count !== 1) {
      await this.prisma.deviceSession.update({
        where: { id: record.sessionId },
        data: { revokedAt: new Date() },
      });
      throw new AuthException(AuthErrorCode.TokenReplay, 'Concurrent refresh detected; session revoked');
    }

    return this.issueSession(record.userId, record.user.guest, record.session.deviceName, record.sessionId);
  }

  // -------------------------------------------------------------------------
  // Logout
  // -------------------------------------------------------------------------

  async logout(rawRefreshToken: string): Promise<void> {
    const hash = TokenService.hash(rawRefreshToken);
    const record = await this.prisma.refreshToken.findUnique({ where: { hash } });
    if (!record) return; // Logout is idempotent.
    await this.prisma.refreshToken.update({ where: { id: record.id }, data: { consumedAt: new Date() } });
    await this.prisma.deviceSession.update({
      where: { id: record.sessionId },
      data: { revokedAt: new Date() },
    });
  }

  async logoutAll(userId: string): Promise<void> {
    await this.prisma.deviceSession.updateMany({
      where: { userId, revokedAt: null },
      data: { revokedAt: new Date() },
    });
    await this.prisma.refreshToken.updateMany({
      where: { userId, consumedAt: null },
      data: { consumedAt: new Date() },
    });
  }

  async revokeSession(actor: AuthenticatedActor, sessionId: string): Promise<void> {
    // Ownership check: a user may only revoke their own sessions.
    const target = await this.prisma.deviceSession.findUnique({ where: { id: sessionId } });
    if (!target || target.userId !== actor.userId) {
      throw new AuthException(AuthErrorCode.Forbidden, 'Not your device session');
    }
    await this.prisma.deviceSession.update({ where: { id: sessionId }, data: { revokedAt: new Date() } });
    await this.prisma.refreshToken.updateMany({
      where: { sessionId, consumedAt: null },
      data: { consumedAt: new Date() },
    });
  }

  // -------------------------------------------------------------------------
  // Guest onboarding
  // -------------------------------------------------------------------------

  /** Creates a lightweight guest account so play can start before signup. */
  async createGuest(deviceName: string): Promise<AuthSession> {
    const user = await this.prisma.user.create({
      data: { displayName: 'Guest', guest: true },
    });
    return this.issueSession(user.id, true, deviceName);
  }

  /**
   * Upgrades a guest in place: same user row, so creatures, inventory and
   * rings survive. Fails with `AUTH_GUEST_UPGRADE_COLLISION` if the email is
   * already owned by another account (progress preserved on the guest).
   */
  async upgradeGuest(
    input: { email: string; password: string; displayName?: string; guestToken: string },
    actor: AuthenticatedActor | null,
  ): Promise<AuthSession> {
    const guest = await this.resolveGuestByRefreshToken(input.guestToken);
    if (actor && actor.userId !== guest.userId) {
      throw new AuthException(AuthErrorCode.Forbidden, 'Guest token does not belong to you');
    }
    if (guest.user.email) {
      throw new AuthException(AuthErrorCode.GuestUpgradeCollision, 'Guest already upgraded');
    }
    const clash = await this.prisma.user.findUnique({ where: { email: input.email } });
    if (clash && clash.id !== guest.userId) {
      throw new AuthException(
        AuthErrorCode.GuestUpgradeCollision,
        'An account with that email already exists; guest progress was preserved',
      );
    }

    const passwordHash = await argon2.hash(input.password, ARGON2_OPTS);
    const user = await this.prisma.user.update({
      where: { id: guest.userId },
      data: {
        email: input.email,
        passwordHash,
        guest: false,
        displayName: input.displayName ?? 'Player',
      },
    });

    const session = await this.issueSession(user.id, false, 'Upgraded guest device');
    return session;
  }

  // -------------------------------------------------------------------------
  // Queries
  // -------------------------------------------------------------------------

  async currentUser(userId: string, currentSessionId: string): Promise<{ profile: Profile; sessions: DeviceSession[] }> {
    const user = await this.prisma.user.findUnique({
      where: { id: userId },
      include: { sessions: { where: { revokedAt: null }, orderBy: { createdAt: 'desc' } } },
    });
    if (!user) {
      throw new AuthException(AuthErrorCode.Unauthenticated, 'Account no longer exists');
    }
    return {
      profile: toProfile(user),
      sessions: user.sessions.map((s) => ({
        id: s.id,
        deviceName: s.deviceName,
        createdAt: s.createdAt.getTime(),
        lastUsedAt: s.lastUsedAt.getTime(),
        current: s.id === currentSessionId,
      })),
    };
  }

  /** Guard support: assert the session backing an access token is live. */
  async assertSessionLive(userId: string, sessionId: string): Promise<void> {
    const session = await this.prisma.deviceSession.findUnique({ where: { id: sessionId } });
    if (!session || session.userId !== userId) {
      throw new AuthException(AuthErrorCode.SessionRevoked, 'Device session revoked');
    }
    if (session.revokedAt) {
      throw new AuthException(AuthErrorCode.SessionRevoked, 'Device session revoked');
    }
  }

  // -------------------------------------------------------------------------
  // Internals
  // -------------------------------------------------------------------------

  private async resolveGuestByRefreshToken(guestToken: string) {
    const hash = TokenService.hash(guestToken);
    const record = await this.prisma.refreshToken.findUnique({
      where: { hash },
      include: { user: true },
    });
    if (!record || record.consumedAt || record.expiresAt.getTime() <= Date.now()) {
      throw new AuthException(AuthErrorCode.InvalidToken, 'Invalid guest token');
    }
    if (!record.user.guest) {
      throw new AuthException(AuthErrorCode.InvalidToken, 'Token is not a guest token');
    }
    return { userId: record.userId, user: record.user };
  }

  private async issueSession(
    userId: string,
    guest: boolean,
    deviceName: string,
    reuseSessionId?: string,
  ): Promise<AuthSession> {
    const session = reuseSessionId
      ? await this.prisma.deviceSession.update({
          where: { id: reuseSessionId },
          data: { lastUsedAt: new Date() },
        })
      : await this.prisma.deviceSession.create({
          data: { userId, deviceName: sanitize(deviceName) },
        });

    const rawRefresh = TokenService.generateRefreshToken();
    const refreshExpiresAt = new Date(Date.now() + this.config.refreshTokenTtlSeconds * 1000);
    await this.prisma.refreshToken.create({
      data: {
        hash: TokenService.hash(rawRefresh),
        sessionId: session.id,
        userId,
        expiresAt: refreshExpiresAt,
      },
    });

    const access = this.tokens.signAccessToken({ sub: userId, sid: session.id, guest }, this.config.accessTokenTtlSeconds);
    const user = await this.prisma.user.findUniqueOrThrow({ where: { id: userId } });

    return {
      accessToken: access.token,
      accessExpiresAt: access.expiresAt,
      refreshToken: rawRefresh,
      refreshTokenExpiresAt: refreshExpiresAt.getTime(),
      profile: toProfile(user),
    };
  }
}

export function toProfile(user: {
  id: string;
  displayName: string;
  email: string | null;
  guest: boolean;
}): Profile {
  return { id: user.id, displayName: user.displayName, email: user.email, guest: user.guest };
}
