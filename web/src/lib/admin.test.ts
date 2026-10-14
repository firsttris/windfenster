import { describe, expect, it } from 'vitest';
import type { AdminUser } from '@/bindings/AdminUser';
import {
  buildDate,
  bytes,
  cadence,
  diskUsed,
  filterUsers,
  parseRegion,
  parseTab,
  percent,
  regionText,
  remaining,
  signInKind,
  span,
  took,
} from './admin';

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

describe('admin times', () => {
  it('names how often a task runs', () => {
    expect(cadence(300)).toEqual({ unit: 'min', n: 5 });
    expect(cadence(540)).toEqual({ unit: 'min', n: 9 });
    expect(cadence(3_600)).toEqual({ unit: 'hour', n: 1 });
    expect(cadence(86_400)).toEqual({ unit: 'day', n: 1 });
    expect(cadence(7 * 86_400)).toEqual({ unit: 'week', n: 1 });
  });

  it('says how long a run took', () => {
    expect(took(800)).toMatch(/^0[.,]8 s$/);
    expect(took(48_000)).toBe('48 s');
    expect(took(180_000)).toBe('3 min');
  });

  it('says spans and shares', () => {
    expect(span(6 * 86_400 + 5)).toMatch(/6/);
    expect(span(30)).toMatch(/1/);
    expect(percent(3412, 9000)).toBe(38);
    expect(percent(5, 0)).toBe(0);
  });
});

describe('admin disk', () => {
  it('adds up the parts', () => {
    expect(
      diskUsed({
        free: 99,
        models: [
          ['icon_d2', 10],
          ['ecmwf_ifs025', 5],
        ],
        basemap: 1,
        terrain: 2,
        spots: 3,
        spotFiles: 1,
        database: 4,
        partial: 5,
        other: 6,
      }),
    ).toBe(36);
  });
});

describe('admin map', () => {
  it('reads builds, regions and what is left', () => {
    expect(buildDate('20261013')).toMatch(/2026/);
    expect(buildDate('nope')).toBe('nope');
    expect(regionText([-25, 27, 45, 72])).toBe('-25,27,45,72');
    expect(regionText(null)).toBe('');
    expect(parseRegion(' 7.5, 47.5, 9, 49.5 ')).toEqual([7.5, 47.5, 9, 49.5]);
    expect(parseRegion('')).toBeNull();
    expect(parseRegion('9,47,7,49')).toBeUndefined();
    expect(parseRegion('1,2,3')).toBeUndefined();
    expect(remaining(25, 100, 5)).toBe(15);
    expect(remaining(100, 100, 5)).toBeNull();
    expect(remaining(1, 100, 0)).toBeNull();
  });
});
