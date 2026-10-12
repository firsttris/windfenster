/**
 * Accounts against the real server: signing up and in with a passkey (Chrome's virtual
 * authenticator stands in for the phone or the password manager; it also takes the passkey the
 * sign-in page offers in its e-mail field) and with e-mail and password, saving a spot, its alarm,
 * the account page.
 */
import type { Page } from '@playwright/test';
import { expect, test } from './fixtures';

/** A passkey store in the browser, as a phone's or a password manager's would be */
async function authenticator(page: Page) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  const { authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  return { cdp, authenticatorId };
}

/** A name of its own for each run, as the server keeps its accounts between tests */
const someone = (who: string) => `${who} ${Date.now().toString(36)}`;

test('a passkey account: sign up, save a spot with its alarm, sign out and in again', async ({ page }) => {
  const { cdp, authenticatorId } = await authenticator(page);
  const name = someone('Kim');

  // A spot needs an account: the button leads to the sign-up and back
  await page.goto('/station/edsb');
  await page.getByRole('link', { name: 'Als Spot merken' }).click();
  await expect(page).toHaveURL(/\/login\?redirect=%2Fstation%2Fedsb/);
  await page.getByRole('button', { name: 'Ich bin neu hier' }).click();
  await page.getByLabel('Dein Name').fill(name);
  await page.getByRole('button', { name: 'Konto mit Passkey anlegen' }).click();
  await expect(page).toHaveURL(/\/station\/edsb$/);
  const { credentials } = await cdp.send('WebAuthn.getCredentials', { authenticatorId });
  expect(credentials).toHaveLength(1);
  await expect(page.getByRole('link', { name: `Konto von ${name}` })).toBeVisible();

  // Save the spot, with its own name
  await page.getByRole('button', { name: 'Als Spot merken' }).click();
  const spot = page.getByRole('dialog', { name: 'Dein Spot' });
  await spot.getByLabel('Name').fill('Baden Airpark');
  await spot.getByRole('button', { name: 'Speichern' }).click();
  await expect(page.getByRole('button', { name: 'Gemerkt' })).toBeVisible();
  // The quick view now marks the hours that work at the spot
  await expect(page.getByRole('region', { name: 'Auf einen Blick' })).toContainText(
    'Grün: Stunden, die bei Baden Airpark gehen',
  );

  // Its alarm: on this server neither push (no browser permission) nor e-mail (no SMTP) is set
  // up, so saving needs a way first and says so
  await page.getByRole('button', { name: 'Alarm', exact: true }).click();
  const alarm = page.getByRole('dialog', { name: /Alarm für/ });
  await expect(alarm).toBeVisible();
  await expect(alarm.getByText('E-Mails sind auf diesem Server nicht eingerichtet.')).toBeVisible();
  await alarm.press('Escape');

  // The home page lists it
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Deine Spots' })).toBeVisible();
  await expect(page.getByRole('link', { name: /Baden Airpark/ }).first()).toBeVisible();

  // Out, and in again with the same passkey
  await page.getByRole('link', { name: `Konto von ${name}` }).click();
  await expect(page.getByRole('heading', { name: 'Dein Konto' })).toBeVisible();
  await expect(page.getByText('1 Passkey für dieses Konto')).toBeVisible();
  await page.getByRole('button', { name: 'Abmelden' }).click();
  await expect(page.getByRole('link', { name: 'Anmelden' })).toBeVisible();
  // The sign-in page offers the passkey in the e-mail field as soon as it opens (autofill); the
  // virtual authenticator takes it at once, as a tap on the offer would
  await page.goto('/login');
  await expect(page.getByRole('link', { name: `Konto von ${name}` })).toBeVisible();
  await expect(page).toHaveURL(/\/$/);
});

test('an account with e-mail and password', async ({ page }) => {
  const email = `${Date.now().toString(36)}@example.org`;
  await page.goto('/login');
  await page.getByRole('button', { name: 'Ich bin neu hier' }).click();
  await page.getByLabel('Dein Name').fill(someone('Alex'));
  await page.getByRole('button', { name: 'Mit E-Mail und Passwort' }).click();
  await page.getByLabel('E-Mail').fill(email);
  // Too short first: the browser does not send it
  await page.getByLabel('Passwort').fill('kurz');
  await page.getByRole('button', { name: 'Konto mit Passwort anlegen' }).click();
  expect(await page.getByLabel('Passwort').evaluate((el) => (el as HTMLInputElement).validity.valid)).toBe(false);
  await expect(page).toHaveURL(/\/login/);
  await page.getByLabel('Passwort').fill('lang genug 123');
  await page.getByRole('button', { name: 'Konto mit Passwort anlegen' }).click();
  await expect(page.getByRole('link', { name: /^Konto von Alex/ })).toBeVisible();

  // Out, then a wrong password, then the right one
  await page.getByRole('link', { name: /^Konto von Alex/ }).click();
  await page.getByRole('button', { name: 'Abmelden' }).click();
  await page.goto('/login');
  await page.getByLabel('E-Mail').fill(email);
  await page.getByLabel('Passwort').fill('falsches passwort');
  await page.getByRole('button', { name: 'Anmelden', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('E-Mail oder Passwort stimmen nicht.');
  await page.getByLabel('Passwort').fill('lang genug 123');
  await page.getByRole('button', { name: 'Anmelden', exact: true }).click();
  await expect(page.getByRole('link', { name: /^Konto von Alex/ })).toBeVisible();
});
