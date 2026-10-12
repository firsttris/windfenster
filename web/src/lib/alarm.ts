/** An alarm's starting point and how this browser shows things, sent along so the server writes
 * its messages the same way. */
import type { AlarmSettings } from '@/bindings/AlarmSettings';
import type { ClientPrefs } from '@/bindings/ClientPrefs';
import type { GearItem } from '@/bindings/GearItem';
import type { WindUnit } from '@/bindings/WindUnit';
import { getLocale } from '@/paraglide/runtime';

/** Without gear, the least wind most boards and kites want */
const DEFAULT_KN = 12;

/** A new alarm: from the low end of the smallest-wind piece of gear, if there is any. */
export function newAlarm(gear: readonly GearItem[], station: boolean): AlarmSettings {
  const low = gear.length > 0 ? Math.min(...gear.map((g) => g.minKn)) : DEFAULT_KN;
  return { minKn: low, gear: gear.length > 0, forecast: true, live: station, push: true, email: false };
}

export function clientPrefs(unit: WindUnit): ClientPrefs {
  return {
    locale: getLocale() === 'en' ? 'en' : 'de',
    timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone || 'Europe/Berlin',
    unit,
  };
}
