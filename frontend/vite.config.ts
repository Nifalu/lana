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
  server: {
    // Dev only: `VITE_API_URL=/heitzli npm run dev` reaches the lana API
    // same-origin, whether or not the API answers CORS preflights.
    proxy: {
      '/heitzli': {
        target: 'https://lana.heitzli.ch',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/heitzli/, ''),
      },
    },
  },
})
