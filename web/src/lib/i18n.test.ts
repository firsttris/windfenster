import { describe, expect, it } from 'vitest';
import de from '../../messages/de.json';
import en from '../../messages/en.json';

const keys = (o: object) =>
  Object.keys(o)
    .filter((k) => k !== '$schema')
    .sort();

describe('messages', () => {
  it('have the same keys in every language', () => {
    expect(keys(en)).toEqual(keys(de));
  });

  it('have the same placeholders in every language', () => {
    const placeholders = (v: unknown) => [...JSON.stringify(v).matchAll(/\{(\w+)\}/g)].map((x) => x[1]).sort();
    for (const k of keys(de)) {
      expect(placeholders((en as Record<string, unknown>)[k]), k).toEqual(
        placeholders((de as Record<string, unknown>)[k]),
      );
    }
  });
});
