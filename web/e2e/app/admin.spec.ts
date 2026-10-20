/**
 * The admin page against the real server: an account made an admin on the command line, as an
 * operator does (`windfenster admin`), then every section of the page, a setting changed and
 * changed back, and someone who is no admin kept out.
 */
import { makeAdmin, signUp, someone } from './accounts';
import { expect, test } from './fixtures';

test('someone who is no admin is kept out', async ({ page }) => {
  await signUp(page, someone('Nina'));
  await expect(page.getByRole('link', { name: 'Verwaltung' })).toHaveCount(0);
  await page.goto('/admin');
  await expect(page.getByText('Die Verwaltung ist nur für Admins.')).toBeVisible();
});

test('an admin goes through every section', async ({ page }) => {
  const who = someone('Ada');
  await signUp(page, who);
  makeAdmin(who.email);
  await page.reload();
  await page.getByRole('link', { name: 'Verwaltung' }).click();
  await expect(page).toHaveURL(/\/admin$/);
  const menu = page.getByRole('navigation', { name: 'Bereiche der Verwaltung' });
  for (const [section, heading] of [
    ['Übersicht', 'Übersicht'],
    ['Karte und Basemap', 'Karte und Basemap'],
    ['Vorhersagen', 'Vorhersagen'],
    ['Messwerte und Stationen', 'Messwerte und Stationen'],
    ['Benutzer', 'Benutzer'],
    ['Alarme und Mail', 'Alarme und Mail'],
    ['Speicher und Datenbank', 'Speicher und Datenbank'],
    ['System', 'System'],
  ]) {
    await menu.getByRole('link', { name: section }).click();
    await expect(page.getByRole('heading', { level: 1, name: heading })).toBeVisible();
  }

  // The overview: the tasks and what needs a decision; this server runs no loops (WF_SCHEDULER)
  await menu.getByRole('link', { name: 'Übersicht' }).click();
  await expect(page.getByRole('heading', { name: 'Hintergrundaufgaben' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Open-Meteo heute' })).toBeVisible();

  // Users: the admin's own account, with its role
  await menu.getByRole('link', { name: 'Benutzer' }).click();
  await page.getByLabel('Name oder E-Mail …').fill(who.email);
  await expect(page.getByRole('button', { name: new RegExp(who.name) })).toContainText('Admin');

  // A setting changed and stored, then back
  await menu.getByRole('link', { name: 'Vorhersagen' }).click();
  const hot = page.getByLabel('Orte aktuell halten, geöffnet innerhalb (Tage)');
  // A save ends with the page reloading the settings; typing before that is overwritten by the
  // saved value, so the next change waits for it
  const reloaded = () =>
    page.waitForResponse((r) => r.request().method() === 'GET' && r.url().endsWith('/api/admin/settings'));
  let settled = reloaded();
  await hot.fill('6');
  await hot.press('Enter');
  await expect
    .poll(async () => (await (await page.request.get('/api/admin/settings')).json()).settings.hotDays)
    .toBe(6);
  await settled;
  await expect(hot).toBeEnabled();
  settled = reloaded();
  await hot.fill('7');
  await hot.press('Enter');
  await expect
    .poll(async () => (await (await page.request.get('/api/admin/settings')).json()).settings.hotDays)
    .toBe(7);
  await settled;

  // The basemap is off on this server (WF_BASEMAP=off): set by the environment, locked
  await menu.getByRole('link', { name: 'Karte und Basemap' }).click();
  await expect(page.getByRole('button', { name: 'Automatisch' })).toBeDisabled();

  // System: the version this server runs
  await menu.getByRole('link', { name: 'System' }).click();
  const health = await (await page.request.get('/api/health')).json();
  await expect(page.getByText(health.version, { exact: true })).toBeVisible();
});

test('on a phone the admin is reached from the account page and fits the screen', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const who = someone('Mo');
  await signUp(page, who);
  makeAdmin(who.email);
  await page.goto('/account');
  // The header has no room for the gear on a phone; the account page links the admin instead
  await expect(page.getByRole('banner').getByRole('link', { name: 'Verwaltung' })).toBeHidden();
  await page.getByRole('main').getByRole('link', { name: 'Verwaltung' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Übersicht' })).toBeVisible();
  const menu = page.getByRole('navigation', { name: 'Bereiche der Verwaltung' });
  for (const section of ['Übersicht', 'Benutzer', 'System']) {
    await menu.getByRole('link', { name: section }).click();
    await expect(page.getByRole('heading', { level: 1, name: section })).toBeVisible();
    // Nothing wider than the phone; the menu and the tables scroll on their own
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
  }
});
