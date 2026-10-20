import { describe, expect, it } from 'vitest';
import type { Model } from '@/bindings/Model';
import { SLOT } from './model-color';
import { inOrder, MODEL_GUIDE, REGIONS, running } from './model-guide';

const model = (id: string, cls: 'super' | 'global', rank: number): Model => ({
  id,
  name: id,
  provider: 'x',
  resolutionKm: 1,
  class: cls,
  domain: null,
  maxLeadH: 48,
  runsPerDay: 8,
  rank,
});

describe('the models page', () => {
  it('has texts for every model Windfenster knows, and areas only for regional ones', () => {
    expect(Object.keys(MODEL_GUIDE).sort()).toEqual(Object.keys(SLOT).sort());
    for (const id of ['ecmwf_ifs025', 'gfs_global', 'ukmo_global_deterministic_10km'])
      expect(MODEL_GUIDE[id].area).toBeUndefined();
    expect(MODEL_GUIDE.icon_d2.good()).toMatch(/Nord- und Ostsee/);
  });

  it('names only models it has texts for', () => {
    for (const r of REGIONS) for (const id of [...r.soon, ...r.later]) expect(MODEL_GUIDE[id], id).toBeDefined();
  });

  it('leaves out models this server does not run, and orders the fine ones first', () => {
    const models = [model('gfs_global', 'global', 50), model('icon_d2', 'super', 10), model('icon_eu', 'global', 46)];
    expect(running(['icon_d2', 'dmi_harmonie_arome_europe'], models).map((x) => x.id)).toEqual(['icon_d2']);
    expect(inOrder(models).map((x) => x.id)).toEqual(['icon_d2', 'icon_eu', 'gfs_global']);
  });
});
