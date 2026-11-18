/**
 * Stable, machine-readable error taxonomy for authentication.
 *
 * Every auth failure maps to one `code`. Clients branch on `code`, never on
 * human messages. Logs produced from these errors must never include
 * credentials or raw tokens.
 */
import { HttpException, HttpStatus } from '@nestjs/common';
import { AuthErrorCode } from '@chaogarden/contracts';

type Code = (typeof AuthErrorCode)[keyof typeof AuthErrorCode];

const STATUS_BY_CODE: Record<string, HttpStatus> = {
  [AuthErrorCode.InvalidCredentials]: HttpStatus.UNAUTHORIZED,
  [AuthErrorCode.InvalidToken]: HttpStatus.UNAUTHORIZED,
  [AuthErrorCode.ExpiredToken]: HttpStatus.UNAUTHORIZED,
  [AuthErrorCode.TokenReplay]: HttpStatus.UNAUTHORIZED,
  [AuthErrorCode.SessionRevoked]: HttpStatus.UNAUTHORIZED,
  [AuthErrorCode.Unauthenticated]: HttpStatus.UNAUTHORIZED,
  [AuthErrorCode.EmailTaken]: HttpStatus.CONFLICT,
  [AuthErrorCode.GuestUpgradeCollision]: HttpStatus.CONFLICT,
  [AuthErrorCode.WeakPassword]: HttpStatus.UNPROCESSABLE_ENTITY,
  [AuthErrorCode.RateLimited]: HttpStatus.TOO_MANY_REQUESTS,
  [AuthErrorCode.Forbidden]: HttpStatus.FORBIDDEN,
  [AuthErrorCode.ValidationError]: HttpStatus.BAD_REQUEST,
};

/** Body shape consumed by the mobile client (mirrors `ApiError`). */
export interface ApiErrorBody {
  status: number;
  code: string;
  message: string;
}

export class AuthException extends HttpException {
  constructor(
    public readonly code: Code,
    message: string,
    /** Optional retry hint for RateLimited responses. */
    public readonly retryAfterSeconds?: number,
  ) {
    const status = STATUS_BY_CODE[code] ?? HttpStatus.UNAUTHORIZED;
    const body: ApiErrorBody = { status, code, message };
    if (retryAfterSeconds !== undefined) {
      (body as ApiErrorBody & { retryAfter?: number }).retryAfter = retryAfterSeconds;
    }
    // Sanitize: never leak tokens/credentials into the message.
    super({ ...body, message: sanitize(message) }, status);
  }
}

const SECRET_PATTERN =
  /(ey[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}|argon2[^"'\s]*|Bearer\s+\S+|password["'=:\s]+\S+)/gi;

/** Strips anything that looks like a token, argon2 hash, or password. */
export function sanitize(message: string): string {
  return message.replace(SECRET_PATTERN, '[redacted]');
}
