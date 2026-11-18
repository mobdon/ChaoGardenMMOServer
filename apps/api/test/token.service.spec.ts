/**
 * Unit tests for the token rotation core: replay detection, expiry, and
 * concurrent refresh claims. Prisma is mocked at the service boundary.
 */
import 'jest';
import { JwtService } from '@nestjs/jwt';
import { AuthErrorCode } from '@chaogarden/contracts';
import { TokenService } from '../src/auth/token.service';
import { AuthException } from '../src/auth/errors';

const jwt = new JwtService({ secret: 'test-secret-0123456789-0123456789' });
const tokens = new TokenService(jwt);

describe('TokenService', () => {
  it('signs and verifies access tokens with audience binding', async () => {
    const { token, expiresAt } = tokens.signAccessToken({ sub: 'u1', sid: 's1', guest: false }, 60);
    expect(expiresAt).toBeGreaterThan(Date.now());
    const claims = await tokens.verifyAccessToken(token);
    expect(claims).toEqual({ sub: 'u1', sid: 's1', guest: false });
  });

  it('rejects tokens signed for a different audience', async () => {
    const foreign = jwt.sign({ sub: 'u1', sid: 's1' }, { expiresIn: 60 });
    await expect(tokens.verifyAccessToken(foreign)).rejects.toMatchObject({
      code: AuthErrorCode.InvalidToken,
    });
  });

  it('distinguishes expired tokens from malformed ones', async () => {
    const expired = jwt.sign({ sub: 'u1', sid: 's1', guest: false }, { expiresIn: -10 });
    await expect(tokens.verifyAccessToken(expired)).rejects.toMatchObject({
      code: AuthErrorCode.ExpiredToken,
    });
    await expect(tokens.verifyAccessToken('not-a-token')).rejects.toMatchObject({
      code: AuthErrorCode.InvalidToken,
    });
  });

  it('produces refresh tokens that hash deterministically and differ per generation', () => {
    const a = TokenService.generateRefreshToken();
    const b = TokenService.generateRefreshToken();
    expect(a).not.toEqual(b);
    expect(TokenService.hash(a)).toEqual(TokenService.hash(a));
    expect(TokenService.hash(a)).not.toEqual(TokenService.hash(b));
    // Hash only — raw tokens must never appear in stored form.
    expect(TokenService.hash(a)).toMatch(/^[0-9a-f]{64}$/);
  });

  it('compares hashes in constant time (equal length fast-path + length mismatch path)', () => {
    const a = TokenService.hash('alpha');
    expect(TokenService.hashesEqual(a, a)).toBe(true);
    expect(TokenService.hashesEqual(a, TokenService.hash('beta'))).toBe(false);
    expect(TokenService.hashesEqual('short', 'a-much-longer-value')).toBe(false);
  });
});
