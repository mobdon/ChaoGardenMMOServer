/**
 * Command pipeline for realtime messages. Every client message is a command:
 * authenticate → validate shape → rate limit → authorize → persist → fan out.
 * This is the single path; there is no relay of client-authored payloads.
 */
import { Logger } from '@nestjs/common';
import type { RealtimeCommand } from '@chaogarden/contracts';
import { PresenceService } from './presence.service';
import { RealtimeEventService } from './realtime-event.service';
import { RealtimeSnapshotService } from './realtime-snapshot.service';

type Actor = { accountId: string; handle: string };
type SocketLike = { id: string; data: Record<string, unknown>; rooms: Set<string>; emit: (event: string, body: unknown, room?: string) => void };
type ServerLike = { emit: (event: string, body: unknown, room?: string) => void };

export class RealtimeCommandService {
  private readonly logger = new Logger(RealtimeCommandService.name);

  constructor(
    private readonly presence: PresenceService,
    private readonly events: RealtimeEventService,
    private readonly snapshots: RealtimeSnapshotService,
  ) {}

  private actorOf(socket: SocketLike): Actor {
    const actor = socket.data?.['actor'] as Actor | undefined;
    if (!actor) throw new Error('UNAUTHENTICATED: socket has no authenticated actor');
    return actor;
  }

  private guardRate(socket: SocketLike, actor: Actor, isReaction: boolean): void {
    if (isReaction) {
      if (!this.presence.reactionAllowed(actor.accountId)) throw new Error('RATE_LIMITED: reaction rate exceeded');
      return;
    }
    if (!this.presence.commandAllowed(socket.id)) throw new Error('RATE_LIMITED: command rate exceeded');
  }

  private roomOf(ownerId: string): string {
    return `garden:${ownerId}`;
  }

  async handle(
    socket: SocketLike,
    _server: ServerLike,
    raw: unknown,
    emit: (room: string, event: string, body: unknown) => void,
  ): Promise<void> {
    const actor = this.actorOf(socket);
    const command = this.validate(raw);
    const isReaction = command.type === 'send_reaction';
    this.guardRate(socket, actor, isReaction);

    switch (command.type) {
      case 'join_garden': {
        const roomId = this.roomOf(command.ownerId);
        if (this.presence.inRoom(roomId, socket.id)) return; // idempotent — no duplicate events
        if (this.presence.roomsOf(socket.id).length >= 8) throw new Error('ROOM_LIMIT: too many gardens');
        if (this.presence.isRoomFull(roomId)) throw new Error('ROOM_FULL: garden is full');
        this.presence.joinRoom(roomId, socket.id);
        this.presence.touchHeartbeat(socket.id);
        // The owner joining their own garden is not a "visit".
        if (command.ownerId !== actor.accountId) {
          await this.events.publish(roomId, 'visitor_join', { visitor: { accountId: actor.accountId, handle: actor.handle } }, emit);
        }
        return;
      }
      case 'leave_garden': {
        const roomId = this.roomOf(command.ownerId);
        if (!this.presence.inRoom(roomId, socket.id)) return; // idempotent
        this.presence.leaveRoom(roomId, socket.id);
        await this.events.publish(roomId, 'visitor_leave', { visitorId: actor.accountId }, emit);
        return;
      }
      case 'send_reaction': {
        if (!this.events.isApprovedReaction(command.reactionKey)) {
          throw new Error('INVALID_PAYLOAD: unknown reaction key');
        }
        const roomId = this.roomOf(command.ownerId);
        if (!this.presence.inRoom(roomId, socket.id)) throw new Error(`FORBIDDEN: not in garden ${roomId}`);
        await this.events.publish(
          roomId,
          'reaction',
          { from: { accountId: actor.accountId, handle: actor.handle }, reactionKey: command.reactionKey },
          emit,
        );
        return;
      }
      case 'move_decoration': {
        const roomId = this.roomOf(command.ownerId);
        if (command.ownerId !== actor.accountId) {
          throw new Error('FORBIDDEN: only the garden owner may move decorations');
        }
        if (!this.presence.inRoom(roomId, socket.id)) this.presence.joinRoom(roomId, socket.id);
        const decoration = this.snapshots.moveDecoration(command.ownerId, command.decorationId, command.slot);
        await this.events.publish(roomId, 'decoration_update', { decoration }, emit);
        return;
      }
      case 'heartbeat': {
        this.presence.touchHeartbeat(socket.id);
        return;
      }
      default:
        throw new Error('INVALID_PAYLOAD: unknown command type');
    }
  }

  /** Shape validation only; authorization happens per-case above. */
  private validate(raw: unknown): RealtimeCommand {
    if (typeof raw !== 'object' || raw === null || typeof (raw as Record<string, unknown>).type !== 'string') {
      throw new Error('INVALID_PAYLOAD: malformed command');
    }
    const type = (raw as Record<string, unknown>).type;
    const body = raw as Record<string, unknown>;
    if (
      type !== 'join_garden' &&
      type !== 'leave_garden' &&
      type !== 'send_reaction' &&
      type !== 'move_decoration' &&
      type !== 'heartbeat'
    ) {
      throw new Error('INVALID_PAYLOAD: unknown command type');
    }
    if (type !== 'heartbeat') {
      const ownerId = body.ownerId;
      if (typeof ownerId !== 'string' || ownerId.length === 0 || ownerId.length > 64 || !/^[A-Za-z0-9_-]+$/.test(ownerId)) {
        throw new Error('INVALID_PAYLOAD: ownerId');
      }
    }
    if (type === 'send_reaction') {
      const key = body.reactionKey;
      if (typeof key !== 'string' || key.length === 0 || key.length > 32) {
        throw new Error('INVALID_PAYLOAD: reactionKey');
      }
    }
    if (type === 'move_decoration') {
      for (const field of ['decorationId', 'slot'] as const) {
        const value = body[field];
        if (typeof value !== 'string' || value.length === 0 || value.length > 64) {
          throw new Error(`INVALID_PAYLOAD: ${field}`);
        }
      }
    }
    return raw as RealtimeCommand;
  }
}
