import http from 'node:http';
import { readFileSync, writeFileSync, mkdirSync, renameSync, existsSync, openSync, fsyncSync, closeSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isIP } from 'node:net';
import { WebSocketServer, WebSocket } from 'ws';
import { Game } from './game.js';
import { publicActivities } from './activities.js';

const serverRoot = dirname(fileURLToPath(import.meta.url));
const dataDir = resolve(process.env.CHAO_DATA_DIR || resolve(serverRoot, 'data'));
const profilePath = resolve(dataDir, 'profiles.json');
const host = process.env.GAME_HOST || '127.0.0.1';
if (!isIP(host)) throw new Error('GAME_HOST must be an IP address.');
const loopback = host === '::1' || host.startsWith('127.');
const ownerBootstrap = ['1', 'true'].includes(process.env.CHAO_OWNER_BOOTSTRAP);
if (ownerBootstrap && !loopback) throw new Error('Owner bootstrap requires a loopback listener.');
const allowedOrigins = (process.env.CHAO_ALLOWED_ORIGINS || '').split(',').map((value) => value.trim()).filter(Boolean).map((value) => {
  const parsed = new URL(value);
  if (!['http:', 'https:'].includes(parsed.protocol)) throw new Error('Configured origins must be HTTP(S).');
  return parsed.origin;
});
const localAssetRoutes = new Map(
  [
    ...['chao', 'hub', 'garden', 'hero-garden', 'dark-garden', 'neutral-chaos', 'hero-chaos', 'dark-chaos', 'tails', 'knuckles', 'amy', 'chaos-emerald', 'tree-neutral', 'dark-chao-walker', 'dark-walker-shot', 'dark-walker-booster', 'shadow-air-shoes-left', 'shadow-air-shoes-right', 'knuckles-hammer-gloves-left', 'knuckles-hammer-gloves-right'].map((name) => `${name}.glb`),
    ...['red', 'purple', 'cyan', 'green', 'blue', 'yellow', 'white'].map((color) => `emerald-${color}.glb`),
    'chao-flame.png', 'emotion-flame.png', 'emotion-flame-atlas.png', 'emotion-sphere.png',
    'chao-stat-panel.png', 'chao-stat-font.png', 'chao-race-parameter.png', 'discord-icon.png', 'login-chao.png',
    'tiny-neutral-chaos.png', 'tiny-hero-chaos.png', 'tiny-dark-chaos.png',
    'shadow-skate-motion.json',
    ...['red', 'purple', 'sky', 'cyan', 'green', 'yellow', 'white', 'blue'].map((color) => `emerald-${color}.png`),
    'hub-grass.png',
    ...['music-hub', 'music-garden', 'music-hero', 'music-dark', 'voice-knuckles', 'voice-tails', 'voice-amy'].map((name) => `${name}.mp3`),
  ].map((filename) => [`/api/assets/${filename}`, resolve(dataDir, 'imports', filename)]),
);
localAssetRoutes.set('/api/assets/hub-grass.png', resolve(dataDir, 'imports', 'nativehubgrass.png'));
for (const name of ['race-normal', 'race-dark', 'race-hero', 'karate-arena']) {
  localAssetRoutes.set(`/api/assets/${name}.glb`, resolve(dataDir, 'imports', 'activities', `${name}.glb`));
}
let profiles = [];
try {
  if (existsSync(profilePath)) {
    const parsed = JSON.parse(readFileSync(profilePath, 'utf8'));
    if (!Array.isArray(parsed)) throw new Error('Profiles must be an array.');
    if (parsed.some(profile => profile !== null && typeof profile === 'object' && Object.hasOwn(profile, 'care'))) {
      throw new Error('Rust care profiles are unsupported by the Node reference. Use the Rust authority to preserve care state.');
    }
    profiles = parsed;
  }
} catch (error) {
  // Do not silently overwrite persisted profiles or bootstrap another owner.
  console.error(`Cannot read ${profilePath}: ${error.message}`);
  process.exit(1);
}

const terrains = {};
for (const zone of ['hub', 'garden', 'hero', 'dark']) {
  const terrainPath = resolve(dataDir, 'terrain', `${zone}.json`);
  if (existsSync(terrainPath)) terrains[zone] = JSON.parse(readFileSync(terrainPath, 'utf8'));
}
function readCatalog(name) { const filename=resolve(dataDir,'activities',`${name}.json`); return existsSync(filename)?JSON.parse(readFileSync(filename,'utf8')):{}; }
const raceCatalog = readCatalog('races');
const karateCatalog = readCatalog('karate');
const activityTerrains = {};
for (const id of new Set((raceCatalog.courses || []).map(course => course.worldId || course.id))) {
  if (typeof id !== 'string' || !/^[a-z0-9-]{1,64}$/.test(id)) throw new Error('Invalid activity world ID.');
  const filename = resolve(dataDir, 'activities', 'terrain', `${id}.json`);
  if (existsSync(filename)) activityTerrains[id] = JSON.parse(readFileSync(filename, 'utf8'));
}
export const game = new Game({ profiles, terrains, activityTerrains, raceCatalog, karateCatalog, allowOwnerBootstrap: ownerBootstrap });
const clients = new Map();

function persist() {
  if (!game.dirty) return;
  mkdirSync(dirname(profilePath), { recursive: true });
  const temporary = `${profilePath}.tmp`;
  const descriptor = openSync(temporary, 'w', 0o600);
  try { writeFileSync(descriptor, JSON.stringify(game.exportProfiles(), null, 2)); fsyncSync(descriptor); }
  finally { closeSync(descriptor); }
  renameSync(temporary, profilePath);
  game.dirty = false;
}

function json(response, status, data) {
  response.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
  response.end(JSON.stringify(data));
}

function localOrigin(request) {
  if (!request.headers.origin) return true;
  try {
    const origin = new URL(request.headers.origin);
    return ['http:', 'https:'].includes(origin.protocol) && (allowedOrigins.includes(origin.origin)
      || (loopback && ['localhost', '127.0.0.1', '[::1]'].includes(origin.hostname)));
  } catch { return false; }
}

async function detectGameFiles() {
  let inventory;
  try { inventory = JSON.parse(await readFile(resolve(dataDir, 'game-inventory.json'), 'utf8')); }
  catch { inventory = { found: false, note: 'No cached game inventory is available. Run the client inventory and set CHAO_DATA_DIR.' }; }
  inventory.imports = { directory: resolve(dataDir, 'imports'),
    chao: existsSync(resolve(dataDir, 'imports', 'chao.glb')), garden: existsSync(resolve(dataDir, 'imports', 'garden.glb')) };
  return inventory;
}

export const server = http.createServer(async (request, response) => {
  if (!localOrigin(request)) return json(response, 403, { error: 'Only local development origins are allowed.' });
  let url;
  try { url = new URL(request.url, 'http://127.0.0.1'); }
  catch { return json(response, 400, { error: 'Invalid request URL.' }); }
  // Match literal allowlisted routes before any URL normalization or decoding.
  // No user-supplied path component is ever passed to the filesystem.
  const localAsset = localAssetRoutes.get(request.url.split('?')[0]);
  if (localAsset && (request.method === 'GET' || request.method === 'HEAD')) {
    try {
      const contents = await readFile(localAsset);
      const isAudio = localAsset.endsWith('.mp3');
      const headers = {
        'Content-Type': isAudio ? 'audio/mpeg' : localAsset.endsWith('.png') ? 'image/png' : localAsset.endsWith('.json') ? 'application/json; charset=utf-8' : 'model/gltf-binary',
        'Content-Length': contents.length,
        'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff',
        ...(isAudio ? { 'Accept-Ranges': 'bytes' } : {}),
      };
      let status = 200; let bytes = contents;
      if (isAudio && request.headers.range) {
        const match = /^bytes=(\d*)-(\d*)$/.exec(request.headers.range);
        let start; let end;
        if (match && (match[1] || match[2])) {
          start = match[1] ? Number(match[1]) : Math.max(0, contents.length - Number(match[2]));
          end = match[1] && match[2] ? Math.min(Number(match[2]), contents.length - 1) : contents.length - 1;
        }
        if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || start >= contents.length || end < start) {
          response.writeHead(416, { ...headers, 'Content-Length': 0, 'Content-Range': `bytes */${contents.length}` });
          return response.end();
        }
        bytes = contents.subarray(start, end + 1); status = 206;
        headers['Content-Length'] = bytes.length; headers['Content-Range'] = `bytes ${start}-${end}/${contents.length}`;
      }
      response.writeHead(status, headers);
      return response.end(request.method === 'HEAD' ? undefined : bytes);
    } catch (error) {
      return json(response, ['ENOENT', 'ENOTDIR'].includes(error.code) ? 404 : 500,
        { error: ['ENOENT', 'ENOTDIR'].includes(error.code) ? 'Local asset not found.' : 'Could not read local asset.' });
    }
  }
  if (request.method === 'GET' && url.pathname === '/api/status') {
    let setup;
    try { setup = await detectGameFiles(); } catch { setup = { available: false, message: 'Game-file detection is unavailable; placeholder assets remain playable.' }; }
    return json(response, 200, { ok: true, mode: 'local-development', online: clients.size, setup,
      terrain: { loaded: [...game.terrains.keys()], flatFallback: ['hub', 'garden', 'hero', 'dark'].filter((zone) => !game.terrains.has(zone)) } });
  }
  if (request.method === 'POST' && url.pathname === '/api/session') {
    if (!String(request.headers['content-type'] || '').startsWith('application/json')) return json(response, 415, { error: 'Use application/json.' });
    let body = ''; let oversized = false;
    request.on('data', (chunk) => {
      body += chunk;
      if (body.length > 2048) { oversized = true; request.destroy(); }
    });
    request.on('end', () => {
      if (oversized) return;
      try {
        const input = JSON.parse(body || '{}');
        if (!input || typeof input !== 'object' || Array.isArray(input)) return json(response, 400, { error: 'Session data must be a JSON object.' });
        // Deliberately select the only accepted request fields.
        if (input.resumeOnly === true && !game.playerForToken(input.token)) return json(response, 404, { error: 'Saved Chao was not found.' });
        const session = game.createSession({ name: input.name, token: input.token, resumeOnly: input.resumeOnly });
        persist();
        json(response, 200, session);
      } catch (error) {
        if (error instanceof SyntaxError) return json(response, 400, { error: 'Invalid JSON.' });
        console.error(`Session persistence failed: ${error.message}`);
        json(response, 500, { error: 'Could not save your session.' });
      }
    });
    return;
  }
  if (request.method === 'GET' && url.pathname === '/api/activities') return json(response, 200, publicActivities(game, true));
  json(response, 404, { error: 'Not found.' });
});

const wss = new WebSocketServer({ noServer: true, maxPayload: 2048 });
server.on('upgrade', (request, socket, head) => {
  let url;
  try { url = new URL(request.url, 'http://127.0.0.1'); }
  catch { socket.destroy(); return; }
  const player = game.playerForToken(url.searchParams.get('token'));
  if (url.pathname !== '/ws' || !player || !localOrigin(request)) {
    socket.write('HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n'); socket.destroy(); return;
  }
  wss.handleUpgrade(request, socket, head, (ws) => wss.emit('connection', ws, request, player.id));
});

wss.on('connection', (ws, _request, id) => {
  clients.set(ws, id); game.connect(id); ws.isAlive = true;
  let windowStart = Date.now(); let messages = 0;
  ws.on('pong', () => { ws.isAlive = true; });
  ws.on('message', (raw) => {
    if (Date.now() - windowStart >= 1000) { windowStart = Date.now(); messages = 0; }
    if (++messages > 80) { ws.close(1008, 'Message rate exceeded.'); return; }
    let message;
    try { message = JSON.parse(raw.toString()); } catch { return; }
    if (!message || typeof message !== 'object') return;
    if (message.type === 'input') game.setInput(id, message);
    if (message.type === 'action' && message.action === 'save') {
      if (!game.requestSave(id)) return ws.send(JSON.stringify({ type: 'notice', kind: 'error', text: 'Please wait a moment before saving again.' }));
      try {
        persist(); ws.send(JSON.stringify({ type: 'event', kind: 'saved', playerId: id, serverTime: game.time }));
      } catch (error) {
        console.error(`Explicit save failed: ${error.message}`);
        ws.send(JSON.stringify({ type: 'notice', kind: 'error', text: 'Could not save your progress. Stay in the garden and try again.' }));
      }
      return;
    }
    if (message.type === 'action' && typeof message.action==='string' && message.action.startsWith('watch-')) {
      const previous=structuredClone(game.players.get(id));const wasDirty=game.dirty;const eventCount=game.events.length;
      game.action(id,message);
      try {persist();const player=game.players.get(id);const watch=game.publicPlayer(player).watch;const before=game.publicPlayer(previous).watch;const accepted=JSON.stringify(before)!==JSON.stringify(watch)||previous.happiness!==player.happiness;ws.send(JSON.stringify({type:'event',kind:'watch-saved',accepted,watch,serverTime:game.time}));}
      catch(error){game.players.set(id,previous);game.dirty=wasDirty;game.events.length=eventCount;console.error(`Could not save resident choice: ${error.message}`);ws.send(JSON.stringify({type:'notice',kind:'error',text:'Your resident choice could not save. Your previous mode was kept; please retry.'}));}
      return;
    }
    if (message.type === 'action') game.action(id, message);
  });
  ws.on('close', () => { clients.delete(ws); game.disconnect(id); });
  ws.on('error', () => {});
  ws.send(JSON.stringify(game.snapshot(id)));
  game.notice(id, game.players.get(id).admin
    ? 'Welcome, owner! Your admin Chao has all S grades, level 99, and max hidden stats.'
    : 'Welcome to the garden! Move with WASD and explore with other Chao.');
});

const simulation = setInterval(() => game.tick(0.05), 50);
const snapshots = setInterval(() => {
  const events = game.drainEvents();
  for (const [ws, id] of clients) {
    if (ws.readyState !== WebSocket.OPEN || ws.bufferedAmount > 128 * 1024) continue;
    for (const event of events) if (!event.target || event.target === id) ws.send(JSON.stringify(event.message));
    ws.send(JSON.stringify(game.snapshot(id)));
  }
}, 100);
const saving = setInterval(() => {
  try { persist(); } catch (error) { console.error(`Could not save profiles: ${error.message}`); }
}, 5000);
const heartbeat = setInterval(() => {
  for (const ws of clients.keys()) {
    if (!ws.isAlive) { ws.terminate(); continue; }
    ws.isAlive = false; ws.ping();
  }
}, 20000);

const port = Number(process.env.GAME_PORT || 3001);
server.listen(port, host, () => console.log(`Chao Node reference server: http://${host}:${server.address().port}`));
server.on('error', (error) => { console.error(error.message); process.exitCode = 1; shutdown(); });

let stopping = false;
function shutdown() {
  if (stopping) return; stopping = true;
  clearInterval(simulation); clearInterval(snapshots); clearInterval(saving); clearInterval(heartbeat);
  try { persist(); } catch (error) { console.error(`Final profile save failed: ${error.message}`); }
  for (const ws of clients.keys()) ws.terminate();
  wss.close(); server.close();
}
process.on('SIGINT', shutdown); process.on('SIGTERM', shutdown);
