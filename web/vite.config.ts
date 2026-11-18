import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// In development, the Rust server runs separately: `vugent --port 7777`.
const server = process.env.VUGENT_SERVER ?? 'http://127.0.0.1:7777';

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  build: {
    // The vugent-server crate embeds the built UI. Building into the crate lets `cargo publish` package it, so that
    // `cargo install vugent` works without Node.
    outDir: '../crates/server/frontend',
    emptyOutDir: true,
  },
  server: {
    proxy: {
      '/api': server,
    },
  },
});
