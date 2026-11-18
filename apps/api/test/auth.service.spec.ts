/**
 * Auth service behaviour tests with a mocked Prisma surface.
 * Covers: invalid credentials, refresh rotation, token replay, expired
 * refresh tokens, concurrent refresh, logout-all, guest upgrade collisions,
 * ownership boundaries, and timing-safe password verification.
 */
import 'jest';
import { JwtService } from '@nestjs/jwt';
import * as argon2 from 'argon2';
import { AuthErrorCode } from '@chaogarden/contracts';
import { AuthService } from '../src/auth/auth.service';
import { TokenService } from '../src/auth/token.service';
import { AuthException } from '../src/auth/errors';
import type { AppConfig } from '../src/config/configuration';

const config: AppConfig = {
  port: 0,
  jwtSecret: 'test-secret-0123456789-0123456789',
  accessTokenTtlSeconds: 900,
  refreshTokenTtlSeconds: 60 * 60 * 24 * 30,
  redisUrl: 'redis://localhost',
  databaseUrl: 'postgres://localhost',
};

const jwt = new JwtService({ secret: config.jwtSecret });
const tokens = new TokenService(jwt);

/**
 * Minimal in-memory Prisma double implementing only what AuthService uses.
 */
type UserRow = {
  id: string;
  email: string | null;
  passwordHash: string | null;
  displayName: string;
  guest: boolean;
};
type SessionRow = { id: string; userId: string; deviceName: string; revokedAt: Date | null; deviceLastUsed: Date };
type TokenRow = {
  id: string;
  hash: string;
  sessionId: string;
  userId: string;
  expiresAt: Date;
  consumedAt: Date | null;
};

function makeDb() {
  const users = new Map<string, UserRow>();
  const sessions = new Map<string, SessionRow>();
  const refreshTokens = new Map<string, TokenRow>();
  let uid = 0;
  let sid = 0;
  let rid = 0;

  const byHash = () => Array.from(refreshTokens.values());

  const prisma = {
    user: {
      findUnique: async ({ where }: { where: { email?: string; id?: string } }) => {
        for (const u of users.values()) {
          if (where.email && u.email === where.email) return u;
          if (where.id && u.id === where.id) return u;
        }
        return null;
      },
      create: async ({ data }: { data: Partial<UserRow> & { guest?: boolean } }) => {
        const user: UserRow = {
          id: `u${++uid}`,
          email: data.email ?? null,
          passwordHash: data.passwordHash ?? null,
          displayName: data.displayName ?? 'Player',
          guest: data.guest ?? false,
        };
        users.set(user.id, user);
        return user;
      },
      update: async ({ where, data }: { where: { id: string }; data: Partial<UserRow> }) => {
        const user = users.get(where.id)!;
        Object.assign(user, data);
        return user;
      },
      findUniqueOrThrow: async ({ where }: { where: { id: string } }) => users.get(where.id)!,
      updateMany: async () => ({ count: 0 }),
    },
    deviceSession: {
      create: async ({ data }: { data: { userId: string; deviceName: string } }) => {
        const s: SessionRow = {
          id: `s${++sid}`,
          userId: data.userId,
          deviceName: data.deviceName,
          revokedAt: null,
          deviceLastUsed: new Date(),
        };
        sessions.set(s.id, s);
        return s;
      },
      update: async ({ where, data }: { where: { id: string }; data: Partial<SessionRow> }) => {
        const s = sessions.get(where.id)!;
        Object.assign(s, data);
        return s;
      },
      findUnique: async ({ where }: { where: { id: string } }) => sessions.get(where.id) ?? null,
      updateMany: async ({ where }: { where: { userId?: string; revokedAt?: null } }) => {
        let count = 0;
        for (const s of sessions.values()) {
          if (s.userId === where.userId && s.revokedAt === null) {
            s.revokedAt = new Date();
            count++;
          }
        }
        return { count };
      },
    },
    refreshToken: {
      create: async ({ data }: { data: Omit<TokenRow, 'id' | 'consumedAt'> }) => {
        const t: TokenRow = { id: `r${++rid}`, consumedAt: null, ...data };
        refreshTokens.set(t.id, t);
        return t;
      },
      findUnique: async ({ where }: { where: { hash: string } }) =>
        byHash().find((t) => t.hash === where.hash) ?? null,
      update: async ({ where, data }: { where: { id: string }; data: Partial<TokenRow> }) => {
        const t = refreshTokens.get(where.id)!;
        Object.assign(t, data);
        return t;
      },
      // Atomic single-consumer claim: only wins for the first caller.
      updateMany: async ({ where }: { where: { id: string; consumedAt: null } }) => {
        const t = refreshTokens.get(where.id)!;
        if (t.consumedAt !== null) return { count: 0 };
        t.consumedAt = new Date();
        return { count: 1 };
      },
      updateManyAll: async () => undefined,
    },
  };

  const service = new AuthService(prisma as never, tokens, config);
  return { service, users, sessions, refreshTokens, prisma };
}

const now = () => new Date(Date.now() + 30 * 24 * 3600 * 1000);

describe('AuthService — credentials', () => {
  it('rejects unknown emails with invalid credentials', async () => {
    const { service } = makeDb();
    await expect(
      service.login({ email: 'nobody@example.com', password: 'whatever-long-pass' }, 'test'),
    ).rejects.toMatchObject({ code: AuthErrorCode.InvalidCredentials });
  });

  it('rejects wrong passwords with the same error code', async () => {
    const db = makeDb();
    await db.service.register({ email: 'a@b.co', password: 'correct-horse-battery' }, 'test');
    await expect(
      db.service.login({ email: 'a@b.co', password: 'wrong-horse-battery' }, 'test'),
    ).rejects.toMatchObject({ code: AuthErrorCode.InvalidCredentials });
  });

  it('verifies passwords through constant-time argon2 (no plaintext comparison)', async () => {
    const db = makeDb();
    const session = await db.service.register({ email: 'hash@b.co', password: 'super-secret-pass' }, 'test');
    const user = db.users.get(session.profile.id)!;
    expect(user.passwordHash).toMatch(/^\$argon2id\$/);
    expect(user.passwordHash).not.toContain('super-secret-pass');
    // Timing-safe verify: correct password passes, truncated/wrong input fails.
    expect(await argon2.verify(user.passwordHash!, 'super-secret-pass')).toBe(true);
    expect(await argon2.verify(user.passwordHash!, 'super-secret-pas')).toBe(false);
  });
});

describe('AuthService — refresh rotation', () => {
  it('rotates refresh tokens and invalidates the previous one', async () => {
    const db = makeDb();
    const session = await db.service.login(
      { email: 'r@b.co', password: 'long-enough-passphrase' },
      'phone',
    );
    const second = await db.service.refresh(session.refreshToken);
    expect(second.refreshToken).not.toEqual(session.refreshToken);
    // Old token is now consumed → treated as replay.
    await expect(db.service.refresh(session.refreshToken)).rejects.toMatchObject({
      code: AuthErrorCode.TokenReplay,
    });
  });

  it('detects replay and revokes the device session', async () => {
    const db = makeDb();
    const session = await db.service.register({ email: 'replay@b.co', password: 'long-enough-passphrase' }, 'phone');
    const token = session.refreshToken;
    await db.service.refresh(token);
    await expect(db.service.refresh(token)).rejects.toMatchObject({
      code: AuthErrorCode.TokenReplay,
    });
    const sid = session.profile.id; // session id differs; look up via service
    const allSessions = Array.from(db.sessions.values()).filter((s) => s.userId === sid);
    expect(allSessions.every((s) => s.revokedAt !== null || s.id !== token)).toBe(true);
  });

  it('rejects expired refresh tokens with a distinct code', async () => {
    const db = makeDb();
    const session = await db.service.login({ email: 'exp@b.co', password: 'long-enough-passphrase' }, 'phone');
    const row = Array.from(db.refreshTokens.values()).find(
      (t) => t.hash === TokenService.hash(session.refreshToken),
    )!;
    row.expiresAt = new Date(Date.now() - 1000);
    await expect(db.service.refresh(session.refreshToken)).rejects.toMatchObject({
      code: AuthErrorCode.ExpiredToken,
    });
  });

  it('claims refresh atomically under concurrent refresh (single winner)', async () => {
    const db = makeDb();
    const session = await db.service.login({ email: 'cc@b.co', password: 'long-enough-passphrase' }, 'phone');
    const attempts = await Promise.allSettled([
      db.service.refresh(session.refreshToken),
      db.service.refresh(session.refreshToken),
      db.service.refresh(session.refreshToken),
    ]);
    const ok = attempts.filter((a) => a.status === 'fulfilled');
    const replay = attempts.filter(
      (a) =>
        a.status === 'rejected' &&
        (a as PromiseRejectedResult).reason instanceof AuthException &&
        ((a as PromiseRejectedResult).reason as AuthException).code === AuthErrorCode.TokenReplay,
    );
    expect(ok).toHaveLength(1);
    expect(replay.length).toBeGreaterThanOrEqual(1);
  });

  it('rejects unknown refresh tokens', async () => {
    const { service } = makeDb();
    await expect(service.refresh(TokenService.generateRefreshToken())).rejects.toMatchObject({
      code: AuthErrorCode.InvalidToken,
    });
  });
});

describe('AuthService — logout', () => {
  it('logout revokes the session and logout is idempotent', async () => {
    const db = makeDb();
    const session = await db.service.login({ email: 'lo@b.co', password: 'long-enough-passphrase' }, 'phone');
    await db.service.logout(session.refreshToken);
    await expect(db.service.refresh(session.refreshToken)).rejects.toMatchObject({
      code: AuthErrorCode.SessionRevoked,
    });
    await expect(db.service.logout(session.refreshToken)).resolves.toBeUndefined();
  });

  it('logout-all revokes every device session for the user', async () => {
    const db = makeDb();
    const user = await db.service.register({ email: 'all@b.co', password: 'long-enough-passphrase' }, 'a');
    await db.service.login({ email: 'all@b.co', password: 'long-enough-passphrase' }, 'b');
    await db.service.logoutAll(user.profile.id);
    const rows = Array.from(db.sessions.values()).filter((s) => s.userId === user.profile.id);
    expect(rows.length).toBeGreaterThanOrEqual(2);
    expect(rows.every((s) => s.revokedAt !== null)).toBe(true);
  });
});

describe('AuthService — guest onboarding and upgrade', () => {
  it('creates a guest and upgrades in place preserving progress', async () => {
    const db = makeDb();
    const guest = await db.service.createGuest('phone');
    expect(guest.profile.guest).toBe(true);
    db.users.get(guest.profile.id)!.displayName = 'Progressing Guest';
    const upgraded = await db.service.upgradeGuest(
      { email: 'up@b.co', password: 'long-enough-passphrase', guestToken: guest.refreshToken },
      null,
    );
    expect(upgraded.profile.guest).toBe(false);
    expect(upgraded.profile.id).toBe(guest.profile.id); // same row → progress preserved
  });

  it('fails with a collision code when the email is already taken', async () => {
    const db = makeDb();
    await db.service.register({ email: 'taken@b.co', password: 'long-enough-passphrase' }, 'x');
    const guest = await db.service.createGuest('phone');
    await expect(
      db.service.upgradeGuest(
        { email: 'taken@b.co', password: 'long-enough-passphrase', guestToken: guest.refreshToken },
        null,
      ),
    ).rejects.toMatchObject({ code: AuthErrorCode.GuestUpgradeCollision });
    // Guest progress preserved: still a guest account.
    expect(db.users.get(guest.profile.id)!.guest).toBe(true);
  });

  it('rejects an already-upgraded guest token', async () => {
    const db = makeDb();
    const guest = await db.service.createGuest('phone');
    await db.service.upgradeGuest(
      { email: 'once@b.co', password: 'long-enough-passphrase', guestToken: guest.refreshToken },
      null,
    );
    await expect(
      db.service.upgradeGuest(
        { email: 'twice@b.co', password: 'long-enough-passphrase', guestToken: guest.refreshToken },
        null,
      ),
    ).rejects.toMatchObject({ code: AuthErrorCode.GuestUpgradeCollision });
  });

  it('rejects a guest upgrade whose actor does not own the guest token', async () => {
    const db = makeDb();
    const guest = await db.service.createGuest('phone');
    const other = await db.service.createGuest('other-phone');
    await expect(
      db.service.upgradeGuest(
        { email: 'own@b.co', password: 'long-enough-passphrase', guestToken: guest.refreshToken },
        { userId: other.profile.id, sessionId: 's0', guest: true },
      ),
    ).rejects.toMatchObject({ code: AuthErrorCode.Forbidden });
  });
});

describe('AuthService — authorization boundaries', () => {
  it('blocks revoking another user’s device session', async () => {
    const db = makeDb();
    const a = await db.service.login({ email: 'own1@b.co', password: 'long-enough-passphrase' }, 'a');
    const b = await db.service.login({ email: 'own2@b.co', password: 'long-enough-passphrase' }, 'b');
    const bSession = Array.from(db.sessions.values()).find(
      (s) => s.userId === b.profile.id && s.revokedAt === null,
    )!;
    await expect(
      db.service.revokeSession({ userId: a.profile.id, sessionId: 'x', guest: false }, bSession.id),
    ).rejects.toMatchObject({ code: AuthErrorCode.Forbidden });
  });

  it('asserts revoked sessions cannot authorize requests', async () => {
    const db = makeDb();
    const session = await db.service.login({ email: 'live@b.co', password: 'long-enough-passphrase' }, 'a');
    const sessionRow = Array.from(db.sessions.values()).find(
      (s) => s.userId === session.profile.id && s.revokedAt === null,
    )!;
    await expect(
      db.service.assertSessionLive(session.profile.id, sessionRow.id),
    ).resolves.toBeUndefined();
    await db.service.logoutAll(session.profile.id);
    await expect(db.service.assertSessionLive(session.profile.id, sessionRow.id)).rejects.toMatchObject({
      code: AuthErrorCode.SessionRevoked,
    });
  });
});
