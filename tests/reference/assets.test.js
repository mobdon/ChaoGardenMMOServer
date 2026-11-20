import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFile, mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import http from 'node:http';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const root = fileURLToPath(new URL('../../', import.meta.url));
const filenames = [
  ...['chao', 'hub', 'garden', 'hero-garden', 'dark-garden', 'neutral-chaos', 'hero-chaos', 'dark-chaos', 'tails', 'knuckles', 'amy', 'chaos-emerald', 'tree-neutral', 'dark-chao-walker', 'dark-walker-shot', 'dark-walker-booster', 'shadow-air-shoes-left', 'shadow-air-shoes-right', 'knuckles-hammer-gloves-left', 'knuckles-hammer-gloves-right', 'chao-behaviour'].map((name) => `${name}.glb`),
  ...['red', 'purple', 'cyan', 'green', 'blue', 'yellow', 'white'].map((color) => `emerald-${color}.glb`),
  'chao-flame.png', 'chao-shadow.png', 'emotion-flame.png', 'emotion-flame-atlas.png', 'emotion-sphere.png',
  'chao-stat-panel.png', 'chao-stat-font.png', 'chao-race-parameter.png', 'discord-icon.png', 'login-chao.png',
  'tiny-neutral-chaos.png', 'tiny-hero-chaos.png', 'tiny-dark-chaos.png',
  'shadow-skate-motion.json',
  ...['race-normal', 'race-dark', 'race-hero', 'karate-arena'].map(name => `${name}.glb`),
  ...['red', 'purple', 'sky', 'cyan', 'green', 'yellow', 'white', 'blue'].map((color) => `emerald-${color}.png`),
  'hub-grass.png',
  ...['music-hub', 'music-garden', 'music-hero', 'music-dark', 'voice-knuckles', 'voice-tails', 'voice-amy'].map((name) => `${name}.mp3`),
];
const diskName = name => name === 'hub-grass.png' ? 'nativehubgrass.png'
  : /^(race-(normal|dark|hero)|karate-arena)\.glb$/.test(name) ? join('activities', name) : name;

function request(port, path, { method = 'GET', origin, range } = {}) {
  return new Promise((resolve, reject) => {
    const pending = http.request({
      host: '127.0.0.1', port, path, method,
      headers: { ...(origin ? { Origin: origin } : {}), ...(range ? { Range: range } : {}) },
    }, (response) => {
      const chunks = [];
      response.on('data', (chunk) => chunks.push(chunk));
      response.on('end', () => resolve({ status: response.statusCode, headers: response.headers, body: Buffer.concat(chunks) }));
    });
    pending.on('error', reject); pending.end();
  });
}

test('local asset endpoint serves exact allowlisted files and rejects path traversal/origin escape', { timeout: 15000 }, async () => {
  // Synthetic bytes in an isolated directory; no game installation or live profiles.
  const dataDir = await mkdtemp(join(tmpdir(), 'chao-reference-assets-'));
  await mkdir(join(dataDir, 'imports'));
  for (const name of filenames) {
    const filename = join(dataDir, 'imports', diskName(name));
    await mkdir(join(filename, '..'), { recursive: true });
    await writeFile(filename, Buffer.alloc(256, 17));
  }
  await writeFile(join(dataDir, 'imports', 'chao-shadow-source.prs'), 'private native source fixture');
  await mkdir(join(dataDir,'activities'));
  await writeFile(join(dataDir,'activities','races.json'),JSON.stringify({courses:[{id:'fixture-course',name:'Fixture course',group:'test',available:false,source:{evidence:'fixture-source'}}]}));
  const child = spawn(process.execPath, ['--input-type=module', '-e',
    "const {server}=await import('./index.js'); server.on('listening',()=>console.log('TEST_READY '+server.address().port));"], {
    cwd: root, env: { ...process.env, GAME_PORT: '0', GAME_HOST: '127.0.0.1', CHAO_DATA_DIR: dataDir, CHAO_OWNER_BOOTSTRAP: '0', CHAO_ALLOWED_ORIGINS: '' }, stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  let output = ''; let errors = '';
  child.stderr.on('data', (chunk) => { errors += chunk; });
  try {
    const port = await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error(`Asset server did not become ready: ${errors}`)), 8000);
      child.on('error', (error) => { clearTimeout(timeout); reject(error); });
      child.on('exit', (code) => { clearTimeout(timeout); reject(new Error(`Asset server exited ${code}: ${errors}`)); });
      child.stdout.on('data', (chunk) => {
        output += chunk;
        const match = output.match(/TEST_READY (\d+)/);
        if (match) { clearTimeout(timeout); resolve(Number(match[1])); }
      });
    });
    const catalog = await request(port,'/api/activities');
    assert.equal(catalog.status,200);
    assert.equal(JSON.parse(catalog.body).raceCourses[0].source.evidence,'fixture-source');
    assert.equal(JSON.parse(catalog.body).raceCourses[0].available,false);
    for (const name of filenames) {
      let local;
      const filename = diskName(name);
      try { local = await readFile(join(dataDir, 'imports', filename)); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
      const get = await request(port, `/api/assets/${name}`);
      const head = await request(port, `/api/assets/${name}`, { method: 'HEAD' });
      assert.equal(get.status, local ? 200 : 404, name);
      assert.equal(head.status, get.status, name);
      assert.equal(head.body.length, 0, name);
      if (local) {
        assert.equal(get.headers['content-type'], name.endsWith('.mp3') ? 'audio/mpeg' : name.endsWith('.png') ? 'image/png' : name.endsWith('.json') ? 'application/json; charset=utf-8' : 'model/gltf-binary');
        assert.equal(get.headers['cache-control'], 'no-store');
        assert.equal(Number(get.headers['content-length']), local.length);
        assert.equal(Number(head.headers['content-length']), local.length);
        assert.deepEqual(get.body, local);
        if (name.endsWith('.mp3')) {
          const partial = await request(port, `/api/assets/${name}`, { range: 'bytes=0-63' });
          assert.equal(partial.status, 206); assert.equal(partial.headers['accept-ranges'], 'bytes');
          assert.equal(partial.headers['content-range'], `bytes 0-63/${local.length}`);
          assert.deepEqual(partial.body, local.subarray(0, 64));
          assert.equal((await request(port, `/api/assets/${name}`, { range: 'bytes=99999999999-' })).status, 416);
        }
      } else {
        assert.equal(get.headers['content-type'], 'application/json; charset=utf-8');
        assert.equal(JSON.parse(get.body).error, 'Local asset not found.');
      }
    }
    await rm(join(dataDir, 'imports', 'chao-shadow.png'));
    for (const method of ['GET', 'HEAD']) {
      const missing = await request(port, '/api/assets/chao-shadow.png', { method });
      assert.equal(missing.status, 404);
      assert.equal(missing.headers['content-type'], 'application/json; charset=utf-8');
      if (method === 'HEAD') assert.equal(missing.body.length, 0);
      else assert.equal(JSON.parse(missing.body).error, 'Local asset not found.');
    }
    for (const path of [
      '/api/assets/unknown.glb', '/api/assets/../../profiles.json',
      '/api/assets/%2e%2e/%2e%2e/profiles.json', '/api/assets/../assets/chao.glb',
      '/api/assets/%2e%2e/assets/chao.glb', '/api/assets/chao.glb/../../profiles.json',
      '/api/assets/activities/race-normal.glb', '/api/assets/activities/../chao.glb',
      '/api/assets/unknown.json', '/api/assets/shadow-skate-motion.json/../profiles.json',
      '/api/assets/chao-shadow-source.prs', '/api/assets/chao-shadow.json',
      '/api/assets/chao-shadow.png.json', '/api/assets/CHAO-SHADOW.PNG',
      '/api/assets/imports/chao-shadow.png', '/api/assets/chao-shadow.png/../profiles.json',
    ]) assert.equal((await request(port, path)).status, 404, path);
    assert.equal((await request(port, '/api/assets/chao.glb', { origin: 'https://example.com' })).status, 403);
  } finally {
    const exited = new Promise((resolve) => child.once('exit', resolve));
    if (child.exitCode === null) { child.kill(); await exited; }
    await rm(dataDir, { recursive: true, force: true });
  }
});
