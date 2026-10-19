import { describe, expect, it } from 'vitest';
import { passkeyLabel } from './passkey-label';

describe('a passkey’s label', () => {
  it('is the own name, else browser and system, else what is known', () => {
    expect(passkeyLabel({ name: 'Pixel', browser: 'Chrome', os: 'Android' })).toBe('Pixel');
    expect(passkeyLabel({ name: null, browser: 'Chrome', os: 'Android' })).toBe('Chrome auf Android');
    expect(passkeyLabel({ name: null, browser: null, os: 'iOS' })).toBe('iOS');
    expect(passkeyLabel({ name: null, browser: null, os: null })).toBe('Passkey');
  });
});
