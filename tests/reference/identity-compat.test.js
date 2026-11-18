import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, writeFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Game } from '../../game.js';

const root = fileURLToPath(new URL('../../', import.meta.url));
const rejection = /Node reference cannot load or save account\/device profiles/;

function legacy() {
  const game = new Game({ now: () => 1000 });
  game.createSession({ name: 'Preserved earnings' });
  const profile = game.exportProfiles()[0];
  profile.rings = 4387;
  return profile;
}

test('reference rejects v2 and malformed identity markers before constructing runtime state', () => {
  const original = legacy();
  for (const marker of [
    { identity: { schemaVersion: 2, devices: [{ id: 'revoked-device', revoked: true }] } },
    { identity: null },
    { identity: {} },
    { schemaVersion: 2 },
    { schemaVersion: 3 },
    { schemaVersion: '2' },
  ]) {
    let clockCalled = false;
    const profile = { ...original, ...marker };
    const before = structuredClone(profile);
    assert.throws(() => new Game({ profiles: [profile], allowOwnerBootstrap: true,
      now: () => { clockCalled = true; return 1000; } }), rejection);
    assert.equal(clockCalled, false);
    assert.deepEqual(profile, before);
  }
  const supported = new Game({ profiles: [original], now: () => 1000 });
  assert.equal(supported.exportProfiles()[0].rings, 4387);
});

test('reference rejects device authority introduced through direct player creation or export', () => {
  const game = new Game({ profiles: [legacy()], now: () => 1000 });
  const player = [...game.players.values()][0];
  assert.throws(() => game.makePlayer({ ...legacy(), identity: { schemaVersion: 2 } }), rejection);
  player.identity = { schemaVersion: 2, devices: [{ id: 'device', revoked: true }] };
  assert.throws(() => game.exportProfiles(), rejection);
  assert.equal(player.identity.devices[0].revoked, true);
});

test('reference process fails before listening or overwriting an account save', { timeout: 10000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'chao-reference-v2-reject-'));
  const profile = { ...legacy(), identity: { schemaVersion: 2, accountId: 'account',
    chaoId: 'canonical-chao', revision: 17, devices: [{ id: 'revoked-device', revoked: true }],
    lease: null, pairing: null } };
  const path = join(directory, 'profiles.json');
  const original = `${JSON.stringify([profile], null, 2)}\n`;
  await writeFile(path, original);
  const child = spawn(process.execPath, ['index.js'], { cwd: root, windowsHide: true,
    env: { ...process.env, CHAO_DATA_DIR: directory, GAME_HOST: '127.0.0.1',
      GAME_PORT: '0', CHAO_OWNER_BOOTSTRAP: '1', CHAO_ALLOWED_ORIGINS: '' },
    stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = ''; let stderr = '';
  child.stdout.on('data', bytes => { stdout += bytes; });
  child.stderr.on('data', bytes => { stderr += bytes; });
  const exited = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', (code, signal) => resolve({ code, signal }));
  });
  const deadline = setTimeout(() => child.kill(), 5000);
  try {
    const result = await exited;
    assert.equal(result.signal, null, `Reference did not exit before its deadline: ${stderr}`);
    assert.equal(result.code, 1);
    assert.match(stderr, rejection);
    assert.doesNotMatch(stdout, /http:\/\//);
    assert.equal(await readFile(path, 'utf8'), original);
    assert.deepEqual(await readdir(directory), ['profiles.json']);
  } finally {
    clearTimeout(deadline);
    if (child.exitCode === null && child.signalCode === null) { child.kill(); await exited; }
    await rm(directory, { recursive: true, force: true });
  }
});
