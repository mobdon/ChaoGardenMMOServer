import base from '@chaogarden/config/eslint';

/** Simulation is pure TypeScript: forbid node-only and framework imports. */
export default [
  ...base,
  {
    files: ['src/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['node:*', 'fs', 'path', 'crypto', 'react', 'react-native', '@nestjs/**'],
              message:
                'The simulation package is a pure, deterministic domain library — no platform dependencies.',
            },
          ],
        },
      ],
    },
  },
];
