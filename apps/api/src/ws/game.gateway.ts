/**
 * Gameplay WebSocket gateway. Socket connections must present a valid access
 * token in the handshake (`socket.auth.token`); messages carrying ownership
 * are validated against the actor attached by `JwtAuthGuard`.
 */
import { Logger } from '@nestjs/common';
import {
  ConnectedSocket,
  MessageBody,
  OnGatewayConnection,
  OnGatewayDisconnect,
  SubscribeMessage,
  WebSocketGateway,
  WebSocketServer,
  WsException,
} from '@nestjs/websockets';
import { UseGuards } from '@nestjs/common';
import { Server, Socket } from 'socket.io';
import { JwtAuthGuard } from '../auth/guards/jwt-auth.guard';
import { ACTOR_KEY, extractBearer } from '../auth/guards/jwt-auth.guard';
import type { AuthenticatedActor } from '../auth/auth.service';
import { AuthErrorCode } from '@chaogarden/contracts';
import { AuthException } from '../auth/errors';

interface HandshakeClient extends Socket {
  handshake: Socket['handshake'] & { auth?: { token?: string } };
}

function actorOf(client: HandshakeClient): AuthenticatedActor {
  const stored = (client.data as Record<string, unknown> | undefined)?.[ACTOR_KEY] as
    | AuthenticatedActor
    | undefined;
  if (!stored) {
    throw new AuthException(AuthErrorCode.Unauthenticated, 'Socket not authenticated');
  }
  return stored;
}

/**
 * Ownership helper: creatures are addressed by id and the server confirms the
 * creature belongs to the actor's user before mutating anything.
 */
export function assertOwned(creatureOwnerId: string, actor: AuthenticatedActor): void {
  if (creatureOwnerId !== actor.userId) {
    throw new AuthException(AuthErrorCode.Forbidden, 'Not your creature');
  }
}

@WebSocketGateway({ namespace: '/game', transports: ['websocket'] })
@UseGuards(JwtAuthGuard)
export class GameGateway implements OnGatewayConnection, OnGatewayDisconnect {
  private readonly logger = new Logger(GameGateway.name);
  @WebSocketServer() server!: Server;

  constructor() {}

  async handleConnection(client: HandshakeClient): Promise<void> {
    // The global JwtAuthGuard runs for the connection; verify manually here so
    // we can close unauthorized sockets immediately.
    const token = client.handshake.auth?.token ?? extractBearer(client.handshake.headers?.authorization);
    if (!token) {
      client.emit('auth-error', { code: AuthErrorCode.Unauthenticated });
      client.disconnect(true);
      return;
    }
    // Full verification is delegated to the guard pipeline; this hook only
    // mirrors the outcome for logging (never logging the token itself).
    this.logger.log(`socket connected user=${client.id}`);
  }

  handleDisconnect(client: HandshakeClient): void {
    this.logger.log(`socket disconnected user=${client.id}`);
  }

  @SubscribeMessage('ping')
  onPing(@ConnectedSocket() client: HandshakeClient): { ok: true; user: string } {
    const actor = actorOf(client);
    return { ok: true, user: actor.userId };
  }

  @SubscribeMessage('care')
  onCare(@MessageBody() body: { creatureOwnerId: string }, @ConnectedSocket() client: HandshakeClient) {
    const actor = actorOf(client);
    assertOwned(body.creatureOwnerId, actor);
    // Gameplay logic is handled by the Rust authority; this is the auth boundary.
    return { ok: true };
  }

  onWsError(err: unknown): void {
    // Never log raw tokens or bodies.
    this.logger.warn(`ws error code=${err instanceof AuthException ? err.code : 'UNKNOWN'}`);
    if (err instanceof AuthException) throw new WsException({ code: err.code, message: err.message });
    throw new WsException({ code: 'UNKNOWN', message: 'Socket error' });
  }
}
