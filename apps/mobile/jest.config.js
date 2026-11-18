module.exports = {
  preset: 'jest-expo',
  setupFiles: ['./jest.setup.js'],
  testMatch: ['**/__tests__/**/*.test.(ts|tsx)'],
  transformIgnorePatterns: [
    'node_modules/(?!((jest-)?react-native|@react-native(-community)?|expo(nent)?|@expo(nent)?|@shopify/react-native-skia|@tanstack|zustand)*)',
  ],
  moduleNameMapper: {
    '^@/(.*)$': '<rootDir>/src/$1',
    '^@chaogarden/contracts$': '<rootDir>/../../packages/contracts/src/index.ts',
  },
};
