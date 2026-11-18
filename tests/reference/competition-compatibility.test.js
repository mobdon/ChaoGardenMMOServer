import test from 'node:test';
import assert from 'node:assert/strict';
import { Game } from '../../game.js';

test('Node reference refuses Rust competition and lease profiles before they can lose receipts', () => {
  const original = { id: 'preserved', tokenHash: 'credential-hash', rings: 123, competition: { version: 1, results: [{ id: 'receipt' }] } };
  const before = structuredClone(original);
  assert.throws(() => new Game({ profiles: [original] }), /require the Rust authority/);
  assert.deepEqual(original, before);
  assert.throws(() => new Game({ profiles: [{ id: 'training', tokenHash: 'hash', activityLease: { kind: 'training' } }] }), /require the Rust authority/);
  assert.doesNotThrow(() => new Game({ profiles: [{ id: 'legacy', tokenHash: 'hash', rings: 123 }] }));
});
