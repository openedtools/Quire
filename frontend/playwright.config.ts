import { defineConfig } from '@playwright/test';

// UI screenshots and keyboard checks against the Next dev server with a mocked Tauri core.
// Run: pnpm test:ui   (screenshots land in tests/ui/screenshots, which git ignores)
export default defineConfig({
  testDir: './tests/ui',
  testMatch: '**/*.ui.ts',
  timeout: 120_000,
  workers: 1,
  reporter: 'list',
  use: {
    baseURL: 'http://localhost:3118',
    viewport: { width: 1280, height: 800 },
  },
  projects: [
    { name: 'light', use: { colorScheme: 'light' } },
    { name: 'dark', use: { colorScheme: 'dark' } },
  ],
  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:3118',
    reuseExistingServer: true,
    timeout: 180_000,
  },
});
