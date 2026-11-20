import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { evaluateCapacity } from './capacity-gates.mjs';

const histogram = (count, slow = false) => [
  ...['persist_failures','event_lag','oversized_snapshots','input_rate_disconnects','admission_rejected']
    .map(name => `chao_${name}_total 0`),
  `chao_tick_duration_seconds_count ${count}`,
  ...[[0.0001,0],[0.001,count / 2],[0.05,slow ? count / 2 : count],[0.1,count],['+Inf',count]]
    .map(([bound, value]) => `chao_tick_duration_seconds_bucket{le="${bound}"} ${value}`),
].join('\n');
function goodMeasurement() {
  return {
    workload: { players:100, residents:100, fastPeers:98, slowPeers:2, metricsWindowSeconds:30, elapsedSeconds:30 },
    setup: { fastOpenAtStart:98, roomSetupFailures:0, startupCloses:0 },
    observed: {
      snapshots:29000, snapshotAgeMs:{count:20000,p95:50}, saveAcks:27, saveErrors:0,
      unexpectedCloses:0, crossRoomLeaks:0, missingSelf:0, cleanShutdown:true,
      shutdownProfiles:200, uniqueShutdownProfiles:200, canonicalIdsPreserved:true,
      credentialsPreserved:true, ownerFlagsPreserved:true, residentOptInPreserved:true,
      persistenceReadError:null,
    },
    metricsBefore:histogram(0), metricsAfter:histogram(600),
  };
}
function fails(document, id) {
  const result = evaluateCapacity(document);
  assert.equal(result.passed, false);
  assert(result.failures.includes(id), `Expected ${id}, got ${result.failures}`);
  return result;
}

test('complete evidence at budget boundaries passes without mutating input', () => {
  const measurement = goodMeasurement();
  measurement.metricsAfter = histogram(570);
  measurement.observed.snapshotAgeMs.p95 = 250;
  const original = JSON.stringify(measurement);
  const result = evaluateCapacity(measurement);
  assert.equal(result.passed, true);
  assert.equal(result.measured.tickHz, 19);
  assert.equal(result.measured.tickP95UpperMs, 50);
  assert.equal(JSON.stringify(measurement), original);
});

test('a mean tick frequency just below 19 Hz fails despite cheap tick work', () => {
  const measurement = goodMeasurement();
  measurement.metricsAfter = histogram(568);
  fails(measurement, 'tick-frequency');
});

test('a fast mean tick rate cannot conceal a slow histogram p95', () => {
  const measurement = goodMeasurement();
  measurement.metricsAfter = histogram(600, true);
  const result = fails(measurement, 'tick-p95-upper-bound');
  assert.equal(result.measured.tickHz, 20);
  assert.equal(result.measured.tickP95UpperMs, 100);
});

test('soak regression: 19.899 Hz still fails when wall-minus-sim age p95 is 603 ms', () => {
  const measurement = goodMeasurement();
  measurement.workload.metricsWindowSeconds = 120.0059272;
  measurement.metricsAfter = histogram(2388);
  measurement.observed.snapshotAgeMs.p95 = 603.03173828125;
  const result = fails(measurement, 'snapshot-age-p95');
  assert(result.measured.tickHz > 19);
  assert(!result.failures.includes('tick-frequency'));
});

test('missing or reset metric evidence and invalid cumulative buckets fail closed', () => {
  for (const mutate of [
    d => { delete d.workload.metricsWindowSeconds; },
    d => { d.metricsAfter = ''; },
    d => { d.metricsBefore = histogram(602); },
    d => { d.metricsAfter = d.metricsAfter.replace(/.*le="\+Inf".*\n?/, ''); },
    d => { d.metricsAfter = d.metricsAfter.replace('le="0.001"} 300', 'le="0.001"} 700'); },
  ]) {
    const measurement = goodMeasurement(); mutate(measurement);
    assert.equal(evaluateCapacity(measurement).passed, false);
  }
  assert.equal(evaluateCapacity(null).passed, false);
});

test('missing or excessive observed age cannot silently pass', () => {
  for (const age of [{}, {count:0,p95:0}, {count:10,p95:251}, {count:10,p95:NaN}]) {
    const measurement = goodMeasurement(); measurement.observed.snapshotAgeMs = age;
    assert.equal(evaluateCapacity(measurement).passed, false);
  }
});

test('room ramp failures and fast-client disconnects fail even when timing passes', () => {
  for (const [section, key, value, id] of [
    ['setup','roomSetupFailures',1,'room-setup'],
    ['setup','fastOpenAtStart',97,'fast-peers-open'],
    ['setup','startupCloses',1,'startup-disconnects'],
    ['observed','unexpectedCloses',1,'unexpected-fast-disconnects'],
    ['observed','crossRoomLeaks',1,'cross-room-leaks'],
    ['observed','missingSelf',1,'missing-self'],
  ]) {
    const measurement = goodMeasurement(); measurement[section][key] = value;
    fails(measurement, id);
  }
});

test('missing save acknowledgments, save errors and unclean shutdown cannot pass', () => {
  for (const [key, value, id] of [
    ['saveAcks',0,'save-acknowledgments'], ['saveErrors',1,'save-errors'],
    ['cleanShutdown',false,'clean-shutdown'],
  ]) {
    const measurement = goodMeasurement(); measurement.observed[key] = value;
    fails(measurement, id);
  }
  const missing = goodMeasurement(); delete missing.observed.saveErrors;
  const result = fails(missing, 'save-errors');
  assert.equal(result.checks.find(check => check.id === 'save-errors').actual, '<missing>');
});

test('server save failures, reliable event loss and rejected capacity fail despite good client samples', () => {
  for (const [metric, id] of [
    ['persist_failures', 'persistence-failures'],
    ['event_lag', 'reliable-event-lag'],
    ['oversized_snapshots', 'oversized-snapshots'],
    ['input_rate_disconnects', 'input-rate-disconnects'],
    ['admission_rejected', 'admission-rejections'],
  ]) {
    const measurement = goodMeasurement();
    measurement.metricsAfter = measurement.metricsAfter.replace(`chao_${metric}_total 0`, `chao_${metric}_total 1`);
    fails(measurement, id);
  }
});

test('lost/duplicate identity, credentials, ownership, residents and unreadable save fail', () => {
  for (const [key, value, id] of [
    ['shutdownProfiles',199,'persisted-profile-count'],
    ['uniqueShutdownProfiles',199,'unique-persisted-profile-count'],
    ...['canonicalIdsPreserved','credentialsPreserved','ownerFlagsPreserved','residentOptInPreserved']
      .map(key => [key,false,`integrity-${key}`]),
    ['persistenceReadError','unreadable','persistence-readable'],
  ]) {
    const measurement = goodMeasurement(); measurement.observed[key] = value;
    fails(measurement, id);
  }
  const missing = goodMeasurement(); delete missing.observed.uniqueShutdownProfiles;
  fails(missing, 'unique-persisted-profile-count');
});

test('read-only CLI returns nonzero for failed saved evidence and preserves raw bytes', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'chao-gate-negative-'));
  try {
    const path = join(directory, 'network-failing.json');
    const measurement = goodMeasurement(); measurement.observed.snapshotAgeMs.p95 = 603;
    const bytes = JSON.stringify(measurement) + '\n';
    await writeFile(path, bytes);
    const result = spawnSync(process.execPath, [resolve('tools/capacity-gates.mjs'), path], {encoding:'utf8', windowsHide:true});
    assert.equal(result.status, 1, result.stderr);
    assert.equal(JSON.parse(result.stdout).passed, false);
    assert(JSON.parse(result.stdout).failures.includes('snapshot-age-p95'));
    assert.equal(await readFile(path,'utf8'), bytes);
    measurement.observed.snapshotAgeMs.p95 = 50;
    await writeFile(path, JSON.stringify(measurement));
    const passed = spawnSync(process.execPath, [resolve('tools/capacity-gates.mjs'), path], {encoding:'utf8', windowsHide:true});
    assert.equal(passed.status, 0, passed.stderr);
    assert.equal(JSON.parse(passed.stdout).passed, true);
  } finally {
    assert.equal(dirname(resolve(directory)), resolve(tmpdir()));
    assert(basename(directory).startsWith('chao-gate-negative-'));
    await rm(directory, {recursive:true, force:true});
  }
});
