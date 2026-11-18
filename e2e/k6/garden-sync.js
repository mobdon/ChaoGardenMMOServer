// Read-heavy garden snapshot sync. Many virtual guests poll /api/status and
// garden state like the mobile client does at rest.
// Run: k6 run e2e/k6/garden-sync.js  (TEST_BASE_URL required)
import http from 'k6/http';
import { check, sleep } from 'k6';
import { LIMITS, httpThresholds, stages, tag } from './thresholds.js';

export const SCENARIO_NAME = 'garden_sync_read';

const BASE = __ENV.TEST_BASE_URL || 'http://127.0.0.1:3001';
const TOKEN_FILE = __ENV.TEST_TOKEN_FILE; // path containing a bearer token

export const options = {
  scenarios: {
    garden_sync_read: {
      executor: 'ramping-vus',
      stages: stages.ramp(60, 120),
      gracefulStop: '10s',
    },
  },
  thresholds: httpThresholds(LIMITS.gardenSyncRead, {
    http_reqs: [`rate>${LIMITS.gardenSyncRead.rps}`],
  }),
  tags: tag(SCENARIO_NAME),
};

function token() {
  if (!TOKEN_FILE) return '';
  return open(TOKEN_FILE).trim();
}

export default function () {
  const params = {
    headers: {
      Accept: 'application/json',
      Authorization: token() ? `Bearer ${token()}` : undefined,
    },
  };
  const res = http.get(`${BASE}/api/status`, params);
  check(res, { 'status 200': (r) => r.status === 200 });
  const garden = http.get(`${BASE}/api/activities`, params);
  check(garden, { 'activities 200': (r) => r.status === 200 });
  // 10 Hz snapshot equivalent is too hot for HTTP; guests poll ~1/s at rest.
  sleep(1);
}
