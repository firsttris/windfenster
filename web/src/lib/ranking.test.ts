import { describe, expect, it } from 'vitest';
import type { Model } from '@/bindings/Model';
import type { ModelScore } from '@/bindings/ModelScore';
import type { Ranking } from '@/bindings/Ranking';
import { byRank, chipRanks } from './ranking';

const model = (id: string): Model => ({
  id,
  name: id,
  provider: '',
  resolutionKm: 2,
  class: 'super',
  domain: null,
  maxLeadH: 48,
  runsPerDay: 8,
  rank: 0,
});

const score = (model: string, mae: number, days = 20): ModelScore => ({
  model,
  n: 100,
  days,
  mae,
  bias: 0,
  dirError: null,
  hits: 0,
  misses: 0,
  falseAlarms: 0,
});

const ranking = (scores: ModelScore[]): Ranking => ({
  days: Math.max(0, ...scores.map((s) => s.days)),
  scores,
  bestSuper: null,
  bestGlobal: null,
  mixFirst: false,
});

describe('ranks on the chips', () => {
  it('numbers the places as the ranking card does, the TAF included', () => {
    const ranks = chipRanks(ranking([score('arome', 1.6), score('taf', 1.8), score('icon_d2', 2.1, 5)]));
    expect(ranks.get('arome')).toEqual({ rank: 1, mae: 1.6, settled: true });
    expect(ranks.get('icon_d2')).toEqual({ rank: 3, mae: 2.1, settled: false });
    expect(chipRanks(null).size).toBe(0);
  });

  it('orders the settled places first and keeps the rest as given', () => {
    const models = ['icon_d2', 'ch1', 'arome', 'harmonie'].map(model);
    const ranks = chipRanks(ranking([score('ch1', 1.2, 3), score('harmonie', 1.5), score('arome', 1.9)]));
    expect(byRank(models, ranks).map((m) => m.id)).toEqual(['harmonie', 'arome', 'icon_d2', 'ch1']);
    expect(byRank(models, new Map()).map((m) => m.id)).toEqual(['icon_d2', 'ch1', 'arome', 'harmonie']);
  });
});
