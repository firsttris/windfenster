/**
 * The app tests' `test`: every page error, console error and server error (5xx) fails the test it
 * happens in, so a page that renders but throws on the way does not pass. Chrome's own line for
 * a 4xx ("Failed to load resource") does not count: a wrong password is meant to get a 401.
 */
import { test as base, expect } from '@playwright/test';

export const test = base.extend<{ errors: string[] }>({
  errors: [
    async ({ page }, use) => {
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(`page error: ${e.message}`));
      page.on('console', (msg) => {
        if (msg.type() === 'error' && !msg.text().startsWith('Failed to load resource'))
          errors.push(`console: ${msg.text()}`);
      });
      page.on('response', (r) => {
        if (r.status() >= 500) errors.push(`server: ${r.status()} ${new URL(r.url()).pathname}`);
      });
      await use(errors);
      expect(errors, 'errors in the browser').toEqual([]);
    },
    { auto: true },
  ],
});

export { expect };
