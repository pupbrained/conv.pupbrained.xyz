import { defineConfig, presetUno, presetAttributify } from 'unocss'

export default defineConfig({
  content: {
    filesystem: [
      '**/*.{html,js,ts,jsx,tsx,vue,svelte,astro}',
    ],
  },
  presets: [
    presetAttributify(),
    presetUno(),
  ],
})
