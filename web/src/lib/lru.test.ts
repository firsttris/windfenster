import { describe, expect, it } from 'vitest';
import { lru } from './lru';

describe('lru', () => {
  it('keeps what is read in turn: six frames read round and round all stay', () => {
    // The wind readout reads two hours of three models, in the same order every time
    const cache = lru<string, number>(8);
    const frames = ['ecmwf a', 'ecmwf b', 'icon a', 'icon b', 'd2 a', 'd2 b'];
    let decoded = 0;
    const read = (f: string) => {
      if (cache.get(f) !== undefined) return;
      decoded++;
      cache.set(f, decoded);
    };
    for (let round = 0; round < 5; round++) for (const f of frames) read(f);
    expect(decoded).toBe(6);
  });

  it('lets the entry used longest ago go first', () => {
    const cache = lru<string, number>(2);
    cache.set('a', 1);
    cache.set('b', 2);
    cache.get('a');
    cache.set('c', 3);
    expect([cache.get('a'), cache.get('b'), cache.get('c')]).toEqual([1, undefined, 3]);
    expect(cache.size).toBe(2);
  });
});
