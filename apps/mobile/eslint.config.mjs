import base from '@chaogarden/config/eslint';

/**
 * Mobile client lint rules.
 *
 * Package boundary enforcement: the mobile bundle must never import
 * server-only dependencies (NestJS, Prisma, Redis, native crypto) — those
 * would break the Expo bundle or leak secrets into the client.
 */
export default [
  ...base,
  {
    files: ['**/*.ts', '**/*.tsx'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: [
                '@nestjs/**',
                '@prisma/**',
                'ioredis',
                'socket.io*',
                'argon2',
                'reflect-metadata',
                'class-validator',
                'class-transformer',
              ],
              message:
                'Server-only module: the mobile client must talk to the API over HTTP/WS contracts (@chaogarden/contracts).',
            },
            {
              group: ['**/apps/api/**'],
              message: 'The mobile client must not import API server internals.',
            },
          ],
        },
      ],
    },
  },
];
