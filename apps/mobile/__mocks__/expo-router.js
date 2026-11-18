/**
 * Minimal Expo Router mock for navigation tests. The mock router records
 * navigation calls so tests can assert flow transitions.
 */
const state = { pathname: '/', navigate: jest.fn(), replace: jest.fn(), back: jest.fn(), push: jest.fn() };

const Stack = ({ children }) => children;
Stack.Screen = () => null;

const Tabs = ({ children }) => children;
Tabs.Screen = () => null;

module.exports = {
  Stack,
  Tabs,
  Link: ({ children }) => children,
  useRouter: () => ({
    navigate: state.navigate,
    replace: state.replace,
    push: state.push,
    back: state.back,
  }),
  usePathname: () => state.pathname,
  useLocalSearchParams: (query) => {
    if (typeof query === 'object' && query !== null) {
      const out = {};
      for (const [k, v] of Object.entries(query)) out[k] = String(v);
      return out;
    }
    return {};
  },
  __testState: state,
};
