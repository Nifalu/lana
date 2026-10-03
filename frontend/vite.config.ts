import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  optimizeDeps: {
    exclude: ['maplibre-gl'],
  },
  // MapLibre starts its worker as an ES module (see src/lib/map/worker.ts).
  worker: {
    format: 'es',
  },
})
