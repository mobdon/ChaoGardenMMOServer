/**
 * Authentication & account security contracts shared by apps/api and apps/mobile.
 *
 * Every request body is a Zod schema so the server validates input and the
 * client can reuse the exact same shapes. Responses never contain secrets —
 * refresh tokens are delivered exactly once at issuance and stored hashed
 * server-side.
 */
import { z } from 'zod';

/** Stable machine-readable error codes returned by apps/api. */
export const AuthErrorCode = {
  InvalidCredentials: 'AUTH_INVALID_CREDENTIALS',
  EmailTaken: 'AUTH_EMAIL_TAKEN',
  WeakPassword: 'AUTH_WEAK_PASSWORD',
  InvalidToken: 'AUTH_INVALID_TOKEN',
  ExpiredToken: 'AUTH_EXPIRED_TOKEN',
  TokenReplay: 'AUTH_TOKEN_REPLAY',
  SessionRevoked: 'AUTH_SESSION_REVOKED',
  RateLimited: 'AUTH_RATE_LIMITED',
  Forbidden: 'AUTH_FORBIDDEN',
  Unauthenticated: 'AUTH_UNAUTHENTICATED',
  GuestUpgradeCollision: 'AUTH_GUEST_UPGRADE_COLLISION',
  ValidationError: 'AUTH_VALIDATION_ERROR',
} as const;

export type AuthErrorCodeValue = (typeof AuthErrorCode)[keyof typeof AuthErrorCode];

export const authError = z.object({
  status: z.number(),
  code: z.string(),
  message: z.string(),
});
export type AuthError = z.infer<typeof authError>;

// ---------------------------------------------------------------------------
// Request contracts
// ---------------------------------------------------------------------------

export const emailSchema = z.string().trim().toLowerCase().email().max(255);
export const passwordSchema = z.string().min(10, 'Password must be at least 10 characters').max(200);

export const registerSchema = z.object({
  email: emailSchema,
  password: passwordSchema,
  displayName: z.string().trim().min(2).max(40).optional(),
  /** Present when upgrading a guest; binds the guest's progress to the account. */
  guestToken: z.string().optional(),
});
export type RegisterRequest = z.infer<typeof registerSchema>;

export const loginSchema = z.object({
  email: emailSchema,
  password: z.string().min(1).max(200),
  deviceName: z.string().trim().min(1).max(80).default('Unknown device'),
});
export type LoginRequest = z.infer<typeof loginSchema>;

export const refreshSchema = z.object({
  refreshToken: z.string().min(10).max(512),
});
export type RefreshRequest = z.infer<typeof refreshSchema>;

export const logoutSchema = z.object({
  refreshToken: z.string().min(10).max(512),
});

export const guestCreateSchema = z.object({
  deviceName: z.string().trim().min(1).max(80).default('Guest device'),
});
export type GuestCreateRequest = z.infer<typeof guestCreateSchema>;

export const guestUpgradeSchema = z.object({
  email: emailSchema,
  password: passwordSchema,
  displayName: z.string().trim().min(2).max(40).optional(),
  /** Current guest refresh token proving control of the guest session. */
  guestToken: z.string().min(10).max(512),
});
export type GuestUpgradeRequest = z.infer<typeof guestUpgradeSchema>;

// ---------------------------------------------------------------------------
// Response contracts
// ---------------------------------------------------------------------------

export const profileSchema = z.object({
  id: z.string(),
  displayName: z.string(),
  email: z.string().email().nullable(),
  guest: z.boolean(),
});

export const authSessionSchema = z.object({
  accessToken: z.string(),
  /** ISO instant after which the access token expires. */
  accessExpiresAt: z.number(),
  /** Opaque rotating refresh token; sent once, stored hashed server-side. */
  refreshToken: z.string(),
  refreshTokenExpiresAt: z.number(),
  profile: profileSchema,
});
export type AuthSession = z.infer<typeof authSessionSchema>;

export const deviceSessionSchema = z.object({
  id: z.string(),
  deviceName: z.string(),
  createdAt: z.number(),
  lastUsedAt: z.number(),
  current: z.boolean(),
});
export type DeviceSession = z.infer<typeof deviceSessionSchema>;

export const currentUserSchema = z.object({
  profile: profileSchema,
  sessions: z.array(deviceSessionSchema),
});
export type CurrentUser = z.infer<typeof currentUserSchema>;

export const messageSchema = z.object({ ok: z.literal(true) });

/** Auth endpoint catalogue (extends the gameplay endpoint list). */
export const authEndpoints = {
  register: '/api/auth/register',
  login: '/api/auth/login',
  refresh: '/api/auth/refresh',
  logout: '/api/auth/logout',
  logoutAll: '/api/auth/logout-all',
  me: '/api/auth/me',
  guestCreate: '/api/auth/guest',
  guestUpgrade: '/api/auth/guest/upgrade',
  sessions: '/api/auth/sessions',
  revokeSession: (sessionId: string) => `/api/auth/sessions/${sessionId}`,
} as const;

export type Profile = z.infer<typeof profileSchema>;
