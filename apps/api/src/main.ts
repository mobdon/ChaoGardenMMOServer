import { NestFactory } from '@nestjs/core';
import { Logger } from '@nestjs/common';
import { AppModule } from './app.module';
import { loadConfig } from './config/configuration';
import { validateEnv } from './config/env';
import { sanitize } from './auth/errors';

async function bootstrap() {
  // Fail fast on missing/invalid environment before wiring the framework.
  const env = validateEnv();
  const app = await NestFactory.create(AppModule, {
    // Exception filters/serializers downstream never log credentials.
    logger: ['error', 'warn', 'log'],
  });
  app.enableCors({ origin: true, credentials: true });
  app.setGlobalPrefix('api');

  const config = loadConfig({ ...process.env, PORT: String(env.PORT) });
  await app.listen(config.port);
  new Logger('Bootstrap').log(`api listening on :${config.port}`);
}

process.on('unhandledRejection', (reason) => {
  // Redact anything token-shaped from crash logs before printing.
  const message = reason instanceof Error ? reason.message : String(reason);
  // eslint-disable-next-line no-console
  console.error('unhandled rejection:', sanitize(message));
});
void bootstrap();
