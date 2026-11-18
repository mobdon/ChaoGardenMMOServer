/**
 * Shared ESLint flat config for the ChaoGarden monorepo.
 *
 * Projects import this with `config.options.projectService` handling their own
 * typed-lint target, then layer project-specific rules (e.g. boundary rules
 * that keep the mobile client from importing server-only modules).
 */
import tseslint from 'typescript-eslint';
import prettier from 'eslint-config-prettier';

/** @type {import('eslint').Linter.Config} */
export const base = tseslint.config(
  {
    ignores: [
      '**/dist/**',
      '**/build/**',
      '**/.expo/**',
      '**/coverage/**',
      '**/node_modules/**',
      '**/babel.config.*',
      '**/jest.setup.*',
    ],
  },
  ...tseslint.configs.recommended,
  {
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: 'module',
    },
    rules: {
      '@typescript-eslint/consistent-type-imports': 'warn',
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
      'no-console': 'off',
      eqeqeq: ['error', 'smart'],
      'no-throw-literal': 'error',
    },
  },
  {
    files: ['**/*.test.ts', '**/*.test.tsx', '**/*.spec.ts', '**/*.spec.tsx'],
    rules: {
      '@typescript-eslint/no-non-null-assertion': 'off',
    },
  },
  prettier,
);

export default base;
