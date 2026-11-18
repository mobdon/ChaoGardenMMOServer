/**
 * Refresh tokens are stored only as SHA-256 hashes; the raw opaque token is
 * shown to the client exactly once. Replay of a consumed token revokes the
 * whole device session (theft detection).
 */
import { randomBytes, createHash, timingSafeEqual } from 'node:crypto';
import { Injectable, UnauthorizedException } from '@nestjs/common';
import { JwtService } from '@nestjs/jwt';
import { AuthErrorCode } from '@chaogarden/contracts';
import { AuthException } from './errors';

export const ACCESS_AUDIENCE = 'chaogarden:access';
export const REFRESH_AUDIENCE = 'chaogarden:refresh';

export interface AccessTokenClaims {
  sub: string;
  sid: string; // device session id
  guest: boolean;
}

export interface HashedRefreshToken {
  id: string;
  hash: string;
  sessionId: string;
  expiresAt: Date;
  consumedAt: Date | null;
}

@Injectable()
export class TokenService {
  constructor(private readonly jwt: JwtService) {}

  /** Opaque, URL-safe refresh token (256 bits of entropy). */
  static generateRefreshToken(): string {
    return randomBytes(32).toString('base64url');
  }

  static hash(token: string): string {
    return createHash('sha256').update(token).digest('hex');
  }

  /** Constant-time comparison of two refresh-token hashes. */
  static hashesEqual(a: string, b: string): boolean {
    const bufA = Buffer.from(a, 'utf8');
    const bufB = Buffer.from(b, 'utf8');
    if (bufA.length !== bufB.length) {
      // Compare against self to keep the timing profile flat, then fail.
      timingSafeEqual(bufA, bufA);
      return false;
    }
    return timingSafeEqual(bufA, bufB);
  }

  signAccessToken(claims: AccessTokenClaims, ttlSeconds: number): { token: string; expiresAt: number } {
    const expiresAt = Math.floor(Date.now() / 1000) + ttlSeconds;
    const token = this.jwt.sign(
      { ...claims },
      {
        secret: (this.jwt.options as { secret: string }).secret,
        audience: ACCESS_AUDIENCE,
        expiresIn: ttlSeconds,
      },
    );
    return { token, expiresAt: expiresAt * 1000 };
  }

  /** Verifies an access token, distinguishing expiry from other failures. */
  async verifyAccessToken(token: string): Promise<AccessTokenClaims> {
    try {
      const payload = await this.jwt.verifyAsync(token, {
        secret: (this.jwt.options as { secret: string }).secret,
        audience: ACCESS_AUDIENCE,
      });
      if (typeof payload.sub !== 'string' || typeof payload.sid !== 'string') {
        throw new AuthException(AuthErrorCode.InvalidToken, 'Malformed token');
      }
      return { sub: payload.sub, sid: payload.sid, guest: payload.guest === true };
    } catch (err) {
      if (err instanceof AuthException) throw err;
      if (err instanceof UnauthorizedException) throw err;
      const name = (err as { name?: string; message?: string })?.name ?? '';
      const msg = (err as { message?: string })?.message ?? '';
      if (name === 'TokenExpiredError' || msg.includes('expired')) {
        throw new AuthException(AuthErrorCode.ExpiredToken, 'Access token expired');
      }
      throw new AuthException(AuthErrorCode.InvalidToken, 'Access token rejected');
    }
  }
}
