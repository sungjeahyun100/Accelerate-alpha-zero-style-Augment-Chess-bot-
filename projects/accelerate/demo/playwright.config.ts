import {defineConfig} from '@playwright/test';
import {demoPaths} from './scripts/paths.mjs';
const paths = demoPaths();
export default defineConfig({
  testDir: './tests', testMatch: 'browser.spec.ts', timeout: 30000, workers: 1,
  outputDir: paths.playwright, reporter: [['list']],
  use: {baseURL: 'http://127.0.0.1:4173', browserName: 'chromium', headless: true},
  projects: process.env.ACCELERATE_BROWSER_CHANNELS === 'desktop'
    ? [{name: 'Chrome', use: {channel: 'chrome'}}, {name: 'Edge', use: {channel: 'msedge'}}]
    : [{name: 'Chromium'}],
  webServer: {command: 'npm exec --workspaces=false -- vite preview --host 127.0.0.1 --port 4173 --strictPort', url: 'http://127.0.0.1:4173', reuseExistingServer: false, timeout: 30000},
});
