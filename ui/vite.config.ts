import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'node:path';

// Vite config for the Agentyx UI (Svelte 5 + TypeScript strict).
//
// Tauri-specific notes:
// - We bind to port 1420 to match `tauri.conf.json` `devUrl`.
// - We disable HMR overlay in Tauri WebView (handled by the env var).
// - The `clearScreen: false` keeps tauri-build output visible.

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [svelte()],

  // Tauri expects a fixed port; fail rather than auto-increment.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: {
      // Don't watch the Rust source tree from the UI process.
      ignored: ['**/crates/**', '**/target/**'],
    },
  },

  // Build the UI into ../crates/agentyx-app/../ui/dist (matches
  // tauri.conf.json `frontendDist`).
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: !!process.env.TAURI_DEBUG,
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
  },

  resolve: {
    alias: {
      $lib: path.resolve(__dirname, 'src/lib'),
    },
  },

  // Vitest test config (used by `bun run test` in ui/).
  test: {
    environment: 'jsdom',
    include: ['src/**/*.{test,spec}.{js,ts}'],
    setupFiles: ['./src/test-setup.ts'],
  },
}));
