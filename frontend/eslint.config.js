import globals from 'globals'
import pluginJs from '@eslint/js'
import tseslint from 'typescript-eslint'
import vue from 'eslint-plugin-vue'
import stylistic from '@stylistic/eslint-plugin'

export default [
  stylistic.configs['recommended-flat'],
  pluginJs.configs.recommended,
  ...tseslint.configs.recommended,
  ...vue.configs['flat/recommended'],
  {
    files: ['**/*.{js,mjs,cjs,ts,vue}'],
    plugins: { '@stylistic': stylistic },
    rules: {
      '@stylistic/brace-style': ['error', '1tbs'],
    },
  },
  { languageOptions: { globals: globals.browser } },
  {
    files: ['**/*.vue'],
    languageOptions: {
      parserOptions: {
        parser: '@typescript-eslint/parser',
      },
    },
    plugins: { vue },
    rules: {
      'vue/script-indent': ['error', 2],
    },
  },
]
