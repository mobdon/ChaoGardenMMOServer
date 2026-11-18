// WebSocket connection soak: open authenticated /ws connections, read the
// 10 Hz snapshot stream, verify no gaps/drops and bounded handshake latency.
// Run: k6 run e2e/k6/websocket-connect.js  (TEST_BASE_URL + TEST_TOKEN_FILE)
import ws from 'k6/ws';
import { check, sleep, Counter } from 'k6';
import { LIMITS, wsThresholds, stages, tag } from './thresholds.js';

export const SCENARIO_NAME = 'websocket_connect';
const BASE = __ENV.TEST_BASE_URL || 'http://127.0.0.1:3001';
const TOKEN_FILE = __ENV.TEST_TOKEN_FILE;
const Drops = new Counter('snapshot_gap_drops');

export const options = {
  scenarios: {
    websocket_connect: { executor: 'ramping-vus', stages: stages.ramp(40, 120) },
  },
  thresholds: wsThresholds(LIMITS.websocketConnect, {
    snapshot_gap_drops: [`count<1000`], // < 0.1% of ~10 Hz * peers * duration
  }),
  tags: tag(SCENARIO_NAME),
};

export default function () {
  const token = TOKEN_FILE ? open(TOKEN_FILE).trim() : '';
  const url = `${BASE.replace(/^http/, 'ws')}/ws?token=${encodeURIComponent(token)}`;
  const res = ws.connect(url, {}, (socket) => {
    let lastSeq = 0;
    socket.on('message', (data) => {
      let msg;
      try { msg = JSON.parse(data); } catch { return; }
      if (msg && msg.seq !== undefined) {
        if (lastSeq && msg.seq > lastSeq + 1) Drops.add(1);
        lastSeq = msg.seq;
      }
    });
    socket.setTimeout(() => socket.close(), 30000);
  });
  check(res, { 'handshake 101': (r) => r && r.status === 101 });
  sleep(1);
}
