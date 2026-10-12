import { defineConfig, devices } from '@playwright/test';

// The browser tests, in two projects:
// - gpu (e2e/*.spec.ts): the wind map's shaders on SwiftShader, in a test page Vite serves;
// - app (e2e/app/*.spec.ts): the whole app, the built SPA from the real server (a debug build,
//   `WF_BIN`), which takes everything from the outside from a stand-in (e2e/app/stand-in.mjs) and
//   its map from small textures written before it starts; nothing is downloaded.
// In CI they run in the official Playwright image; `PW_CHROMIUM` points at another Chromium where
// the image's is not installed.
const chromium = {
  ...devices['Desktop Chrome'],
  launchOptions: {
    executablePath: process.env.PW_CHROMIUM || undefined,
    args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
  },
};

export default defineConfig({
  forbidOnly: !!process.env.CI,
  // A software GPU per browser: more at once only makes each slower, and a scene of the look
  // tests alone takes some 20 seconds to paint
  workers: process.env.CI ? 2 : 3,
  timeout: 90_000,
  reporter: process.env.CI ? 'github' : 'list',
  // The approved pictures by test file and name only, whichever project; in CI a missing one
  // fails instead of being written
  snapshotPathTemplate: '{testDir}/{testFilePath}-snapshots/{arg}-{platform}{ext}',
  updateSnapshots: process.env.CI ? 'none' : 'missing',
  projects: [
    {
      name: 'gpu',
      testDir: 'e2e',
      testIgnore: 'app/**',
      use: { ...chromium, baseURL: 'http://127.0.0.1:5199' },
    },
    {
      name: 'app',
      testDir: 'e2e/app',
      // The map loads in 5 to 10 seconds on a software GPU, and the pages wait on it at times
      expect: { timeout: 15_000 },
      // Passkeys need a secure context: localhost is one.
      use: { ...chromium, baseURL: 'http://localhost:5299', locale: 'de-DE', timezoneId: 'Europe/Berlin' },
    },
  ],
  webServer: [
    {
      command: 'pnpm exec vite --port 5199 --strictPort --host 127.0.0.1',
      url: 'http://127.0.0.1:5199/e2e/gpu.html',
      reuseExistingServer: !process.env.CI,
    },
    {
      command: 'node e2e/app/start.mjs',
      url: 'http://localhost:5299/api/health',
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
    },
  ],
});
