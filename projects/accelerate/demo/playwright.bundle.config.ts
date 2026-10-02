import {defineConfig} from '@playwright/test';
import preview from './playwright.config';
import {demoPaths} from './scripts/paths.mjs';

export default defineConfig({
  ...preview,
  testMatch: 'bundled.spec.ts',
  use: {...preview.use, baseURL: 'http://127.0.0.1:4174/static-demo/'},
  webServer: {
    command: `npm exec --workspaces=false -- vite preview --outDir "${demoPaths().bundle}" --base /static-demo/ --host 127.0.0.1 --port 4174 --strictPort`,
    url: 'http://127.0.0.1:4174/static-demo/', reuseExistingServer: false, timeout: 30000,
  },
});
