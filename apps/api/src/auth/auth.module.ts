import { Module } from '@nestjs/common';
import { JwtModule } from '@nestjs/jwt';
import { AuthController } from './auth.controller';
import { AuthService } from './auth.service';
import { TokenService } from './token.service';
import { JwtAuthGuard } from './guards/jwt-auth.guard';
import { APP_GUARD } from '@nestjs/core';
import { PrismaService } from '../database/prisma.service';
import { REDIS_PROVIDER } from '../redis/redis.provider';
import { ThrottleService } from './throttle/throttle.service';
import { RateLimitGuard } from './throttle/rate-limit.guard';
import { loadConfig } from '../config/configuration';

@Module({
  imports: [
    JwtModule.registerAsync({
      useFactory: () => {
        const config = loadConfig();
        return { secret: config.jwtSecret, signOptions: { expiresIn: '15m' } };
      },
    }),
  ],
  controllers: [AuthController],
  providers: [
    PrismaService,
    REDIS_PROVIDER,
    ThrottleService,
    AuthService,
    TokenService,
    RateLimitGuard,
    // Global auth guard: every route requires a live access token unless @Public().
    { provide: APP_GUARD, useExisting: JwtAuthGuard },
    JwtAuthGuard,
  ],
  exports: [AuthService, TokenService, JwtAuthGuard],
})
export class AuthModule {}
