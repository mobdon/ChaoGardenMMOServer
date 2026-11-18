// Room fan-out: several peers join one garden room; one peer acts and every
// other peer must receive the event within threshold with no drops.
// Run: k6 run e2e/k6/room-fanout.js  (TEST_BASE_URL + TEST_TOKEN_FILE)
import ws from 'k6/ws';
import http from 'k6/http';
import { check, sleep, Counter, Trend } from 'k6';
import { LIMITS, tag } from './thresholds.js';

export const SCENARIO_NAME = 'room_fanout';
const BASE = __ENV.TEST_BASE_URL || 'http://127.0.0.1:3001';
const TOKEN_FILE = __ENV.TEST_TOKEN_FILE;
const Delivery = new Trend('event_delivery_ms', true);
const Dropped = new Counter('dropped_events');
const RateLimited = new Counter('per_peer_rate_limit_429');

export const options = {
  scenarios: {
    room_fanout: {
      executor: 'per-vu-iterations',
      vus: 12,
      iterations: 20,
      maxDuration: '3m',
    },
  },
  thresholds: {
    event_delivery_ms: [`p(95)<${LIMITS.roomFanout.p95}`],
    dropped_events: [`rate<${LIMITS.roomFanout.drop}`],
    per_peer_rate_limit_429: [`rate<=${LIMITS.roomFanout.rateLimit}`],
  },
  tags: tag(SCENARIO_NAME),
};

export default function () {
  const token = TOKEN_FILE ? open(TOKEN_FILE).trim() : '';
  const url = `${BASE.replace(/^http/, 'ws')}/ws?token=${encodeURIComponent(token)}`;
  // Peers alternate roles by VU parity: even VUs act, odd VUs listen.
  const isActor = __VU % 2 === 0;
  ws.connect(url, {}, (socket) => {
    socket.on('message', (data) => {
      let msg;
      try { msg = JSON.parse(data); } catch { return; }
      if (msg && msg.kind === 'fanout') {
        const sentAt = msg.sentAt;
        if (sentAt) Delivery.add(Date.now() - sentAt);
        else Dropped.add(1);
      }
    });
    if (isActor) {
      // Trigger a broadcast action; payload stays well under the 2 KiB limit.
      http.post(
        `${BASE}/api/session`,
        JSON.stringify({ resumeOnly: true, broadcastProbe: true }),
        { headers: { 'Content-Type': 'application/json' } },
      );
      // The 80 msg/s per-connection cap means a legit burst may see 429 only
      // if the server misclassifies; count it, it must stay at zero.
      if (http.lastRequest && http.lastRequest.status === 429) RateLimited.add(1);
    }
    socket.setTimeout(() => socket.close(), 15000);
  });
  sleep(0.5);
}
