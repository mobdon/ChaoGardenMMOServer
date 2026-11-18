import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);

/** Bridge tests run with the repo's shared jest-expo config. */
const config = {
  preset: 'jest-expo',
  testMatch: ['**/__tests__/**/*.test.(ts|tsx)'],
  moduleNameMapper: {
    '^@chaogarden/contracts$': '<rootDir>/../../packages/contracts/src/all.ts',
  },
};

export default config;
