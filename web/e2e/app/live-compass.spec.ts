/** The measured wind on a compass in "Jetzt gemessen", and for a saved spot whether its
 * direction works there. */
import { signUp, someone } from './accounts';
import { expect, test } from './fixtures';

test("the measured wind on a compass, with the spot's directions", async ({ page }) => {
  await page.goto('/station/edsb');
  const card = page.getByRole('article', { name: 'Jetzt gemessen' });
  const live = await (await page.request.get('/api/stations/edsb/live')).json();
  const dir: number = live.observations.at(-1).dir;
  await expect(card.getByRole('img', { name: /^Windrose: Wind aus / })).toBeVisible();
  // Not a saved spot: nothing to say about the directions
  await expect(card.getByText(/Richtung passt/)).toHaveCount(0);

  await signUp(page, someone('Robin'));
  const around = { from: (dir + 315) % 360, to: (dir + 45) % 360 };
  const r = await page.request.post('/api/favorites', {
    data: { slug: 'edsb', lat: 0, lon: 0, name: 'Baggersee', sector: around },
  });
  expect(r.status()).toBe(201);
  await page.goto('/station/edsb');
  await expect(card.getByText('Richtung passt am Spot', { exact: true })).toBeVisible();
  await expect(card.getByRole('img', { name: /am Spot gehen/ })).toBeVisible();

  // The opposite directions do not work
  const fav = (await (await page.request.get('/api/favorites')).json())[0];
  const opposite = { from: (dir + 135) % 360, to: (dir + 225) % 360 };
  await page.request.put(`/api/favorites/${fav.id}`, { data: { ...fav, sector: opposite } });
  await page.goto('/station/edsb');
  await expect(card.getByText('Richtung passt am Spot nicht')).toBeVisible();
});
