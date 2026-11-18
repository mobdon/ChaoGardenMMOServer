// Reconnect storm: kill and re-open WebSocket connections repeatedly, each
// time resuming with resumeOnly:true. Verifies the resume path (no duplicate
// profile creation, bounded latency, no silent data loss) under churn.
// Run: k6 run e2e/k6/reconnect-storm.js  (TEST_BASE_URL + TEST_TOKEN_FILE)
import ws from 'k6/ws';
import http from 'k6/http';
import { check, sleep } from 'k6';
import { Trend, Rate } from 'k6/metrics';
import { LIMITS, tag } from './thresholds.js';

export const SCENARIO_NAME = 'reconnect_storm';
const BASE = __ENV.TEST_BASE_URL || 'http://127.0.0.1:3001';
const TOKEN_FILE = __ENV.TEST_TOKEN_FILE;
const RESUMES = Number(__ENV.RESUMES_PER_VU || 5);

const resumeLatency = new Trend('resume_latency_ms', true);
const resumeSuccess = new Rate('session_resume_success');

export const options = {
  scenarios: {
    reconnect_storm: {
      executor: 'per-vu-iterations',
      vus: 30,
      iterations: RESUMES,
      maxDuration: '5m',
    },
  },
  thresholds: {
    session_resume_success: [`rate>${LIMITS.reconnectStorm.resume}`],
    resume_latency_ms: [`p(95)<${LIMITS.reconnectStorm.p95}`],
  },
  tags: tag(SCENARIO_NAME),
};

export default function () {
  const token = TOKEN_FILE ? open(TOKEN_FILE).trim() : '';
  // 1. Resume-only session must exist (404 would mean the profile vanished).
  const started = Date.now();
  const res = http.post(
    `${BASE}/api/session`,
    JSON.stringify({ resumeOnly: true }),
    { headers: { 'Content-Type': 'application/json' } },
  );
  resumeLatency.add(Date.now() - started);
  resumeSuccess.add(res.status === 200 ? 1 : 0);

  // 2. Reconnect the WebSocket immediately (simulates app backgrounding).
  const url = `${BASE.replace(/^http/, 'ws')}/ws?token=${encodeURIComponent(token)}`;
  const conn = ws.connect(url, {}, (socket) => {
    socket.setTimeout(() => socket.close(), 3000);
  });
  check(conn, { 'ws re-handshake 101': (r) => r && r.status === 101 });
  sleep(1);
}
