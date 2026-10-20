/** Two accounts one after the other in one tab (a shared device): what the first one had on its
 * pages is gone when the next one signs in, without a reload. */
import { signUp, someone } from './accounts';
import { expect, test } from './fixtures';

test('after signing out and in as another account, the passkey list is the new one', async ({ page }) => {
  const a = someone('Anna');
  await signUp(page, a);
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
  await page.getByRole('link', { name: `Konto von ${a.name}` }).click();
  await page.getByRole('button', { name: 'Passkey auf diesem Gerät hinzufügen' }).click();
  await expect(page.getByText('1 Passkey für dieses Konto')).toBeVisible();
  await page.getByRole('button', { name: 'Chrome auf Windows umbenennen' }).click();
  await page.getByLabel('Name des Passkeys').fill('Annas Geheimgerät');
  await page.getByLabel('Name des Passkeys').press('Enter');
  await expect(page.getByText('Annas Geheimgerät')).toBeVisible();

  // The device's passkeys away, so the login page does not sign Anna in again
  await cdp.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId });
  // Out (the SPA navigates to the start page), then a new account in the same tab, no reload
  await page.getByRole('button', { name: 'Abmelden', exact: true }).click();
  await page.getByRole('link', { name: 'Anmelden' }).click();
  const b = someone('Bert');
  await page.getByRole('button', { name: 'Ich bin neu hier' }).click();
  await page.getByLabel('Dein Name').fill(b.name);
  await page.getByRole('button', { name: 'Mit E-Mail und Passwort' }).click();
  await page.getByLabel('E-Mail').fill(b.email);
  await page.getByLabel('Passwort').fill('lang genug 123');
  await page.getByRole('button', { name: 'Konto mit Passwort anlegen' }).click();
  await page.getByRole('link', { name: `Konto von ${b.name}` }).click();
  await expect(page.getByText('0 Passkeys für dieses Konto')).toBeVisible();
  // Bert has no passkey: Anna's must not be listed
  await expect(page.getByText('Annas Geheimgerät')).toHaveCount(0);
});
