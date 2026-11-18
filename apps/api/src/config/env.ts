import { z } from 'zod';

/**
 * Zod-validated environment for the API server. Secrets are loaded from the
 * environment (or apps/api/.env during local development) and are never
 * committed to git.
 */
export const envSchema = z.object({
  NODE_ENV: z.enum(['development', 'test', 'production']).default('development'),
  PORT: z.coerce.number().int().positive().default(3001),
  DATABASE_URL: z
    .string()
    .min(1)
    .default('postgres://postgres:postgres@localhost:5432/chaogarden'),
  REDIS_URL: z.string().default('redis://localhost:6379'),
  JWT_SECRET: z.string().min(32, 'JWT_SECRET must be at least 32 characters'),
  ACCESS_TOKEN_TTL_SECONDS: z.coerce.number().int().positive().default(900),
  REFRESH_TOKEN_TTL_SECONDS: z.coerce.number().int().positive().default(60 * 60 * 24 * 30),
});

export type ApiEnv = z.infer<typeof envSchema>;

/** Parse and validate raw environment values; throws a readable error list. */
export function validateEnv(raw: NodeJS.ProcessEnv = process.env): ApiEnv {
  const parsed = envSchema.safeParse(raw);
  if (!parsed.success) {
    const issues = parsed.error.issues
      .map((issue) => `  ${issue.path.join('.') || '(root)'}: ${issue.message}`)
      .join('\n');
    throw new Error(`Invalid API environment configuration:\n${issues}`);
  }
  return parsed.data;
}
