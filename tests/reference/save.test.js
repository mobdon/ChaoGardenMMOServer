import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, mkdir, rmdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import WebSocket from 'ws';

const root = fileURLToPath(new URL('../../', import.meta.url));
function result(socket, send) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { socket.off('message', handler); reject(new Error('Save acknowledgement timed out.')); }, 4000);
    function handler(bytes) {
      const message = JSON.parse(bytes);
      if (message.kind === 'saved' || (message.type === 'notice' && message.kind === 'error')) {
        clearTimeout(timer); socket.off('message', handler); resolve(message);
      }
    }
    socket.on('message', handler); send();
  });
}
test('reference explicit save acknowledges disk success, rejects client profile data and reports disk failure', { timeout: 12000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'chao-reference-save-'));
  const child = spawn(process.execPath, ['index.js'], { cwd: root, env: { ...process.env, CHAO_DATA_DIR: directory, GAME_HOST: '127.0.0.1', GAME_PORT: '0', CHAO_OWNER_BOOTSTRAP: '1', CHAO_ALLOWED_ORIGINS: '' }, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let output = ''; let errors = ''; let socket;
  child.stderr.on('data', bytes => { errors += bytes; });
  const exited = new Promise(resolve => child.once('exit', resolve));
  try {
    const base = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`Reference startup timed out: ${errors}`)), 4000);
      child.once('error', error => { clearTimeout(timer); reject(error); });
      child.once('exit', code => { clearTimeout(timer); reject(new Error(`Reference exited ${code}: ${errors}`)); });
      child.stdout.on('data', bytes => { output += bytes; const match = output.match(/http:\/\/127\.0\.0\.1:\d+/); if (match) { clearTimeout(timer); resolve(match[0]); } });
    });
    const session = await (await fetch(`${base}/api/session`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: 'Saved owner' }) })).json();
    const invalidResume = await fetch(`${base}/api/session`, { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({token:'unknown',resumeOnly:true}) });
    assert.equal(invalidResume.status,404);
    const resumed = await (await fetch(`${base}/api/session`, { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({token:session.token,resumeOnly:true}) })).json();
    assert.equal(resumed.player.id,session.player.id);
    assert.equal(JSON.parse(await readFile(join(directory,'profiles.json'),'utf8')).length,1);
    socket = new WebSocket(`${base.replace('http:', 'ws:')}/ws?token=${session.token}`);
    await new Promise((resolve, reject) => { socket.once('open', resolve); socket.once('error', reject); });
    const saved = await result(socket, () => socket.send(JSON.stringify({ type: 'action', action: 'save', playerId: 'forged', rings: 999999, profiles: [] })));
    assert.equal(saved.kind, 'saved'); assert.equal(saved.playerId, session.player.id);
    const profiles = JSON.parse(await readFile(join(directory, 'profiles.json'), 'utf8'));
    assert.equal(profiles[0].rings, 20); assert.equal(profiles[0].admin, true); assert.equal(profiles[0].name, 'Saved owner');
    await new Promise(resolve => setTimeout(resolve, 350));
    const blocked = join(directory, 'profiles.json.tmp'); await mkdir(blocked);
    const failed = await result(socket, () => socket.send(JSON.stringify({ type: 'action', action: 'save' })));
    assert.equal(failed.type, 'notice'); assert.equal(failed.kind, 'error'); assert.match(failed.text, /Could not save/);
    await rmdir(blocked); await new Promise(resolve => setTimeout(resolve, 350));
    assert.equal((await result(socket, () => socket.send(JSON.stringify({ type: 'action', action: 'save' })))).kind, 'saved');
  } finally {
    socket?.terminate(); if (child.exitCode === null) { child.kill(); await exited; }
    await rm(directory, { recursive: true, force: true });
  }
});
