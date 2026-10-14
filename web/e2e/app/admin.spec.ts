/**
 * The admin page against the real server: an account made an admin on the command line, as an
 * operator does (`windfenster admin`), then every section of the page, a setting changed and
 * changed back, and someone who is no admin kept out.
 */
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { expect, test } from './fixtures';

const bin = process.env.WF_BIN || resolve(import.meta.dirname, '../../../target/debug/windfenster');

/** A name and an address of their own for each run, as the server keeps its accounts */
const someone = (who: string) => {
  const id = Date.now().toString(36);
  return { name: `${who} ${id}`, email: `${who.toLowerCase()}-${id}@example.org` };
};

async function signUp(page: Page, who: { name: string; email: string }) {
  await page.goto('/login');
  await page.getByRole('button', { name: 'Ich bin neu hier' }).click();
  await page.getByLabel('Dein Name').fill(who.name);
  await page.getByRole('button', { name: 'Mit E-Mail und Passwort' }).click();
  await page.getByLabel('E-Mail').fill(who.email);
  await page.getByLabel('Passwort').fill('lang genug 123');
  await page.getByRole('button', { name: 'Konto mit Passwort anlegen' }).click();
  await expect(page.getByRole('link', { name: `Konto von ${who.name}` })).toBeVisible();
}

function makeAdmin(email: string) {
  const data = readFileSync(join(tmpdir(), 'windfenster-e2e-data-dir'), 'utf8');
  execFileSync(bin, ['admin', email], { env: { ...process.env, WF_DATA_DIR: data, RUST_LOG: 'warn' } });
}

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
  await hot.fill('6');
  await hot.press('Enter');
  await expect
    .poll(async () => (await (await page.request.get('/api/admin/settings')).json()).settings.hotDays)
    .toBe(6);
  await hot.fill('7');
  await hot.press('Enter');
  await expect
    .poll(async () => (await (await page.request.get('/api/admin/settings')).json()).settings.hotDays)
    .toBe(7);

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
