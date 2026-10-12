import { describe, expect, it } from 'vitest';
import { newAlarm } from './alarm';
import { keyBytes, targetOf } from './push';

describe('a new alarm', () => {
  it('starts where the gear starts, or at 12 kn', () => {
    const wing = { kind: 'wing', size: 5, minKn: 14, maxKn: 22 } as const;
    const kite = { kind: 'kite', size: 12, minKn: 11, maxKn: 19 } as const;
    expect(newAlarm([wing, kite], true)).toEqual({
      minKn: 11,
      gear: true,
      forecast: true,
      live: true,
      push: true,
      email: false,
    });
    expect(newAlarm([], false)).toMatchObject({ minKn: 12, gear: false, live: false });
  });
});

describe('push in the browser', () => {
  it('reads the server key and keeps what the server needs', () => {
    expect([...keyBytes('AQID_-8')]).toEqual([1, 2, 3, 255, 239]);
    expect(targetOf({ endpoint: 'https://p/1', keys: { p256dh: 'k', auth: 'a' } })).toEqual({
      endpoint: 'https://p/1',
      p256dh: 'k',
      auth: 'a',
    });
    expect(targetOf({ endpoint: 'https://p/1' })).toBeNull();
  });
});
