/**
 * Gateway-level integration tests. These exercise command handling, ownership,
 * rate limiting, event fan-out, sequence recovery, duplicate commands, room
 * isolation, and notification counters against the in-memory service graph
 * (the same services used by REST).
 */

import { GameGateway } from '../src/ws/game.gateway';
import { PresenceService } from '../src/ws/presence.service';
import { RealtimeCommandService } from '../src/ws/realtime-command.service';
import { RealtimeEventService } from '../src/ws/realtime-event.service';
import { RealtimeSnapshotService } from '../src/ws/realtime-snapshot.service';
import { RealtimeGatewayConfig } from '../src/ws/gateway.config';

type Actor = { accountId: string; handle: string };

const ACTOR_A: Actor = { userId: 'user-a', handle: 'a', accountId: 'user-a' } as never;
const ACTOR_B: Actor = { userId: 'user-b', handle: 'b', accountId: 'user-b' } as never;

function socket(id: string, actor: Actor) {
  const data: Record<string, unknown> = { actor };
  const emitted: Array<{ room?: string; event: string; payload: unknown }> = [];
  return {
    id,
    data,
    emitted,
    emit: (event: string, payload: unknown, room?: string) => emitted.push({ event, payload, room }),
  } as never;
}

function buildGateway() {
  const presence = new PresenceService(new RealtimeGatewayConfig());
  const events = new RealtimeEventService(presence, new RealtimeGatewayConfig());
  const snapshots = new RealtimeSnapshotService();
  const commands = new RealtimeCommandService(presence, events, snapshots);
  const gateway = new GameGateway(presence, events, snapshots, commands);
  return { presence, events, snapshots, commands, gateway };
}

function lastSeqOf(events: RealtimeEventService, ownerId: string): number {
  return (events as unknown as { sequences: Map<string, number> }).sequences.get(`garden:${ownerId}`) ?? 0;
}

describe('game gateway', () => {
  it('rejects commands without an authenticated actor', async () => {
    const { gateway } = buildGateway();
    const client = { id: 's1', data: {}, emitted: [], emit: () => undefined } as never;
    await expect(gateway.handleCommand(client, { type: 'heartbeat' })).rejects.toThrow();
  });

  it('lets the owner open their own garden room silently, then announces visitors', async () => {
    const { gateway } = buildGateway();
    const owner = socket('s-owner', ACTOR_A);
    const visitor = socket('s-visit', ACTOR_B);

    await gateway.handleCommand(owner, { type: 'join_garden', ownerId: 'user-a' });
    expect(lastSeqOf(gateway['events'], 'user-a')).toBe(0); // no visitor_join for the owner

    await gateway.handleCommand(visitor, { type: 'join_garden', ownerId: 'user-a' });
    expect(lastSeqOf(gateway['events'], 'user-a')).toBe(1);
  });

  it('is idempotent for duplicate join commands', async () => {
    const { gateway } = buildGateway();
    const owner = socket('s-owner', ACTOR_A);
    const visitor = socket('s-visit', ACTOR_B);

    await gateway.handleCommand(owner, { type: 'join_garden', ownerId: 'user-a' });
    await gateway.handleCommand(visitor, { type: 'join_garden', ownerId: 'user-a' });
    await gateway.handleCommand(visitor, { type: 'join_garden', ownerId: 'user-a' });
    expect(lastSeqOf(gateway['events'], 'user-a')).toBe(1);
  });

  it('fans reactions out only to garden occupants and rejects non-members', async () => {
    const { gateway } = buildGateway();
    const owner = socket('s-owner', ACTOR_A);
    const visitor = socket('s-visit', ACTOR_B);
    const outsider = socket('s-out', { accountId: 'user-out', handle: 'out' });

    await gateway.handleCommand(owner, { type: 'join_garden', ownerId: 'user-a' });
    await gateway.handleCommand(visitor, { type: 'join_garden', ownerId: 'user-a' });

    // outsider tries to react without being in the garden
    await expect(
      gateway.handleCommand(outsider, { type: 'send_reaction', ownerId: 'user-a', reactionKey: 'wave' }),
    ).rejects.toThrow(/not in garden/);

    const before = lastSeqOf(gateway['events'], 'user-a');
    await gateway.handleCommand(visitor, { type: 'send_reaction', ownerId: 'user-a', reactionKey: 'cheer' });
    expect(lastSeqOf(gateway['events'], 'user-a')).toBe(before + 1);

    // invalid catalog key rejected before persistence
    await expect(
      gateway.handleCommand(visitor, { type: 'send_reaction', ownerId: 'user-a', reactionKey: 'not-a-reaction' }),
    ).rejects.toThrow(/reaction/);
  });

  it('emits visitor_join and visitor_leave events with sequences', async () => {
    const { gateway } = buildGateway();
    const owner = socket('s-owner', ACTOR_A);
    const visitor = socket('s-visit', ACTOR_B);

    await gateway.handleCommand(owner, { type: 'join_garden', ownerId: 'user-a' });
    await gateway.handleCommand(visitor, { type: 'join_garden', ownerId: 'user-a' });
    await gateway.handleCommand(visitor, { type: 'leave_garden', ownerId: 'user-a' });

    const seq = lastSeqOf(gateway['events'], 'user-a');
    expect(seq).toBeGreaterThanOrEqual(2);
  });

  it('rate limits reaction bursts', async () => {
    const { gateway } = buildGateway();
    const owner = socket('s-owner', ACTOR_A);
    await gateway.handleCommand(owner, { type: 'join_garden', ownerId: 'user-a' });
    for (let i = 0; i < 12; i++) {
      await gateway.handleCommand(owner, { type: 'send_reaction', ownerId: 'user-a', reactionKey: 'wave' });
    }
    await expect(
      gateway.handleCommand(owner, { type: 'send_reaction', ownerId: 'user-a', reactionKey: 'wave' }),
    ).rejects.toThrow(/rate/i);
  });

  it('rejects garden join above the per-socket room limit', async () => {
    const { gateway } = buildGateway();
    const visitor = socket('s-visit', ACTOR_B);
    for (let i = 0; i < 8; i++) {
      await gateway.handleCommand(visitor, { type: 'join_garden', ownerId: `owner-${i}` });
    }
    await expect(gateway.handleCommand(visitor, { type: 'join_garden', ownerId: 'owner-overflow' })).rejects.toThrow(
      /too many/i,
    );
  });

  it('recoverSync replays buffered events when the client is behind, else falls back to a snapshot', async () => {
    const { gateway } = buildGateway();
    const owner = socket('s-owner', ACTOR_A);
    await gateway.handleCommand(owner, { type: 'join_garden', ownerId: 'user-a' });
    const visitor = socket('s-visit', ACTOR_B);
    await gateway.handleCommand(visitor, { type: 'join_garden', ownerId: 'user-a' });
    await gateway.handleCommand(visitor, { type: 'send_reaction', ownerId: 'user-a', reactionKey: 'sparkle' });

    const current = lastSeqOf(gateway['events'], 'user-a');
    const result = gateway.recoverSync(owner, 'garden:user-a', current - 1);
    expect(result.recovered).toBe(true);
    expect(result.events.length).toBeGreaterThan(0);

    const tooFar = gateway.recoverSync(owner, 'garden:user-a', 0);
    expect(tooFar.recovered).toBe(false);
    expect(tooFar.snapshot).not.toBeNull();
    expect(tooFar.snapshot!.seq).toBe(current);
  });

  it('moves decorations only for the garden owner', async () => {
    const { gateway } = buildGateway();
    const owner = socket('s-owner', ACTOR_A);
    const visitor = socket('s-visit', ACTOR_B);

    await gateway.handleCommand(owner, { type: 'join_garden', ownerId: 'user-a' });
    await gateway.handleCommand(visitor, { type: 'join_garden', ownerId: 'user-a' });

    await expect(
      gateway.handleCommand(visitor, { type: 'move_decoration', ownerId: 'user-a', decorationId: 'd1', slot: 'center' }),
    ).rejects.toThrow(/owner/);

    await gateway.handleCommand(owner, { type: 'move_decoration', ownerId: 'user-a', decorationId: 'd1', slot: 'center' });
    const stored = gateway['snapshots'].lastDecoration('user-a');
    expect(stored).toEqual({ id: 'd1', slot: 'center', catalogId: 'd1' });
  });

  it('increments and resets notification counters', () => {
    const { presence } = buildGateway();
    presence.pushNotification('user-a', 'visit');
    presence.pushNotification('user-a', 'reaction');
    expect(presence.unreadCount('user-a')).toBe(2);
    presence.resetUnread('user-a');
    expect(presence.unreadCount('user-a')).toBe(0);
  });
});
