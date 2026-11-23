// Read-only capacity report helper. Histogram quantiles are bucket upper bounds.
// node tools/summarize-capacity.mjs [--json] [measurement.json | directory ...]
import { readFile, readdir, stat } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';

const arguments_ = process.argv.slice(2);
const json = arguments_.includes('--json');
if (arguments_.some(value => value.startsWith('--') && value !== '--json')) {
  throw new Error('Usage: node tools/summarize-capacity.mjs [--json] [measurement.json | directory ...]');
}
const inputs = arguments_.filter(value => value !== '--json');
if (!inputs.length) inputs.push('measurements/2026-11-18');
const paths = [];
for (const input of inputs) {
  const path = resolve(input);
  if ((await stat(path)).isDirectory()) {
    for (const file of (await readdir(path)).filter(name => /^network.*\.json$/.test(name)).sort()) {
      paths.push(join(path, file));
    }
  } else paths.push(path);
}
if (!paths.length) throw new Error('No network*.json measurements found.');

function metrics(text) {
  const result = new Map();
  for (const line of text.split('\n')) {
    const match = line.match(/^([a-zA-Z_:][a-zA-Z0-9_:]*(?:\{[^}]*\})?)\s+([-+\deE.]+)(?:\s|$)/);
    if (match) result.set(match[1], Number(match[2]));
  }
  return result;
}
function required(values, key) {
  const value = values.get(key);
  if (!Number.isFinite(value)) throw new Error(`Missing or non-finite metric: ${key}`);
  return value;
}
function summarize(document, file) {
  const before = metrics(document.metricsBefore);
  const after = metrics(document.metricsAfter);
  const delta = key => {
    const result = required(after, key) - required(before, key);
    if (result < 0) throw new Error(`${file}: metric reset during measurement: ${key}`);
    return result;
  };
  const elapsed = document.workload.metricsWindowSeconds ?? document.workload.elapsedSeconds;
  if (!(elapsed > 0)) throw new Error(`${file}: invalid elapsedSeconds`);
  const histogram = name => {
    const count = delta(`${name}_count`);
    const sumSeconds = delta(`${name}_sum`);
    const buckets = [...after.keys()].filter(key => key.startsWith(`${name}_bucket{le="`))
      .map(key => ({ key, bound: Number(key.match(/le="([^"]+)"/)[1].replace('+Inf', 'Infinity')) }))
      .sort((a, b) => a.bound - b.bound);
    const bucket = count ? buckets.find(({ key }) => delta(key) >= Math.ceil(count * 0.95)) : null;
    if (count && !bucket) throw new Error(`${file}: histogram count exceeds buckets: ${name}`);
    return {
      count,
      meanMs: count ? sumSeconds * 1000 / count : null,
      p95UpperMs: !bucket ? null : Number.isFinite(bucket.bound) ? bucket.bound * 1000 : '+Inf',
    };
  };
  const histograms = Object.fromEntries([
    ['tick', 'chao_tick_duration_seconds'],
    ['tickLockWait', 'chao_tick_lock_wait_seconds'],
    ['residentAi', 'chao_resident_tick_duration_seconds'],
    ['snapshotBuild', 'chao_snapshot_duration_seconds'],
    ['save', 'chao_save_duration_seconds'],
    ['send', 'chao_send_duration_seconds'],
  ].map(([label, name]) => [label, histogram(name)]));
  const snapshots = delta('chao_snapshots_total');
  const bytes = delta('chao_snapshot_bytes_total');
  return {
    file: basename(file),
    players: document.workload.players,
    residents: document.workload.residents,
    rooms: document.workload.rooms,
    elapsedSeconds: elapsed,
    rateWindow: document.workload.metricsWindowSeconds == null
      ? 'Approximate: metric scrapes bracket additional resource-sampling overhead outside elapsedSeconds'
      : 'Explicit metric scrape window',
    histograms,
    tickHz: histograms.tick.count / elapsed,
    serverSnapshotTraffic: {
      snapshots,
      snapshotsPerSecond: snapshots / elapsed,
      bytes,
      bytesPerSecond: bytes / elapsed,
      meanBytes: snapshots ? bytes / snapshots : null,
      excludes: 'WebSocket/TLS overhead and non-snapshot frames',
    },
    counters: Object.fromEntries([
      ['slowClients', 'chao_slow_clients_total'],
      ['eventLagEntries', 'chao_event_lag_total'],
      ['oversizedSnapshots', 'chao_oversized_snapshots_total'],
      ['reconnects', 'chao_reconnects_total'],
      ['saveFailures', 'chao_persist_failures_total'],
      ['admissionRejected', 'chao_admission_rejected_total'],
      ['rateDisconnected', 'chao_input_rate_disconnects_total'],
    ].map(([label, name]) => [label, delta(name)])),
    queue: {
      depthBefore: required(before, 'chao_event_queue_depth'),
      depthAfter: required(after, 'chao_event_queue_depth'),
      highWaterBefore: required(before, 'chao_event_queue_high_water'),
      highWaterAfter: required(after, 'chao_event_queue_high_water'),
      highWaterScope: 'Process lifetime gauge; do not subtract to infer the window maximum',
    },
    client: {
      snapshotAgeP95Ms: document.observed.snapshotAgeMs.p95,
      snapshotIntervalP95Ms: document.observed.snapshotIntervalMs.p95,
      saveAckP95Ms: document.observed.saveAckMs.p95,
      reconnectFirstSnapshotP95Ms: document.observed.reconnectFirstSnapshotMs.p95,
      crossRoomLeaks: document.observed.crossRoomLeaks,
      missingSelf: document.observed.missingSelf,
      unexpectedCloses: document.observed.unexpectedCloses,
      quantileScope: 'Client sampled observed quantiles, not server histogram bucket bounds',
    },
    serverCpuCores: document.observed.serverCpuCores,
    workingSetBytes: document.observed.resourceAfter.workingSetBytes,
  };
}
const reports = [];
for (const file of paths) reports.push(summarize(JSON.parse(await readFile(file, 'utf8')), file));
if (json) console.log(JSON.stringify(reports, null, 2));
else {
  const n = value => value == null ? 'n/a' : typeof value === 'number' ? Number(value.toFixed(3)).toString() : value;
  console.log('Server p95 values below are histogram bucket upper bounds. Client p95 values are sampled observed quantiles. Rates are approximate when the source omits metricsWindowSeconds.');
  console.log('\n| Scenario players/residents | Tick Hz | Tick p95 ≤ ms | Lock p95 ≤ ms | Snapshot p95 ≤ ms | Save p95 ≤ ms | Client age p95 ms | Snapshot MiB/s | Slow / lag / reconnect | Queue HWM |');
  console.log('| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: |');
  for (const report of reports) {
    const h = report.histograms, c = report.counters;
    console.log(`| ${report.players}/${report.residents} (${report.rooms} rooms) | ${n(report.tickHz)} | ${n(h.tick.p95UpperMs)} | ${n(h.tickLockWait.p95UpperMs)} | ${n(h.snapshotBuild.p95UpperMs)} | ${n(h.save.p95UpperMs)} | ${n(report.client.snapshotAgeP95Ms)} | ${n(report.serverSnapshotTraffic.bytesPerSecond / 1048576)} | ${c.slowClients} / ${c.eventLagEntries} / ${c.reconnects} | ${report.queue.highWaterAfter} |`);
  }
  console.log('\n| Scenario | Mean tick / resident AI / snapshot / save / send / lock wait (ms) | Snapshot frames/s | Oversize / save failures | Client save / reconnect p95 (ms) |');
  console.log('| --- | --- | ---: | --- | --- |');
  for (const report of reports) {
    console.log(`| ${report.players}/${report.residents} | ${['tick','residentAi','snapshotBuild','save','send','tickLockWait'].map(key => n(report.histograms[key].meanMs)).join(' / ')} | ${n(report.serverSnapshotTraffic.snapshotsPerSecond)} | ${report.counters.oversizedSnapshots} / ${report.counters.saveFailures} | ${n(report.client.saveAckP95Ms)} / ${n(report.client.reconnectFirstSnapshotP95Ms)} |`);
  }
  console.log('\nQueue HWM is the final process-lifetime high-water gauge. No histogram interpolation is used. Use --json for all counters and histogram sample counts.');
}
