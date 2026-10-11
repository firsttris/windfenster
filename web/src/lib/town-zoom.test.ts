import { describe, expect, it } from 'vitest';
import { TOWN_FROM, TOWN_TO, townness, zoomOf } from './town-zoom';

describe('town zoom', () => {
  it('reads the zoom from the view', () => {
    // The whole world on 512 pixels is zoom 0; a quarter of it on 1024 pixels zoom 3.
    expect(zoomOf(1, 512)).toBe(0);
    expect(zoomOf(0.25, 1024)).toBe(3);
  });

  it('fades smoothly from the wind to the map', () => {
    expect(townness(5)).toBe(0);
    expect(townness(TOWN_FROM)).toBe(0);
    expect(townness((TOWN_FROM + TOWN_TO) / 2)).toBe(0.5);
    expect(townness(TOWN_TO)).toBe(1);
    expect(townness(16)).toBe(1);
  });
});
