import { expect, describe as group, it } from 'vitest';
import { decodeMetar } from './metar';
import { type Context, describe, summary } from './metar-text';

// 2026-10-11 08:50 UTC, at Baden Airpark (Europe/Berlin, summer time: 10:50 local)
const AT = 1_791_708_600;
const ctx: Context = { unit: 'kn', at: AT, timeZone: 'Europe/Berlin' };
const words = (raw: string, c = ctx) => decodeMetar(raw).map((g) => describe(g, c));

group('a METAR in words', () => {
  it('reads a fair-weather report', () => {
    expect(words('METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015')).toEqual([
      '11. Okt., 08:50 UTC = 10:50 Uhr',
      'automatisch, ohne Beobachter',
      'Wind aus 210° (SSW), 9 kn',
      'Sicht 10 km oder mehr, keine Wolken unter 5.000 ft, kein Wetter',
      '13 °C, Taupunkt 10 °C',
      'Luftdruck (QNH) 1015 hPa',
    ]);
  });

  it('reads gusts, shifting wind, weather, clouds and a trend', () => {
    const got = words('METAR LGRP 111150Z 32018G28KT 290V350 4000 -SHRA FEW020CB BKN040 22/17 Q1009 TEMPO 32025G35KT');
    expect(got).toContain('Wind aus 320° (NW), 18 kn, Böen 28 kn');
    expect(got).toContain('Richtung wechselt zwischen 290° und 350°');
    expect(got).toContain('Sicht 4.000 m');
    expect(got).toContain('Regen (Schauer, leicht)');
    expect(got).toContain('wenige Wolken in 2.000 ft, Gewitterwolken');
    expect(got).toContain('durchbrochen in 4.000 ft');
    expect(got).toContain('in den nächsten 2 Stunden zeitweise:');
    // Nothing left untranslated
    expect(got).not.toContain('nicht übersetzt');
  });

  it('writes the wind in the unit chosen', () => {
    const got = words('METAR EDSB 110850Z 21010G20KT 9999 NSC 13/10 Q1015', { ...ctx, unit: 'ms' });
    expect(got).toContain('Wind aus 210° (SSW), 5,1 m/s, Böen 10,3 m/s');
  });

  it('reads calm air, variable wind, poor sight and an obscured sky', () => {
    expect(words('METAR EDSB 110850Z 00000KT 0300 FG VV002 08/08 Q1021')).toEqual([
      '11. Okt., 08:50 UTC = 10:50 Uhr',
      'windstill',
      'Sicht 300 m',
      'Nebel',
      'Himmel nicht erkennbar, Vertikalsicht 200 ft',
      '8 °C, Taupunkt 8 °C',
      'Luftdruck (QNH) 1021 hPa',
    ]);
    expect(words('METAR EDSB 110850Z VRB03KT 9999 SKC 13/M02 Q1015')[1]).toBe('umlaufender Wind, 3 kn');
  });

  it('picks the chips next to the measured wind, before any trend', () => {
    const groups = decodeMetar('METAR EDSB 110850Z 21009KT 9999 FEW030 BKN045 13/10 Q1015 BECMG 4000 RA');
    expect(summary(groups).map((g) => describe(g, ctx))).toEqual([
      '13 °C, Taupunkt 10 °C',
      'Luftdruck (QNH) 1015 hPa',
      'Sicht 10 km oder mehr',
      'wenige Wolken in 3.000 ft',
    ]);
  });
});
