// API integration tests for security boundaries. Runs against an isolated
// PostgreSQL + Redis started by e2e/scripts/run-integration.sh (Docker),
// or a local Rust authority started with CHAO_DATA_DIR pointed at a seed dir.
//
// Run: node --test e2e/integration/api.test.js
// Required env:
//   TEST_BASE_URL      http://127.0.0.1:<port>
//   TEST_TOKEN_FILE    path holding a test bearer token
//   TEST_DATABASE_URL  isolated Postgres DSN (checked, never printed)
//   TEST_REDIS_URL     isolated Redis DSN (checked, never printed)
import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

const BASE = process.env.TEST_BASE_URL || 'http://127.0.0.1:3001';
const TOKEN_FILE = process.env.TEST_TOKEN_FILE;
const DB_URL = process.env.TEST_DATABASE_URL;
const REDIS_URL = process.env.TEST_REDIS_URL;

// Never print DSNs or tokens: derive a boolean for diagnostics instead.
const hasToken = Boolean(TOKEN_FILE && fs.existsSync(TOKEN_FILE));
const hasDb = Boolean(DB_URL && DB_URL.includes('postgres'));
const hasRedis = Boolean(REDIS_URL && REDIS_URL.includes('redis'));

function token() {
  return hasToken ? fs.readFileSync(TOKEN_FILE, 'utf8').trim() : '';
}

function authHeaders(extra = {}) {
  const t = token();
  return {
    'Content-Type': 'application/json',
    ...(t ? { Authorization: `Bearer ${t}` } : {}),
    ...extra,
  };
}

function describeEnv() {
  return JSON.stringify({
    base: BASE,
    tokenFile: hasToken ? TOKEN_FILE : null,
    db: hasDb ? 'isolated-postgres-configured' : 'not-configured',
    redis: hasRedis ? 'isolated-redis-configured' : 'not-configured',
  });
}

test('environment reports isolated services without leaking secrets', () => {
  // Fails with safe diagnostics only; DSNs are never echoed.
  assert.ok(hasDb, `isolated Postgres not configured: ${describeEnv()}`);
  assert.ok(hasRedis, `isolated Redis not configured: ${describeEnv()}`);
});

test('session rejects unknown resume token without creating a profile', async () => {
  const res = await fetch(`${BASE}/api/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ resumeOnly: true, token: 'definitely-not-a-token' }),
  });
  assert.equal(res.status, 404, 'resumeOnly must 404 unknown tokens');
});

test('session rejects wrong content type and bad origin', async () => {
  const res = await fetch(`${BASE}/api/session`, {
    method: 'POST',
    headers: { Origin: 'https://evil.example' },
    body: JSON.stringify({}),
  });
  assert.ok([403, 415].includes(res.status), `expected 403/415, got ${res.status}`);
});

test('actions reject client-supplied currency and stats (authority)', async () => {
  const res = await fetch(`${BASE}/api/session`, {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify({ resumeOnly: true, rings: 999999 }),
  });
  // Server must ignore rings: a 200/404 response proves no crash, and the
  // follow-up status check confirms the value was not adopted.
  const status = await fetch(`${BASE}/api/status`, { headers: authHeaders() });
  assert.ok([200, 404].includes(res.status));
  if (status.ok) {
    const body = await status.json();
    assert.ok(!('rings' in (body.me ?? {})) || body.me.rings !== 999999);
  }
});

test('origin guard allows loopback development origins', async () => {
  const res = await fetch(`${BASE}/api/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Origin: 'http://127.0.0.1:5173' },
    body: JSON.stringify({ resumeOnly: true }),
  });
  assert.ok([200, 404].includes(res.status), `loopback origin must be allowed, got ${res.status}`);
});

test('oversized payloads are rejected with 413', async () => {
  const res = await fetch(`${BASE}/api/session`, {
    method: 'POST',
    headers: authHeaders(),
    body: 'x'.repeat(3 * 1024), // over the 2 KiB WS/API payload guard
  });
  assert.equal(res.status, 413, `expected 413, got ${res.status}`);
});

test('status endpoint reports health and never exposes tokens', async () => {
  const res = await fetch(`${BASE}/api/status`, { headers: authHeaders() });
  assert.equal(res.status, 200);
  const text = await res.text();
  assert.ok(!text.includes(token() || '>>no-token<<'), 'response leaked the bearer token');
});
