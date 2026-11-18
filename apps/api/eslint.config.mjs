import base from '@chaogarden/config/eslint';

/** API server lint rules. */
export default [
  ...base,
  {
    files: ['**/*.ts'],
    rules: {
      // Keep server code from reaching into other workspaces' internals.
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['@chaogarden/mobile', '@chaogarden/mobile/**'],
              message: 'The API must not depend on the mobile client.',
            },
            {
              group: ['@chaogarden/simulation/**'],
              message: 'Import the simulation package by its public entry point only.',
            },
          ],
        },
      ],
    },
  },
];
