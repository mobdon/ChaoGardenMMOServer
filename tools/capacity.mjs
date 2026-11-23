// Real Rust protocol, synthetic profiles, temporary data, loopback listener only.
// node tools/capacity.mjs --players 100 --residents 100 --seconds 30 --out results.json
import assert from 'node:assert/strict';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, writeFile, readFile, rm, mkdir } from 'node:fs/promises';
import { tmpdir, cpus, totalmem, platform, release } from 'node:os';
import { resolve, dirname } from 'node:path';
import net from 'node:net';
import { performance } from 'node:perf_hooks';
import { evaluateCapacity } from './capacity-gates.mjs';

const args = new Map();
for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i], process.argv[i + 1]);
const number = (key, fallback, min, max) => {
  const n = Number(args.get(key) ?? fallback);
  assert(Number.isInteger(n) && n >= min && n <= max, `Invalid ${key}`);
  return n;
};
const players = number('--players', 100, 4, 500);
const residents = number('--residents', players, 0, 2000);
const seconds = number('--seconds', 30, 10, 3600);
const slowCount = number('--slow', Math.max(1, Math.floor(players / 50)), 0, players - 2);
const crowded = args.get('--crowded') === 'true';
const executable = resolve(args.get('--server') ?? `target/release/chao-game-server${process.platform === 'win32' ? '.exe' : ''}`);
const output = resolve(args.get('--out') ?? `capacity-${players}-${residents}.json`);
const sleep = ms => new Promise(r => setTimeout(r, ms));
const exec = promisify(execFile);
const percentile = (values, p) => values.length ? [...values].sort((a,b) => a-b)[Math.min(values.length - 1, Math.ceil(values.length * p) - 1)] : null;
const summary = values => ({ count: values.length, p50: percentile(values,.5), p95: percentile(values,.95), p99: percentile(values,.99), max: values.length ? Math.max(...values) : null });
// Bound generator memory during a soak; deterministic reservoir with a fixed cap.
const samples = () => ({ values: [], count: 0, add(n) { this.count++; if (this.values.length < 20000) this.values.push(n); else if (this.count % 13 === 0) this.values[this.count % 20000] = n; } });
const directory = await mkdtemp(resolve(tmpdir(), 'chao-capacity-'));
const profiles = [], identities = [];
for (let i = 0; i < players + residents; i++) {
  const token = randomBytes(32).toString('hex');
  const zone = crowded ? 'garden' : ['hub','garden','hero','dark'][i % 4];
  const residentZone = zone === 'hub' ? 'garden' : zone;
  const p = { id: randomUUID(), tokenHash: createHash('sha256').update(token).digest('hex'), name: `Load ${i}`, admin: false, alignment: zone === 'hero' ? 100 : zone === 'dark' ? -100 : 0, rings: 20 };
  if (i >= players) p.resident = { enabled: true, zone: residentZone, x: 0, z: 6 };
  profiles.push(p);
  if (i < players) identities.push({ token, id: p.id, zone });
}
await writeFile(resolve(directory, 'profiles.json'), JSON.stringify(profiles));
let stderr = '', base, processExit;
const child = spawn(executable, [], { cwd: directory, env: { ...process.env, CHAO_DATA_DIR: directory, GAME_HOST: '127.0.0.1', GAME_PORT: '0', CHAO_OWNER_BOOTSTRAP: '0', CHAO_ALLOWED_ORIGINS: '', CHAO_SHUTDOWN_STDIN: '1' }, stdio: ['pipe','pipe','pipe'], windowsHide: true });
processExit = new Promise(r => child.on('exit', (code, signal) => r({ code, signal })));
child.stderr.on('data', b => { stderr = (stderr + b).slice(-8000); });
const peers = [], raw = [];
const expectedCloses = new WeakSet();
let stopped = false;
async function stop() {
  if (stopped) return;
  stopped = true;
  for (const peer of peers) peer.ws?.close();
  for (const s of raw) s.destroy();
  child.stdin.end('shutdown\n');
  const result = await Promise.race([processExit, sleep(5000).then(() => null)]);
  if (!result) {
    child.kill();
    // Confirm this exact child is gone before deleting its data directory.
    // On failure, retain the directory by throwing before the finally cleanup.
    await Promise.race([processExit, sleep(5000).then(() => { throw Error('Test authority did not exit; data retained'); })]);
  }
}
process.on('SIGINT', () => { void stop(); });
const sizes = samples(), ages = samples(), intervals = samples(), saveLatency = [], reconnectLatency = [];
let measured = false, crossRoomLeaks = 0, missingSelf = 0, receivedBytes = 0, snapshots = 0, unexpectedCloses = 0;
let startupCloses = 0;
let saveStarted = null, saveAcks = 0, saveRejected = 0, saveErrors = 0;
try {
  base = await new Promise((ok, bad) => {
    const timer = setTimeout(() => bad(Error('Server startup timeout')), 15000);
    child.on('error', bad);
    child.on('exit', code => bad(Error(`Server exited ${code}: ${stderr}`)));
    child.stdout.on('data', bytes => {
      const match = bytes.toString().match(/Chao Rust game server: (http:\/\/127\.0\.0\.1:\d+)/);
      if (match) { clearTimeout(timer); ok(match[1]); }
    });
  });
  async function openPeer(identity, rejoining = false) {
    const peer = rejoining ? identity : { ...identity, ws: null, lastSnapshot: null, expectedClose: false, latest: null };
    peer.expectedClose = false;
    const started = performance.now();
    const ws = peer.ws = new WebSocket(`${base.replace('http:', 'ws:')}/ws?token=${peer.token}`);
    await new Promise((ok, bad) => {
      const timer = setTimeout(() => bad(Error('WebSocket open/first snapshot timeout')), 15000);
      ws.addEventListener('error', () => { clearTimeout(timer); bad(Error('WebSocket error')); });
      ws.addEventListener('message', event => {
        const message = JSON.parse(event.data);
        if (message.type === 'snapshot') {
          peer.latest = message;
          if (rejoining && !peer.reconnected) { peer.reconnected = true; reconnectLatency.push(performance.now() - started); }
          const self = message.players.find(p => p.id === peer.id);
          if (measured) {
            snapshots++;
            const size = Buffer.byteLength(event.data); receivedBytes += size; sizes.add(size);
            ages.add(Math.max(0, Date.now() - message.serverTime));
            if (peer.lastSnapshot !== null) intervals.add(performance.now() - peer.lastSnapshot);
            if (!self) missingSelf++;
            if (self && message.players.some(p => p.zone !== self.zone)) crossRoomLeaks++;
          }
          peer.lastSnapshot = performance.now();
          clearTimeout(timer); ok();
        }
        if (measured && message.type === 'event' && message.kind === 'saved') {
          saveAcks++;
          if (saveStarted !== null) { saveLatency.push(performance.now() - saveStarted); saveStarted = null; }
        }
        if (measured && message.type === 'notice' && /saving again/.test(message.text ?? '')) {
          saveRejected++; saveStarted = null;
        }
        if (measured && message.type === 'notice' && /Could not save/.test(message.text ?? '')) {
          saveErrors++; saveStarted = null;
        }
      });
      ws.addEventListener('close', () => {
        if (measured && !expectedCloses.has(ws)) unexpectedCloses++;
        else if (!measured && !stopped) startupCloses++;
      });
    });
    // This is an intent, not a client-claimed teleport. The service validates gates.
    if (peer.zone !== 'hub') ws.send(JSON.stringify({ type:'action', action:peer.zone }));
    return peer;
  }
  // Keep startup concurrency bounded, outside the timed measurement.
  for (let i = 0; i < players - slowCount; i += 16) {
    peers.push(...await Promise.all(identities.slice(i, Math.min(players - slowCount, i + 16)).map(id => openPeer(id))));
  }
  for (const identity of identities.slice(players - slowCount)) {
    const u = new URL(base);
    const socket = net.createConnection({ host:'127.0.0.1', port:Number(u.port) }); raw.push(socket);
    socket.on('error', () => {});
    await new Promise((ok,bad) => {
      socket.once('connect', () => socket.write(`GET /ws?token=${identity.token} HTTP/1.1\r\nHost: ${u.host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ${randomBytes(16).toString('base64')}\r\nSec-WebSocket-Version: 13\r\n\r\n`));
      socket.once('data', chunk => { if (!chunk.toString().startsWith('HTTP/1.1 101')) bad(Error('Slow peer handshake rejected')); else { socket.pause(); ok(); } });
      socket.once('error', bad);
    });
  }
  await sleep(3000);
  // At overload, preserve the failed ramp instead of discarding all measurements.
  const setup = {
    fastOpenAtStart: peers.filter(p => p.ws.readyState === WebSocket.OPEN).length,
    roomSetupFailures: peers.filter(p => p.latest.players.find(q => q.id === p.id)?.zone !== p.zone).length,
    startupCloses,
  };
  const readMetrics = async () => { const r = await fetch(`${base}/metrics`); assert(r.ok); return r.text(); };
  async function resources() {
    if (process.platform === 'win32') {
      const { stdout } = await exec('powershell.exe', ['-NoProfile','-Command', `$p=Get-Process -Id ${child.pid}; [pscustomobject]@{cpuSeconds=$p.CPU;workingSetBytes=$p.WorkingSet64;peakWorkingSetBytes=$p.PeakWorkingSet64}|ConvertTo-Json -Compress`], { windowsHide:true });
      return JSON.parse(stdout);
    }
    if (process.platform === 'linux') {
      const status = await readFile(`/proc/${child.pid}/status`, 'utf8');
      const { stdout } = await exec('ps', ['-p',String(child.pid),'-o','time=']);
      const t = stdout.trim().split(':').map(Number);
      return { cpuSeconds: t.reduce((n,v) => n * 60 + v,0), workingSetBytes: Number(status.match(/VmRSS:\s+(\d+)/)?.[1] ?? 0)*1024, peakWorkingSetBytes: Number(status.match(/VmHWM:\s+(\d+)/)?.[1] ?? 0)*1024 };
    }
    return null;
  }
  const resourceBefore = await resources();
  const metricsWindowStart = performance.now();
  const metricsBefore = await readMetrics();
  const start = performance.now(); measured = true;
  const movement = setInterval(() => {
    for (const [i,peer] of peers.entries()) if (peer.ws.readyState === WebSocket.OPEN) peer.ws.send(JSON.stringify({type:'input',dx:Math.sin(i + (performance.now()-start)/1000),dz:0.25,jump:false}));
  },100);
  const saver = setInterval(() => {
    const peer = peers[0];
    if (peer.ws.readyState === WebSocket.OPEN && saveStarted === null) {
      peer.ws.send(JSON.stringify({type:'action',action:'rename',name:`Load ${Math.round(performance.now())}`}));
      saveStarted = performance.now(); peer.ws.send(JSON.stringify({type:'action',action:'save'}));
    }
  },1100);
  const churn = setInterval(() => {
    const index = 1 + Math.floor((performance.now()-start)/2000) % (peers.length - 1);
    const peer = peers[index]; peer.expectedClose = true; expectedCloses.add(peer.ws); peer.ws.close(); peer.reconnected = false;
    setTimeout(() => { if (measured) void openPeer(peer,true).catch(() => { unexpectedCloses++; }); },100);
  },2000);
  await sleep(seconds*1000);
  clearInterval(movement); clearInterval(saver); clearInterval(churn); measured = false;
  const elapsed = (performance.now()-start)/1000;
  const metricsAfter = await readMetrics();
  const metricsWindowSeconds = (performance.now() - metricsWindowStart) / 1000;
  const resourceAfter = await resources();
  const result = {
    schemaVersion:1, harnessRevision:5, at:new Date().toISOString(), baseRevision:'282f6ab', executable,setup,
    host:{platform:platform(),release:release(),cpu:cpus()[0]?.model,logicalCpus:cpus().length,ramBytes:totalmem(),node:process.version},
    workload:{players,fastPeers:peers.length,slowPeers:slowCount,residents,rooms:crowded?1:4,seconds,elapsedSeconds:elapsed,metricsWindowSeconds,movementHz:10,reconnectEveryMs:2000,saveEveryMs:1100,fixture:'Synthetic server-side profiles, flat fallback terrain; no retail assets; no race/combat/positive minigame workload'},
    observed:{snapshots,receivedBytes,receivedBytesPerSecond:receivedBytes/elapsed,snapshotBytes:summary(sizes.values),snapshotAgeMs:summary(ages.values),snapshotIntervalMs:summary(intervals.values),saveAckMs:summary(saveLatency),saveAcks,saveRejected,saveErrors,reconnectFirstSnapshotMs:summary(reconnectLatency),crossRoomLeaks,missingSelf,unexpectedCloses,resourceBefore,resourceAfter,serverCpuCores:resourceAfter&&resourceBefore?(resourceAfter.cpuSeconds-resourceBefore.cpuSeconds)/elapsed:null},
    metricsBefore,metricsAfter,stderr
  };
  await stop();
  try {
    const saved = JSON.parse(await readFile(resolve(directory,'profiles.json'),'utf8'));
    assert(Array.isArray(saved), 'Saved profile document must be a JSON array');
    const byId = new Map(saved.map(profile => [profile?.id, profile]));
    result.observed.shutdownProfiles = saved.length;
    result.observed.uniqueShutdownProfiles = byId.size;
    result.observed.canonicalIdsPreserved = saved.length === profiles.length && profiles.every(profile => byId.has(profile.id));
    result.observed.credentialsPreserved = profiles.every(profile => byId.get(profile.id)?.tokenHash === profile.tokenHash);
    result.observed.ownerFlagsPreserved = profiles.every(profile => byId.get(profile.id)?.admin === profile.admin);
    result.observed.residentOptInPreserved = profiles.every(profile => Boolean(byId.get(profile.id)?.resident?.enabled) === Boolean(profile.resident?.enabled));
    result.observed.persistenceReadError = null;
  } catch {
    result.observed.persistenceReadError = 'Saved profile data could not be read as a JSON profile array';
  }
  result.observed.cleanShutdown = (await processExit).code === 0;
  result.gates = evaluateCapacity(result);
  await mkdir(dirname(output), {recursive:true}); await writeFile(output,JSON.stringify(result,null,2)+'\n');
  console.log(JSON.stringify({ output, players,residents, ...result.observed, gates:result.gates, metricsBefore:undefined,metricsAfter:undefined }));
  if (!result.gates.passed) process.exitCode = 1;
} finally {
  await stop();
  await rm(directory,{recursive:true,force:true});
}
