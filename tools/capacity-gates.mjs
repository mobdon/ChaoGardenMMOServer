// Pure capacity acceptance policy plus a read-only evaluator for saved measurements.
// node tools/capacity-gates.mjs measurements/2026-11-18
import { readFile, readdir, stat } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const CAPACITY_THRESHOLDS = Object.freeze({
  minimumTickHz: 19,
  maximumTickP95UpperMs: 50,
  maximumSnapshotAgeP95Ms: 250,
});

function parseMetrics(text) {
  const metrics = new Map();
  if (typeof text !== 'string') return metrics;
  for (const line of text.split('\n')) {
    const match = line.match(/^([a-zA-Z_:][a-zA-Z0-9_:]*(?:\{[^}]*\})?)\s+([-+\deE.]+)(?:\s|$)/);
    if (match) metrics.set(match[1], Number(match[2]));
  }
  return metrics;
}
const nonnegativeInteger = value => Number.isSafeInteger(value) && value >= 0;
const positiveFinite = value => Number.isFinite(value) && value > 0;

/** Evaluate recorded evidence without starting a service or changing the input. */
export function evaluateCapacity(document) {
  const workload = document?.workload ?? {};
  const setup = document?.setup ?? {};
  const observed = document?.observed ?? {};
  const before = parseMetrics(document?.metricsBefore);
  const after = parseMetrics(document?.metricsAfter);
  const checks = [];
  const check = (id, pass, actual, expected) => checks.push({ id, pass: Boolean(pass), actual: actual === undefined ? '<missing>' : actual, expected });
  const delta = key => {
    const first = before.get(key), last = after.get(key);
    return nonnegativeInteger(first) && nonnegativeInteger(last) && last >= first ? last - first : null;
  };
  const tickCount = delta('chao_tick_duration_seconds_count');
  const metricWindow = workload.metricsWindowSeconds;
  const tickHz = tickCount != null && positiveFinite(metricWindow) ? tickCount / metricWindow : null;
  check('explicit-metric-window', positiveFinite(metricWindow), metricWindow, 'finite seconds > 0; elapsedSeconds is not an interchangeable scrape window');
  check('tick-count', tickCount != null && tickCount > 0, tickCount, 'positive monotonic histogram sample delta');
  check('tick-frequency', tickHz != null && tickHz >= CAPACITY_THRESHOLDS.minimumTickHz, tickHz, `>= ${CAPACITY_THRESHOLDS.minimumTickHz} Hz`);

  const prefix = 'chao_tick_duration_seconds_bucket';
  const buckets = [...after.keys()].filter(key => key.startsWith(`${prefix}{le="`))
    .map(key => ({ key, bound: Number(key.match(/le="([^"]+)"/)[1].replace('+Inf', 'Infinity')), count: delta(key) }))
    .sort((a, b) => a.bound - b.bound);
  let previous = 0;
  const validBuckets = buckets.length > 0 && buckets.every(bucket => {
    const valid = bucket.count != null && bucket.count >= previous && bucket.count <= tickCount && bucket.bound > 0;
    previous = bucket.count;
    return valid;
  }) && buckets.at(-1).bound === Infinity && buckets.at(-1).count === tickCount;
  const percentileBucket = validBuckets && tickCount > 0
    ? buckets.find(bucket => bucket.count >= Math.ceil(tickCount * 0.95)) : null;
  const p95UpperMs = percentileBucket ? percentileBucket.bound * 1000 : null;
  check('tick-histogram-valid', validBuckets, validBuckets, 'monotonic cumulative delta buckets ending at +Inf and sample count');
  check('tick-p95-upper-bound', Number.isFinite(p95UpperMs) && p95UpperMs <= CAPACITY_THRESHOLDS.maximumTickP95UpperMs,
    p95UpperMs === Infinity ? '+Inf' : p95UpperMs, `<= ${CAPACITY_THRESHOLDS.maximumTickP95UpperMs} ms (bucket upper bound; no interpolation)`);
  for (const [id, metric] of [
    ['persistence-failures', 'chao_persist_failures_total'],
    ['reliable-event-lag', 'chao_event_lag_total'],
    ['oversized-snapshots', 'chao_oversized_snapshots_total'],
    ['input-rate-disconnects', 'chao_input_rate_disconnects_total'],
    ['admission-rejections', 'chao_admission_rejected_total'],
  ]) {
    const count = delta(metric);
    check(id, count === 0, count, 'zero monotonic counter delta');
  }

  const age = observed.snapshotAgeMs ?? {};
  check('snapshot-age-samples', nonnegativeInteger(age.count) && age.count > 0, age.count, 'at least one observed sample');
  check('snapshot-age-p95', Number.isFinite(age.p95) && age.p95 >= 0 && age.p95 <= CAPACITY_THRESHOLDS.maximumSnapshotAgeP95Ms,
    age.p95, `<= ${CAPACITY_THRESHOLDS.maximumSnapshotAgeP95Ms} ms, observed wall time minus authoritative simulation time`);
  check('snapshots-observed', nonnegativeInteger(observed.snapshots) && observed.snapshots > 0, observed.snapshots, 'positive snapshot count');

  const expectedProfiles = nonnegativeInteger(workload.players) && workload.players > 0 && nonnegativeInteger(workload.residents)
    ? workload.players + workload.residents : null;
  const expectedFast = nonnegativeInteger(workload.players) && nonnegativeInteger(workload.slowPeers)
    ? workload.players - workload.slowPeers : null;
  check('workload-profile-counts', expectedProfiles != null && expectedFast > 0 && workload.fastPeers === expectedFast,
    { players: workload.players, residents: workload.residents, fastPeers: workload.fastPeers, slowPeers: workload.slowPeers }, 'valid counts; fastPeers = players - slowPeers > 0');
  check('room-setup', setup.roomSetupFailures === 0, setup.roomSetupFailures, 0);
  check('fast-peers-open', expectedFast > 0 && setup.fastOpenAtStart === expectedFast, setup.fastOpenAtStart, expectedFast);
  check('startup-disconnects', setup.startupCloses === 0, setup.startupCloses, 0);
  for (const [id, key] of [['unexpected-fast-disconnects', 'unexpectedCloses'], ['cross-room-leaks', 'crossRoomLeaks'], ['missing-self', 'missingSelf'], ['save-errors', 'saveErrors']]) {
    check(id, observed[key] === 0, observed[key], 0);
  }
  check('save-acknowledgments', nonnegativeInteger(observed.saveAcks) && observed.saveAcks > 0, observed.saveAcks, '> 0');
  check('clean-shutdown', observed.cleanShutdown === true, observed.cleanShutdown, true);
  check('persisted-profile-count', expectedProfiles != null && observed.shutdownProfiles === expectedProfiles, observed.shutdownProfiles, expectedProfiles);
  check('unique-persisted-profile-count', expectedProfiles != null && observed.uniqueShutdownProfiles === expectedProfiles, observed.uniqueShutdownProfiles, expectedProfiles);
  for (const key of ['canonicalIdsPreserved', 'credentialsPreserved', 'ownerFlagsPreserved', 'residentOptInPreserved']) {
    check(`integrity-${key}`, observed[key] === true, observed[key], true);
  }
  check('persistence-readable', observed.persistenceReadError === null, observed.persistenceReadError, null);
  return {
    policyVersion: 1,
    passed: checks.every(value => value.pass),
    thresholds: CAPACITY_THRESHOLDS,
    measured: { tickHz, tickP95UpperMs: p95UpperMs === Infinity ? '+Inf' : p95UpperMs, snapshotAgeP95Ms: age.p95 ?? null },
    checks,
    failures: checks.filter(value => !value.pass).map(value => value.id),
    scope: 'Synthetic movement/save/reconnect transport gate only; passing is not a representative gameplay or public-deployment capacity claim. Missing historical evidence fails closed; raw measurements are not modified.',
  };
}

async function evaluateFiles(arguments_) {
  const inputs = arguments_.length ? arguments_ : ['measurements/2026-11-18'];
  const paths = [];
  for (const input of inputs) {
    const path = resolve(input);
    if ((await stat(path)).isDirectory()) {
      for (const file of (await readdir(path)).filter(name => /^network.*\.json$/.test(name)).sort()) paths.push(join(path, file));
    } else paths.push(path);
  }
  if (!paths.length) throw new Error('No network*.json measurements found.');
  let passed = true;
  for (const path of paths) {
    try {
      const result = evaluateCapacity(JSON.parse(await readFile(path, 'utf8')));
      console.log(JSON.stringify({ file: basename(path), ...result }));
      passed &&= result.passed;
    } catch (error) {
      console.log(JSON.stringify({ file: basename(path), passed: false, error: error.message }));
      passed = false;
    }
  }
  return passed;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { if (!await evaluateFiles(process.argv.slice(2))) process.exitCode = 1; }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
