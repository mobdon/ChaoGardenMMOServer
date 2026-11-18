/**
 * Presence and garden realtime gateway. Socket.IO namespace `/game`.
 *
 * Every client message is a command that goes through the shared pipeline in
 * RealtimeCommandService: authentication, validation, rate limiting,
 * authorization, persistence, then sequence-stamped fan-out. The gateway never
 * relays client-authored stats, currency, or freeform payloads.
 */
import { Logger } from '@nestjs/common';
import { OnGatewayConnection, OnGatewayDisconnect, WebSocketGateway, WebSocketServer } from '@nestjs/websockets';
import { Server, Socket } from 'socket.io';
import type { SyncResponse } from '@chaogarden/contracts';
import { PresenceService } from './presence.service';
import { RealtimeEventService } from './realtime-event.service';
import { RealtimeSnapshotService } from './realtime-snapshot.service';
import { RealtimeCommandService } from './realtime-command.service';

@WebSocketGateway({ namespace: '/game', transports: ['websocket'] })
export class GameGateway implements OnGatewayConnection, OnGatewayDisconnect {
  private readonly logger = new Logger(GameGateway.name);
  @WebSocketServer() server!: Server;

  constructor(
    private readonly presence: PresenceService,
    private readonly events: RealtimeEventService,
    private readonly snapshots: RealtimeSnapshotService,
    private readonly commands: RealtimeCommandService,
  ) {}

  async handleConnection(socket: Socket): Promise<void> {
    const actor = (socket.data as Record<string, unknown>)?.['actor'] as
      | { accountId: string; handle: string }
      | undefined;
    if (!actor) {
      socket.emit('realtime_error', { code: 'UNAUTHENTICATED' });
      socket.disconnect(true);
      return;
    }
    this.presence.trackSocket(socket.id, actor);
    this.presence.touchHeartbeat(socket.id);
    socket.emit('presence_ack', { at: Date.now() });
  }

  async handleDisconnect(socket: Socket): Promise<void> {
    const actor = this.presence.actorOf(socket.id);
    const rooms = this.presence.forgetSocket(socket.id);
    for (const room of rooms) {
      await this.events.publish(room, 'visitor_leave', { visitorId: actor?.accountId ?? socket.id }, (r, e, b) =>
        this.server?.to(r).emit(e, b),
      );
    }
  }

  /**
   * Single entry point for all client commands. Errors are converted to typed
   * realtime errors on the socket, then rethrown so callers/tests observe the
   * rejection. Raw payloads and stack traces never reach the client.
   */
  async handleCommand(socket: Socket, raw: unknown): Promise<void> {
    try {
      await this.commands.handle(socket, this.server, raw, (room, event, body) =>
        this.server?.to(room).emit(event, body),
      );
    } catch (err) {
      this.logger.warn(`command rejected socket=${socket.id}`);
      socket.emit('realtime_error', { code: this.codeOf(err) });
      throw err;
    }
  }

  /**
   * Reconnect recovery. Replays buffered events when possible, otherwise
   * instructs the client to refetch a snapshot.
   */
  recoverSync(socket: Socket, roomId: string, afterSeq: number): SyncResponse {
    const replay = this.events.replay(roomId, afterSeq);
    const current = this.events.lastSeq(roomId);
    if (replay) {
      return {
        recovered: true,
        seq: current,
        events: replay.map((record) => ({ ...record, gardenId: roomId })) as never,
        snapshot: null,
      };
    }
    const actor = (socket.data as Record<string, unknown>)?.['actor'] as { accountId: string } | undefined;
    const unread = this.presence.unreadCount(actor?.accountId ?? '');
    const snapshot = this.snapshots.build(roomId, unread, current);
    return { recovered: false, seq: current, events: [], snapshot };
  }

  private codeOf(err: unknown): string {
    const message = err instanceof Error ? err.message : '';
    const code = message.split(':')[0];
    return code || 'UNKNOWN';
  }
}
