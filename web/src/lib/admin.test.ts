import { describe, expect, it } from 'vitest';
import type { AdminUser } from '@/bindings/AdminUser';
import { bytes, filterUsers, parseTab, signInKind } from './admin';

const user = (u: Partial<AdminUser>): AdminUser => ({
  id: 1,
  name: 'Ann',
  email: null,
  emailConfirmed: false,
  passkeys: 1,
  password: false,
  admin: false,
  disabledAt: null,
  createdAt: 0,
  lastSeenAt: null,
  favorites: 0,
  alarms: 0,
  ...u,
});

describe('admin', () => {
  it('reads the tab from the address', () => {
    expect(parseTab('users')).toBe('users');
    expect(parseTab('nope')).toBe('overview');
    expect(parseTab(undefined, ['users'])).toBe('users');
  });

  it('says sizes in the largest unit', () => {
    expect(bytes(512)).toBe('512 B');
    expect(bytes(323_000_000)).toBe('323 MB');
    expect(bytes(3_900_000_000)).toMatch(/^3[.,]9 GB$/);
  });

  it('filters accounts by name, address and state', () => {
    const now = 400 * 86_400;
    const users = [
      user({ id: 1, name: 'Ann', email: 'ann@x.de', lastSeenAt: now - 86_400 }),
      user({ id: 2, name: 'Ben', disabledAt: 5 }),
      user({ id: 3, name: 'Cem', createdAt: 0 }),
    ];
    expect(filterUsers(users, 'ANN', 'all', now).map((u) => u.id)).toEqual([1]);
    expect(filterUsers(users, 'x.de', 'all', now).map((u) => u.id)).toEqual([1]);
    expect(filterUsers(users, '', 'disabled', now).map((u) => u.id)).toEqual([2]);
    expect(filterUsers(users, '', 'inactive', now).map((u) => u.id)).toEqual([2, 3]);
  });

  it('names how an account signs in', () => {
    expect(signInKind({ passkeys: 2, password: true })).toBe('both');
    expect(signInKind({ passkeys: 0, password: true })).toBe('password');
    expect(signInKind({ passkeys: 0, password: false })).toBe('none');
  });
});
