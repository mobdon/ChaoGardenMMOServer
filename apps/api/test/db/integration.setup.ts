/**
 * Integration-test harness: provisions a disposable per-run namespace inside
 * the integration database.
 *
 * Strategy: tests connect to `INTEGRATION_DATABASE_URL`. If the URL has no
 * explicit schema, we generate one (`itest_<random>`) per Jest worker and
 * point Prisma at it via `?schema=`. Migrations are applied with
 * `prisma migrate deploy` against that schema's database by CI (see
 * scripts). At teardown, the schema (and all its tables) is dropped.
 *
 * Run with: DATABASE_URL=postgres://... npm run test:integration
 */
import { PrismaClient } from '@prisma/client';
import { randomBytes } from 'crypto';

const BASE_URL = process.env.INTEGRATION_DATABASE_URL ?? process.env.DATABASE_URL;

export const INTEGRATION_ENABLED = Boolean(BASE_URL);

let schemaName: string | null = null;

export function integrationSchema(): string {
  if (!schemaName) {
    schemaName = `itest_${randomBytes(6).toString('hex')}`;
  }
  return schemaName;
}

/** A Prisma client scoped to this run's disposable schema, or null if off. */
export function createTestClient(): PrismaClient | null {
  if (!BASE_URL) return null;
  const url = new URL(BASE_URL);
  url.searchParams.set('schema', integrationSchema());
  return new PrismaClient({ datasources: { db: { url: url.toString() } } });
}

export const SCHEMA_PREFIX = 'itest_';
