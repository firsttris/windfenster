import { describe, expect, it } from 'vitest';
import type { Forecast } from '@/bindings/Forecast';
import { modelColor } from './model-color';

const page = (ids: string[]): Forecast =>
  ({ models: [{ id: 'mix' }, ...ids.map((id) => ({ id }))] }) as unknown as Forecast;

// The models /api/point returns at Lake Garda (45.65, 10.7), in rank order
const garda = [
  'icon_d2',
  'italia_meteo_arpae_icon_2i',
  'meteofrance_arome_france_hd',
  'dmi_harmonie_arome_europe',
  'ecmwf_ifs025',
  'icon_eu',
  'gfs_global',
];

describe('model colours', () => {
  it('gives each model on a page its own colour', () => {
    const fc = page(garda);
    expect(new Set(garda.map((id) => modelColor(fc, id))).size).toBe(garda.length);
  });

  it('keeps a model its colour from place to place', () => {
    const rhodes = page(['ecmwf_ifs025', 'icon_eu', 'gfs_global']);
    expect(modelColor(rhodes, 'ecmwf_ifs025')).toBe(modelColor(page(garda), 'ecmwf_ifs025'));
  });

  it('gives a model without a slot a free one', () => {
    const fc = page([...garda, 'new_a', 'new_b']);
    const colours = [...garda, 'new_a', 'new_b'].map((id) => modelColor(fc, id));
    expect(new Set(colours).size).toBe(colours.length);
    expect(modelColor(fc, 'mix')).toBe('var(--chart-mix)');
  });
});
