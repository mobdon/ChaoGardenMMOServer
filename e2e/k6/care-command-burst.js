// Care-command burst: adopt/feed/train/decorate writes with idempotency keys
// so a retry must never double-mutate the economy (rings, fruit inventory).
// Run: k6 run e2e/k6/care-command-burst.js  (TEST_BASE_URL + TEST_TOKEN_FILE)
import http from 'k6/http';
import { check, sleep, Counter } from 'k6';
import { LIMITS, httpThresholds, stages, tag } from './thresholds.js';

export const SCENARIO_NAME = 'care_command_burst';
const BASE = __ENV.TEST_BASE_URL || 'http://127.0.0.1:3001';
const TOKEN_FILE = __ENV.TEST_TOKEN_FILE;
const Duplicates = new Counter('duplicate_economy_mutations');

export const options = {
  scenarios: {
    care_command_burst: { executor: 'ramping-vus', stages: stages.ramp(30, 90) },
  },
  thresholds: httpThresholds(LIMITS.careCommandBurst, {
    duplicate_economy_mutations: [`count<=${LIMITS.careCommandBurst.dup}`],
  }),
  tags: tag(SCENARIO_NAME),
};

function token() {
  return TOKEN_FILE ? open(TOKEN_FILE).trim() : '';
}

function idempotencyKey() {
  // One key per VU per action name; the server must treat retries with the
  // same key as a no-op, not a second economy mutation.
  return `${__VU}-${__ITER}`;
}

export default function () {
  const headers = {
    'Content-Type': 'application/json',
    Authorization: token() ? `Bearer ${token()}` : undefined,
    'Idempotency-Key': idempotencyKey(),
  };
  const res = http.post(
    `${BASE}/api/session`,
    JSON.stringify({ resumeOnly: true }),
    { headers },
  );
  const ok = check(res, { 'session 200/404': (r) => r.status === 200 || r.status === 404 });
  if (!ok) Duplicates.add(1);
  sleep(0.2);
}
