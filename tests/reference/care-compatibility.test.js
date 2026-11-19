import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, readFile, writeFile, access, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));

for (const [label, care] of [['v1', { version: 1, inventory: { 'garden-fruit': 2 } }], ['unknown version', { version: 999 }], ['null', null]]) {
  test(`Node reference rejects ${label} Rust care without rewriting profiles`, async () => {
    const directory = await mkdtemp(join(tmpdir(), 'chao-care-compatibility-'));
    const profilePath = join(directory, 'profiles.json');
    const original = Buffer.from(JSON.stringify([
      { id: 'legacy-first', tokenHash: 'legacy-hash', rings: 20 },
      { id: 'care-owner', tokenHash: 'owner-hash', admin: true, rings: 123, care, futureField: { preserve: true } },
    ], null, 2) + '\n');
    try {
      await writeFile(profilePath, original);
      const result = spawnSync(process.execPath, ['index.js'], {
        cwd: root,
        env: { ...process.env, CHAO_DATA_DIR: directory, GAME_HOST: '127.0.0.1', GAME_PORT: '0', CHAO_OWNER_BOOTSTRAP: '', CHAO_ALLOWED_ORIGINS: '' },
        windowsHide: true,
        timeout: 5000,
        encoding: 'utf8',
      });
      assert.ifError(result.error);
      assert.equal(result.status, 1);
      assert.match(result.stderr, /Rust care profiles are unsupported by the Node reference/);
      assert.doesNotMatch(result.stdout, /Node reference server:/);
      assert.deepEqual(await readFile(profilePath), original);
      await assert.rejects(access(join(directory, 'profiles.json.tmp')), { code: 'ENOENT' });
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
}
