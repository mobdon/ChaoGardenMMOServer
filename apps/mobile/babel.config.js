module.exports = function preset(api) {
  api.cache(true);
  return {
    presets: [['babel-preset-expo', { jsxImportSource: 'react' }], 'module:react-native-reanimated/plugin'],
  };
};
