/**
 * Test setup: stub native-only modules (Skia, Reanimated, SafeArea) so the
 * Jest environment can exercise UI logic without a device.
 */
process.env.EXPO_NO_TELEMETRY = '1';

jest.mock('react-native-reanimated', () => {
  const Reanimated = require('react-native-reanimated/mock');
  Reanimated.default.useReducedMotion = jest.fn(() => false);
  return Reanimated;
});

jest.mock('react-native-safe-area-context', () => ({
  SafeAreaProvider: ({ children }) => children,
  useSafeAreaInsets: () => ({ top: 0, bottom: 0, left: 0, right: 0 }),
  SafeAreaView: ({ children }) => children,
}));

jest.mock('@shopify/react-native-skia', () => ({
  Canvas: ({ children }) => children,
  Group: ({ children }) => children,
  Circle: () => null,
  Rect: () => null,
  RoundedRect: () => null,
  Path: () => null,
  LinearGradient: () => null,
  vec: (x, y) => ({ x, y }),
}));

jest.mock('expo-router', () => {
  const actual = jest.requireActual('./__mocks__/expo-router');
  return actual;
});
