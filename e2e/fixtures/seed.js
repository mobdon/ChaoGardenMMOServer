#!/usr/bin/env node
// Deterministic seed fixtures for the release test suites. Every profile,
// token hash, and inventory value is derived from fixed strings (no Date.now,
// no Math.random), so runs are reproducible and diffs are stable.
//
// Usage:
//   node e2e/fixtures/seed.js                  # writes into TEST_DATA_DIR
//   node e2e/fixtures/seed.js --print          # prints the fixture plan as JSON
// Requires TEST_DATA_DIR to point at an empty disposable directory unless
// --print. Never targets a live/owner data directory (see reset-test-state.sh).

import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const FIXTURE_NAME = 'release-suite-v1';

function tokenHash(token) {
  // Matches the server's profile storage: SHA-256 of the raw bearer token.
  return createHash('sha256').update(token, 'utf8').digest('hex');
}

const fixtures = {
  name: FIXTURE_NAME,
  profiles: [
    {
      id: 'seed-owner',
      name: 'SeedOwner',
      // Deterministic dev-only token; never used against staging/production.
      token: 'test-token-owner-0001',
      role: 'owner',
      rings: 5000,
      chao: [{ id: 'seed-chao-1', name: 'SeedChao', stage: 'child', level: 5 }],
      gear: { footwear: null, hands: null },
      inventory: { 'round-tree': 2, fruit: 5 },
      decorations: [{ slot: 3, item: 'round-tree' }],
    },
    {
      id: 'seed-friend',
      name: 'TestFriend',
      token: 'test-token-friend-0002',
      role: 'resident',
      rings: 500,
      chao: [{ id: 'seed-chao-2', name: 'FriendChao', stage: 'child', level: 3 }],
      gear: { footwear: null, hands: null },
      inventory: { 'round-tree': 0, fruit: 2 },
      decorations: [],
    },
    {
      id: 'seed-guest',
      name: 'Guest',
      token: 'test-token-guest-0003',
      role: 'guest',
      rings: 100,
      chao: [],
      gear: { footwear: null, hands: null },
      inventory: { 'round-tree': 0, fruit: 1 },
      decorations: [],
    },
  ],
  // Widget/mobile snapshot compatibility fixtures (see contract.test.js).
  snapshots: {
    current: { snapshotVersion: 3 },
    // Older supported versions that cached widget payloads may still carry.
    supported: [{ snapshotVersion: 1 }, { snapshotVersion: 2 }],
  },
};

function serializeProfiles() {
  return fixtures.profiles.map((p) => ({
    id: p.id,
    name: p.name,
    role: p.role,
    rings: p.rings,
    chao: p.chao,
    gear: p.gear,
    inventory: p.inventory,
    decorations: p.decorations,
    tokenHash: tokenHash(p.token),
  }));
}

function tokens() {
  return Object.fromEntries(fixtures.profiles.map((p) => [p.name, p.token]));
}

if (process.argv.includes('--print')) {
  // Print the plan without writing anything (safe for CI logs; tokens are
  // test-only fixtures, but still scrub them from the printed plan).
  console.log(JSON.stringify({ ...fixtures, tokens: Object.keys(tokens()) }, null, 2));
  process.exit(0);
}

const dataDir = process.env.TEST_DATA_DIR;
if (!dataDir) {
  console.error('TEST_DATA_DIR must point at an empty disposable directory');
  process.exit(2);
}
const abs = path.resolve(dataDir);
if (fs.existsSync(abs)) {
  const entries = fs.readdirSync(abs).filter((f) => f !== '.' && f !== '..');
  if (entries.length > 0 && !fs.existsSync(path.join(abs, '.test-seed'))) {
    console.error(`Refusing to seed non-empty data dir without a test marker: ${abs}`);
    process.exit(2);
  }
} else {
  fs.mkdirSync(abs, { recursive: true });
}

fs.mkdirSync(path.join(abs, 'profiles'), { recursive: true });
fs.mkdirSync(path.join(abs, 'terrain'), { recursive: true });
fs.writeFileSync(
  path.join(abs, 'profiles.json'),
  JSON.stringify({ profiles: serializeProfiles() }, null, 2),
);
fs.writeFileSync(
  path.join(abs, 'terrain', 'garden.json'),
  JSON.stringify({ minX: -10, minZ: -10, step: 1, width: 21, depth: 21, heights: [] }, null, 2),
);
// Test marker lets reset-test-state.sh recognise the directory as disposable.
fs.writeFileSync(path.join(abs, '.test-seed'), `${FIXTURE_NAME}\n`);
// Tokens live outside the data dir so profile snapshots never contain them.
fs.writeFileSync(
  path.join(abs, '.test-seed-tokens'),
  JSON.stringify(tokens(), null, 2),
);

console.log(`Seeded ${FIXTURE_NAME} into ${abs}`);
console.log(JSON.stringify({ profiles: fixtures.profiles.map((p) => p.name) }));
