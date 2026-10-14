/**
 * The pictures of the README and the documentation (docs/*.png), from the app against the
 * stand-in, so every run shows the same data. Not part of `pnpm e2e`: `pnpm screenshots` takes
 * them, the "Update screenshots" workflow commits them. In English, as the README and the docs.
 */
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { makeAdmin, signUp, someone } from './accounts';
import { expect, test } from './fixtures';

const DOCS = resolve(import.meta.dirname, '../../../docs');
const shot = (name: string) => resolve(DOCS, `${name}.png`);

test.use({ locale: 'en-GB' });

const desktop = { width: 1440, height: 900 };
const phone = { width: 390, height: 844 };

/** The theme before the first paint, as the toggle keeps it (index.html) */
async function theme(page: Page, t: 'light' | 'dark') {
  await page.addInitScript((t) => localStorage.setItem('wf.theme', t), t);
  await page.emulateMedia({ colorScheme: t });
}

/** Opens the map at `url` and waits until its wind is drawn: a texture in (the wait starts
 * before the page, so a fast load cannot slip past it) and the particles under way */
async function openMap(page: Page, url: string) {
  const texture = page.waitForResponse(
    (r) => r.ok() && /^\/api\/map\/icon_d2\/\d{10}\/\d{3}\.png$/.test(new URL(r.url()).pathname),
    { timeout: 45_000 },
  );
  await page.goto(url);
  await texture;
  await page.waitForTimeout(15_000);
}

test.describe('@screenshots', () => {
  test('the wind map', async ({ page }) => {
    await page.setViewportSize(desktop);
    await theme(page, 'dark');
    await openMap(page, '/map?spot=48.7794%2C8.0805&station=edsb#4.6/52.5/-4');
    await expect(page.getByRole('region', { name: 'Karlsruhe Baden-Baden Airport' })).toBeVisible();
    await page.screenshot({ path: shot('screenshot') });
  });

  test('the wind map with gusts', async ({ page }) => {
    await page.setViewportSize(desktop);
    await theme(page, 'dark');
    await openMap(page, '/map?layer=gusts#4.6/52.5/-4');
    await page.waitForTimeout(1_500);
    await page.screenshot({ path: shot('screenshot-map-gusts') });
  });

  test('a station page: live wind, the ten days at a glance', async ({ page }) => {
    await page.setViewportSize(desktop);
    await theme(page, 'light');
    await page.goto('/station/edsb');
    await expect(page.getByRole('region', { name: 'At a glance' })).toBeVisible();
    await page.waitForTimeout(1_000);
    await page.screenshot({ path: shot('screenshot-station') });
  });

  test('the forecast: quick view and day cards', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 1400 });
    await theme(page, 'light');
    await page.goto('/station/edsb');
    // The quick view at the top, under the header
    await page
      .getByRole('region', { name: 'At a glance' })
      .evaluate((el) => window.scrollBy(0, el.getBoundingClientRect().top - 90));
    await page.waitForTimeout(800);
    await page.screenshot({ path: shot('screenshot-forecast') });
  });

  test('the forecast in the dark', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 1100 });
    await theme(page, 'dark');
    await page.goto('/station/edsb');
    const day = page.locator('section[id^="day-"]').nth(1);
    await day.scrollIntoViewIfNeeded();
    await page.waitForTimeout(800);
    await day.screenshot({ path: shot('screenshot-day-dark') });
  });

  test('the model ranking and the comparison chart', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 1000 });
    await theme(page, 'light');
    await page.goto('/station/edsb');
    // The chart and the ranking side by side, as one picture
    const row = page.locator('[aria-labelledby="compare-title"]').locator('..');
    await row.scrollIntoViewIfNeeded();
    await page.waitForTimeout(800);
    await row.screenshot({ path: shot('screenshot-models') });
  });

  test('on a phone: station and map', async ({ page }) => {
    await page.setViewportSize(phone);
    await theme(page, 'dark');
    await page.goto('/station/edsb');
    await expect(page.getByRole('region', { name: 'At a glance' })).toBeVisible();
    await page.waitForTimeout(800);
    await page.screenshot({ path: shot('screenshot-phone-station') });
    await openMap(page, '/map?spot=48.7794%2C8.0805&station=edsb#4/53/-6');
    await page.screenshot({ path: shot('screenshot-phone-map') });
  });

  test('the admin page: what the data folder holds, and its care', async ({ page }) => {
    await page.setViewportSize(desktop);
    await theme(page, 'dark');
    const who = { ...someone('Ada'), name: 'Ada' };
    await signUp(page, who, 'en');
    makeAdmin(who.email);
    await page.goto('/admin?tab=storage');
    await expect(page.getByRole('heading', { name: 'Tables' })).toBeVisible();
    await page.waitForTimeout(800);
    await page.screenshot({ path: shot('screenshot-admin'), fullPage: true });
  });
});
