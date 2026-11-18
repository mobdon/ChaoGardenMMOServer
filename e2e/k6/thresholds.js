// Shared k6 option/threshold builders. Values mirror e2e/thresholds.json so
// k6 exits non-zero on breach and verify-thresholds.js can re-check reports.
export const LIMITS = {
  gardenSyncRead: { p95: 150, err: 0.01, rps: 50 },
  careCommandBurst: { p95: 120, err: 0.02, db: 20, dup: 0 },
  websocketConnect: { ok: 0.995, p95: 400, drop: 0.001 },
  roomFanout: { p95: 250, drop: 0.001, rateLimit: 0.0 },
  reconnectStorm: { resume: 0.99, p95: 400 },
};

export function httpThresholds(limits, extra = {}) {
  return {
    http_req_duration: [`p(95)<${limits.p95}`],
    http_req_failed: [`rate<${limits.err}`],
    ...extra,
  };
}

export function wsThresholds(limits, extra = {}) {
  return {
    ws_connect_success: [`rate>${limits.ok}`],
    ws_session_duration: [`p(95)<60000`],
    ...extra,
  };
}

// Standard scenario shapes so every file documents its own load profile.
export const stages = {
  ramp: (peak, seconds) => [
    { duration: '30s', target: Math.ceil(peak / 3) },
    { duration: `${seconds}s`, target: peak },
    { duration: '30s', target: 0 },
  ],
};

export function tag(name) {
  return { test_run: name };
}

export function handleSummary(data) {
  // Always emit machine-readable JSON for verify-thresholds.js + CI artifacts.
  const name = (data.options && data.options.summaryTrendStats) ? 'k6' : 'k6';
  const summary = {
    scenario: typeof SCENARIO_NAME !== 'undefined' ? SCENARIO_NAME : name,
    metrics: {},
  };
  for (const [key, value] of Object.entries(data.metrics)) {
    summary.metrics[key] = {
      // Keep only serializable primitives; k6 objects are plain here.
      value: value.value !== undefined ? value.value : undefined,
      rate: value.rate,
      p95: value.thresholds ? undefined : undefined,
    };
  }
  return {
    stdout: textSummary(data),
    'e2e/reports/k6/summary.json': JSON.stringify(summary, null, 2),
  };
}

function textSummary(data) {
  const lines = ['k6 summary'];
  for (const [key, m] of Object.entries(data.metrics)) {
    if (m.values && m.values['p(95)'] !== undefined) {
      lines.push(`${key}: p95=${m.values['p(95)'].toFixed(1)}`);
    } else if (m.rate !== undefined) {
      lines.push(`${key}: rate=${m.rate}`);
    } else if (m.value !== undefined) {
      lines.push(`${key}: ${m.value}`);
    }
  }
  return lines.join('\n') + '\n';
}
