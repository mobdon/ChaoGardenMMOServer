import { Module } from '@nestjs/common';
import { AuthModule } from './auth/auth.module';
import { GameGateway } from './ws/game.gateway';
import { PrismaService } from './database/prisma.service';

@Module({
  imports: [AuthModule],
  providers: [PrismaService, GameGateway],
})
export class AppModule {}
