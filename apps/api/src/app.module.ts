import { Module } from '@nestjs/common';
import { AuthModule } from './auth/auth.module';
import { GameGateway } from './ws/game.gateway';
import { PrismaService } from './database/prisma.service';
import { DomainModule } from './domain/domain.module';

@Module({
  imports: [AuthModule, DomainModule],
  providers: [PrismaService, GameGateway],
})
export class AppModule {}
