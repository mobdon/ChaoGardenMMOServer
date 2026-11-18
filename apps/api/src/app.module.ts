import { Module } from '@nestjs/common';
import { AuthModule } from './auth/auth.module';
import { GameGateway } from './ws/game.gateway';
import { PresenceService } from './ws/presence.service';
import { RealtimeEventService } from './ws/realtime-event.service';
import { RealtimeSnapshotService } from './ws/realtime-snapshot.service';
import { RealtimeCommandService } from './ws/realtime-command.service';
import { RealtimeGatewayConfig } from './ws/gateway.config';
import { PrismaService } from './database/prisma.service';
import { DomainModule } from './domain/domain.module';

@Module({
  imports: [AuthModule, DomainModule],
  providers: [
    PrismaService,
    RealtimeGatewayConfig,
    PresenceService,
    RealtimeEventService,
    RealtimeSnapshotService,
    RealtimeCommandService,
    GameGateway,
  ],
})
export class AppModule {}
