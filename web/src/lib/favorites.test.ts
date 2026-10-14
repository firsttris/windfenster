import { describe, expect, it } from 'vitest';
import type { Favorite } from '@/bindings/Favorite';
import { COMPASS_DEGREES, findFavorite, inSector } from './favorites';

const list: Favorite[] = [
  { id: 1, slug: 'edsb', lat: 48.7794, lon: 8.0805, name: 'Baggersee', sector: null, model: null },
  { id: 2, slug: null, lat: 36.41, lon: 28.09, name: 'Prasonisi', sector: { from: 270, to: 30 }, model: null },
];

describe('favourites', () => {
  it('finds the favourite of a station or a point on the 0.01° grid', () => {
    expect(findFavorite(list, { slug: 'edsb' })?.id).toBe(1);
    expect(findFavorite(list, { lat: 36.4123, lon: 28.0876 })?.id).toBe(2);
    expect(findFavorite(list, { lat: 36.43, lon: 28.09 })).toBeUndefined();
    expect(findFavorite(list, { slug: 'lgrp' })).toBeUndefined();
    // Rounded as the server rounds: halves away from zero, also south and west
    const south: Favorite = { id: 3, slug: null, lat: -33.87, lon: -70.67, name: 'Sur', sector: null, model: null };
    expect(findFavorite([south], { lat: -33.865, lon: -70.665 })?.id).toBe(3);
  });

  it('has the compass points in degrees', () => {
    expect(COMPASS_DEGREES).toHaveLength(16);
    expect([COMPASS_DEGREES[0], COMPASS_DEGREES[4], COMPASS_DEGREES[11]]).toEqual([0, 90, 248]);
  });

  it('knows whether wind comes from the sector, also across north', () => {
    const s = { from: 270, to: 30 };
    expect(inSector(s, 0)).toBe(true);
    expect(inSector(s, 300)).toBe(true);
    expect(inSector(s, 90)).toBe(false);
    expect(inSector({ from: 200, to: 290 }, 245)).toBe(true);
  });
});
