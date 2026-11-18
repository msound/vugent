import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// In development, the Rust server runs separately: `vugent --port 7777`.
const server = process.env.VUGENT_SERVER ?? 'http://127.0.0.1:7777';

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  server: {
    proxy: {
      '/api': server,
    },
  },
});
