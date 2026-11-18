/**
 * Verifies that the Redis-backed fan-out hook is wired: with a Redis client
 * attached, publish calls `redis.publish` per event; failures degrade
 * gracefully (local delivery still succeeds). Uses a stub client — no live
 * Redis required.
 */

import { PresenceService } from '../src/ws/presence.service';
import { RealtimeEventService } from '../src/ws/realtime-event.service';
import { RealtimeGatewayConfig } from '../src/ws/gateway.config';

function build() {
  const presence = new PresenceService(new RealtimeGatewayConfig());
  const events = new RealtimeEventService(presence, new RealtimeGatewayConfig());
  return { presence, events };
}

describe('redis fan-out', () => {
  it('publishes events to the room channel when redis is attached', async () => {
    const { events } = build();
    const published: Array<{ channel: string; message: string }> = [];
    events.attachRedis({
      publish: async (channel: string, message: string) => {
        published.push({ channel, message });
        return 1;
      },
    } as never);

    await events.publish('garden:user-a', 'reaction', { reactionKey: 'wave' }, () => undefined);
    expect(published).toHaveLength(1);
    expect(published[0].channel).toBe('room:garden:user-a');
    expect(JSON.parse(published[0].message).type).toBe('reaction');
  });

  it('degrades gracefully when redis publish fails', async () => {
    const { events } = build();
    events.attachRedis({
      publish: async () => {
        throw new Error('connection lost');
      },
    } as never);

    let delivered = 0;
    await events.publish('garden:user-a', 'presence', { online: true }, () => {
      delivered += 1;
    });
    expect(delivered).toBe(1); // local delivery unaffected
    expect(events.lastSeq('garden:user-a')).toBe(1);
  });

  it('works without redis attached (single-instance mode)', async () => {
    const { events } = build();
    await events.publish('garden:user-a', 'visitor_join', { visitorId: 'u2' }, () => undefined);
    expect(events.lastSeq('garden:user-a')).toBe(1);
  });
});
