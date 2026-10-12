import { defineConfig, devices } from '@playwright/test';

// The browser tests in e2e/: the wind map's shaders on the GPU (SwiftShader in headless
// Chromium). In CI they run in the official Playwright image; `PW_CHROMIUM` points at another
// Chromium where the image's is not installed.
export default defineConfig({
  testDir: 'e2e',
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: 'http://127.0.0.1:5199',
    ...devices['Desktop Chrome'],
    launchOptions: {
      executablePath: process.env.PW_CHROMIUM || undefined,
      args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
    },
  },
  webServer: {
    command: 'pnpm exec vite --port 5199 --strictPort --host 127.0.0.1',
    url: 'http://127.0.0.1:5199/e2e/gpu.html',
    reuseExistingServer: !process.env.CI,
  },
});
