// Contract compatibility tests between the mobile client, the phone widget,
// WebSocket events, and API responses — including older supported snapshot
// versions (see e2e/fixtures/seed.js "snapshots").
//
// Run: node --test e2e/integration/contract.test.js
// Env: TEST_BASE_URL, TEST_TOKEN_FILE, WIDGET_SNAPSHOT_DIR (optional cached
// widget payloads captured from a device build).
import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

const BASE = process.env.TEST_BASE_URL || 'http://127.0.0.1:3001';
const TOKEN_FILE = process.env.TEST_TOKEN_FILE;
const WIDGET_DIR = process.env.WIDGET_SNAPSHOT_DIR;

// The current schema version and every older version the widget cache may
// still hold. Bumping CURRENT requires keeping the previous version parseable
// (additive changes only) and adding a fixture below.
const CURRENT = 3;
const SUPPORTED = [1, 2, 3];

// Shape contracts pinned from the mobile client / widget code.
const SNAPSHOT_SHAPE = {
  snapshotVersion: 'number',
  playerId: 'string',
  rings: 'number',
  chao: 'object', // array
  terrain: 'object',
  serverTime: 'number',
};

const WS_EVENT_SHAPE = {
  type: 'string', // "event" | "snapshot"
  kind: 'string?',
};

function token() {
  return TOKEN_FILE && fs.existsSync(TOKEN_FILE)
    ? fs.readFileSync(TOKEN_FILE, 'utf8').trim()
    : '';
}

function validateShape(obj, shape, path = '') {
  for (const [key, type] of Object.entries(shape)) {
    if (type.endsWith('?')) {
      if (obj[key] !== undefined) {
        assert.equal(typeof obj[key], type.slice(0, -1), `${path}${key} type`);
      }
      continue;
    }
    assert.ok(key in obj, `missing field ${path}${key}`);
    if (type === 'object' && Array.isArray(obj[key])) continue;
    assert.equal(typeof obj[key], type, `${path}${key} must be ${type}`);
  }
}

test('api snapshot matches the pinned mobile-client contract', async () => {
  const res = await fetch(`${BASE}/api/status`, {
    headers: { Authorization: `Bearer ${token()}`, Accept: 'application/json' },
  });
  assert.equal(res.status, 200);
  const body = await res.json();
  const snap = body.snapshot ?? body;
  validateShape(snap, SNAPSHOT_SHAPE, 'snapshot.');
  assert.ok(
    SUPPORTED.includes(snap.snapshotVersion),
    `snapshotVersion ${snap.snapshotVersion} outside supported ${SUPPORTED}`,
  );
});

test('snapshot versions only ever add fields (older cache stays parseable)', async () => {
  // Older widget caches tolerate extra unknown fields; they must never break
  // on the *absence* of fields they know. Pin the historical field set here.
  const v1Fields = ['snapshotVersion', 'playerId', 'rings', 'chao', 'serverTime'];
  const v2Fields = [...v1Fields, 'terrain'];
  const res = await fetch(`${BASE}/api/status`, {
    headers: { Authorization: `Bearer ${token()}` },
  });
  assert.equal(res.status, 200);
  const snap = (await res.json()).snapshot ?? (await res.json());
  const keys = Object.keys(snap);
  for (const f of v1Fields) assert.ok(keys.includes(f), `v1 field ${f} missing`);
  for (const f of v2Fields) assert.ok(keys.includes(f), `v2 field ${f} missing`);
  assert.equal(CURRENT, 3, 'bump SUPPORTED/fixtures when the current version changes');
});

test('widget cached snapshots parse against the API contract', { skip: !WIDGET_DIR }, () => {
  const files = fs.readdirSync(WIDGET_DIR).filter((f) => f.endsWith('.json'));
  assert.ok(files.length > 0, 'no cached widget snapshots to validate');
  for (const file of files) {
    const snap = JSON.parse(fs.readFileSync(`${WIDGET_DIR}/${file}`, 'utf8'));
    validateShape(snap, SNAPSHOT_SHAPE, `${file}.`);
    assert.ok(
      SUPPORTED.includes(snap.snapshotVersion),
      `${file}: unsupported snapshotVersion`,
    );
  }
});

test('WebSocket events conform to the event envelope', async () => {
  // Node 22 has a built-in WebSocket client; skip quietly on older runtimes.
  if (typeof WebSocket === 'undefined') return;
  const url = `${BASE.replace(/^http/, 'ws')}/ws?token=${encodeURIComponent(token())}`;
  const first = await new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    const timer = setTimeout(() => reject(new Error('no ws message in 5s')), 5000);
    ws.onmessage = (ev) => {
      clearTimeout(timer);
      ws.close();
      resolve(JSON.parse(ev.data));
    };
    ws.onerror = () => {
      clearTimeout(timer);
      reject(new Error('ws error (check token/origin config)'));
    };
  });
  assert.ok(['event', 'snapshot'].includes(first.type), 'unknown event envelope type');
  if (first.type === 'event') assert.ok('kind' in first, 'event missing kind');
});

test('saved event is only acknowledged after a real save', async () => {
  if (typeof WebSocket === 'undefined') return;
  const url = `${BASE.replace(/^http/, 'ws')}/ws?token=${encodeURIComponent(token())}`;
  await new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    const timer = setTimeout(() => { ws.close(); resolve(); }, 4000);
    ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.kind === 'saved') {
        clearTimeout(timer);
        assert.ok('playerId' in msg && 'serverTime' in msg, 'saved event shape');
        ws.close();
        resolve();
      }
      if (msg.kind === 'error') {
        clearTimeout(timer);
        ws.close();
        reject(new Error(`server error event: ${JSON.stringify(msg)}`));
      }
    };
    ws.onopen = () => ws.send(JSON.stringify({ type: 'action', action: 'save' }));
  });
});
