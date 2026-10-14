/**
 * Accounts for the browser tests: one signed up through the login page, in the page's language,
 * and made an admin on the command line, as an operator does (`windfenster admin`).
 */
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { expect } from './fixtures';

const bin = process.env.WF_BIN || resolve(import.meta.dirname, '../../../target/debug/windfenster');

type Messages = Record<string, string>;
const messages = (lang: 'de' | 'en'): Messages =>
  JSON.parse(readFileSync(resolve(import.meta.dirname, `../../messages/${lang}.json`), 'utf8'));

/** A name and an address of their own for each run, as the server keeps its accounts */
export const someone = (who: string) => {
  const id = Date.now().toString(36);
  return { name: `${who} ${id}`, email: `${who.toLowerCase()}-${id}@example.org` };
};

/** Signs up with e-mail and password; `lang` is the language the page shows */
export async function signUp(page: Page, who: { name: string; email: string }, lang: 'de' | 'en' = 'de') {
  const m = messages(lang);
  await page.goto('/login');
  await page.getByRole('button', { name: m.login_noAccount }).click();
  await page.getByLabel(m.login_name).fill(who.name);
  await page.getByRole('button', { name: m.login_withPassword }).click();
  await page.getByLabel(m.login_email).fill(who.email);
  await page.getByLabel(m.login_password).fill('lang genug 123');
  await page.getByRole('button', { name: m.login_passwordCreate }).click();
  await expect(page.getByRole('link', { name: m.nav_account.replace('{name}', who.name) })).toBeVisible();
}

/** Gives the account the admin role on the e2e server's database (start.mjs notes its folder) */
export function makeAdmin(email: string) {
  const data = readFileSync(join(tmpdir(), 'windfenster-e2e-data-dir'), 'utf8');
  execFileSync(bin, ['admin', email], { env: { ...process.env, WF_DATA_DIR: data, RUST_LOG: 'warn' } });
}
