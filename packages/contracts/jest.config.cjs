/** Contract tests (widget snapshot schema + sanitizer) run with jest-expo. */
const config = {
  preset: 'jest-expo',
  testMatch: ['<rootDir>/test/**/*.test.ts'],
  transformIgnorePatterns: [
    'node_modules/(?!((jest-)?react-native|expo(nent)?|@expo(nent)?|zustand)*)',
  ],
};

export default config;
