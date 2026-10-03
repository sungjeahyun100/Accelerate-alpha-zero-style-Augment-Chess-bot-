import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { demoPaths } from './scripts/paths.mjs';

const paths = demoPaths();

export default defineConfig({
  plugins: [svelte()],
  envDir: false,
  base: './',
  publicDir: paths.public,
  cacheDir: paths.viteCache,
  build: { outDir: paths.site, emptyOutDir: true, target: 'es2022' },
  worker: { format: 'es' },
  server: { host: '127.0.0.1', port: 4173, strictPort: true },
  preview: { host: '127.0.0.1', port: 4173, strictPort: true }
});
