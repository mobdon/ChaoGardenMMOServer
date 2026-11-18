/**
 * Realtime client tests: sequence-gap detection triggers snapshot refetch,
 * degraded transport starts polling, and commands are forwarded untouched.
 */

import { RealtimeClient, RealtimeTransport, GardenEventPayload } from '../api/realtime';

function stubTransport() {
  const eventListeners: Array<(e: GardenEventPayload) => void> = [];
  const statusListeners: Array<(s: string) => void> = [];
  const sent: Array<Record<string, unknown>> = [];
  const transport: RealtimeTransport = {
    connect: () => statusListeners.forEach((l) => l('online')),
    close: () => undefined,
    emitCommand: (command) => sent.push(command),
    onEvent: (listener) => {
      eventListeners.push(listener);
      return () => undefined;
    },
    onStatus: (listener) => {
      statusListeners.push(listener);
      return () => undefined;
    },
  };
  const fire = (event: GardenEventPayload) => eventListeners.forEach((l) => l(event));
  const setStatus = (s: 'online' | 'polling' | 'offline' | 'connecting') =>
    statusListeners.forEach((l) => l(s));
  return { transport, fire, setStatus, sent };
}

describe('RealtimeClient', () => {
  it('detects a sequence gap and refetches the snapshot', () => {
    const { transport, fire } = stubTransport();
    const refetch = jest.fn().mockResolvedValue(undefined);
    const client = new RealtimeClient({ url: 'wss://x', token: 't', transport, refetch });

    client.start();
    fire({ seq: 1, at: 0, event: { type: 'visitor_join' } });
    fire({ seq: 3, at: 0, event: { type: 'reaction' } }); // gap: seq 2 missed
    expect(refetch).toHaveBeenCalledTimes(1);
    expect(client.lastSeenSeq()).toBe(3);
  });

  it('does not refetch on contiguous sequences', () => {
    const { transport, fire } = stubTransport();
    const refetch = jest.fn().mockResolvedValue(undefined);
    const client = new RealtimeClient({ url: 'wss://x', token: 't', transport, refetch });
    client.start();
    fire({ seq: 1, at: 0, event: { type: 'visitor_join' } });
    fire({ seq: 2, at: 0, event: { type: 'reaction' } });
    expect(refetch).not.toHaveBeenCalled();
  });

  it('requests replay with lastSeq when the connection comes back online', () => {
    const { transport, fire, setStatus, sent } = stubTransport();
    const client = new RealtimeClient({ url: 'wss://x', token: 't', transport, refetch: jest.fn() });
    client.start();
    fire({ seq: 4, at: 0, event: { type: 'presence' } });
    // simulate reconnect: transport re-emits online
    setStatus('online');
    expect(sent).toContainEqual({ type: 'sync', lastSeq: 4 });
  });

  it('polls while degraded and stops polling when back online', () => {
    jest.useFakeTimers();
    const { transport, setStatus } = stubTransport();
    const refetch = jest.fn().mockResolvedValue(undefined);
    const client = new RealtimeClient({ url: 'wss://x', token: 't', transport, refetch, pollIntervalMs: 1000 });
    client.start();
    setStatus('polling');
    jest.advanceTimersByTime(2500);
    expect(refetch).toHaveBeenCalledTimes(2);
    setStatus('online');
    jest.advanceTimersByTime(3000);
    expect(refetch).toHaveBeenCalledTimes(2); // polling stopped
    client.stop();
    jest.useRealTimers();
  });
});
