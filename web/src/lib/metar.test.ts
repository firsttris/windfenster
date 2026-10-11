import { describe, expect, it } from 'vitest';
import { decodeMetar, type Group } from './metar';

const kinds = (gs: Group[]) => gs.map((g) => g.kind);
const find = <K extends Group['kind']>(gs: Group[], kind: K) =>
  gs.find((g): g is Extract<Group, { kind: K }> => g.kind === kind);

// Reports from aviationweather.gov's cache, 2026-10-11.
describe('decodeMetar', () => {
  it('reads a plain report', () => {
    const gs = decodeMetar('METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015');
    expect(kinds(gs)).toEqual(['time', 'auto', 'wind', 'cavok', 'temperature', 'pressure']);
    expect(find(gs, 'time')).toMatchObject({ day: 11, hour: 8, minute: 50 });
    expect(find(gs, 'wind')).toMatchObject({ dir: 210, kn: 9, gustKn: null, token: '21009KT' });
    expect(find(gs, 'temperature')).toMatchObject({ temp: 13, dew: 10 });
    expect(find(gs, 'pressure')).toMatchObject({ hpa: 1015, inHg: null });
  });

  it('reads variation, weather, clouds and the trend', () => {
    const gs = decodeMetar(
      'METAR LYNI 110900Z 10003KT 060V140 4000 RA FEW017 BKN037 OVC090 16/15 Q1009 TEMPO VRB15G25KT 2000 TSRA SCT030CB BKN040',
    );
    expect(kinds(gs)).toEqual([
      'time',
      'wind',
      'windVariation',
      'visibility',
      'weather',
      'clouds',
      'clouds',
      'clouds',
      'temperature',
      'pressure',
      'trend',
      'wind',
      'visibility',
      'weather',
      'clouds',
      'clouds',
    ]);
    expect(find(gs, 'windVariation')).toMatchObject({ from: 60, to: 140 });
    expect(find(gs, 'visibility')).toMatchObject({ meters: 4000, atLeast: false });
    expect(find(gs, 'weather')).toMatchObject({ intensity: null, descriptor: null, phenomena: ['RA'] });
    expect(gs[11]).toMatchObject({ kind: 'wind', dir: null, kn: 15, gustKn: 25 });
    expect(gs[13]).toMatchObject({ kind: 'weather', descriptor: 'TS', phenomena: ['RA'] });
    expect(gs[14]).toMatchObject({ kind: 'clouds', cover: 'SCT', feet: 3000, type: 'CB' });
  });

  it('reads fog, runway visual range and vertical visibility', () => {
    const gs = decodeMetar('METAR LFBX 110900Z AUTO VRB02KT 0500 0450 R29/0650U FG VV/// 09/09 Q1017');
    expect(kinds(gs)).toEqual([
      'time',
      'auto',
      'wind',
      'visibility',
      'visibility',
      'rvr',
      'weather',
      'verticalVisibility',
      'temperature',
      'pressure',
    ]);
    expect(find(gs, 'rvr')).toMatchObject({ runway: '29' });
    expect(find(gs, 'verticalVisibility')).toMatchObject({ feet: null });
  });

  it('reads frost, light snow showers and remarks', () => {
    const gs = decodeMetar(
      'METAR ENSS 110850Z AUTO 27009KT 1100 R33/P2000 -SHSN NCD 00/M01 Q1011 RMK WIND 0500FT 28012KT',
    );
    expect(find(gs, 'weather')).toMatchObject({ intensity: 'light', descriptor: 'SH', phenomena: ['SN'] });
    expect(find(gs, 'sky')).toMatchObject({ code: 'NCD' });
    expect(find(gs, 'temperature')).toMatchObject({ temp: 0, dew: -1 });
    expect(gs.at(-1)).toEqual({ kind: 'remarks', token: 'RMK WIND 0500FT 28012KT' });
    expect(find(decodeMetar('METAR ENRO 110850Z AUTO 12009KT 9999 -SN NCD M00/M03 Q1007'), 'temperature')).toEqual({
      kind: 'temperature',
      token: 'M00/M03',
      temp: 0,
      dew: -3,
    });
  });

  it('reads missing sensor values and the sea state of an offshore platform', () => {
    const gs = decodeMetar(
      'METAR EHQN 110855Z AUTO 31023G33KT 290V010 //// // FEW022/// SCT031/// BKN038/// 13/09 Q1008 RE// W18/H26',
    );
    expect(kinds(gs)).toEqual([
      'time',
      'auto',
      'wind',
      'windVariation',
      'notObserved',
      'notObserved',
      'clouds',
      'clouds',
      'clouds',
      'temperature',
      'pressure',
      'notObserved',
      'sea',
    ]);
    expect(find(gs, 'wind')).toMatchObject({ dir: 310, kn: 23, gustKn: 33 });
    expect(find(gs, 'clouds')).toMatchObject({ cover: 'FEW', feet: 2200, type: null });
    expect(find(gs, 'sea')).toMatchObject({ temp: 18, state: null, waveM: 2.6 });
    expect(kinds(decodeMetar('METAR LOAN 110900Z 28005KT 9999 FEW030 ///// Q////'))).toEqual([
      'time',
      'wind',
      'visibility',
      'clouds',
      'notObserved',
      'notObserved',
    ]);
  });

  // ESNG, KSEE and EHKD are real; the m/s, wind shear and SPECI reports are written by hand.
  it('reads calm, m/s, US forms, colour states and wind shear', () => {
    expect(find(decodeMetar('METAR ESNG 110850Z 00000KT 9999 NSC M01/M07 Q1016'), 'wind')).toMatchObject({
      dir: null,
      kn: 0,
    });
    expect(find(decodeMetar('METAR UUEE 110900Z 24005MPS 9999 BKN020 08/04 Q1012 NOSIG'), 'wind')).toMatchObject({
      dir: 240,
      kn: 10,
    });
    const us = decodeMetar('METAR KSEE 110855Z AUTO 17005G11KT 10SM SCT030 20/18 A2945 RMK AO2');
    expect(find(us, 'visibilityMiles')).toMatchObject({ miles: 10, atLeast: false });
    expect(find(us, 'pressure')).toMatchObject({ hpa: 997, inHg: 29.45 });
    const mil = decodeMetar('METAR EHKD 110855Z AUTO 29014KT 240V320 9999 SCT027 BKN031 BKN036 13/08 Q1007 BLU NOSIG');
    expect(find(mil, 'colour')).toMatchObject({ code: 'BLU' });
    expect(find(mil, 'visibility')).toMatchObject({ atLeast: true });
    const ws = decodeMetar('METAR LGAV 110850Z 02015KT 9999 FEW030 22/12 Q1015 WS R03R NOSIG');
    expect(find(ws, 'windShear')).toMatchObject({ token: 'WS R03R' });
    expect(kinds(decodeMetar('SPECI COR EDDF 110850Z 21009KT 9999 XYZ 13/10 Q1015='))).toEqual([
      'time',
      'wind',
      'visibility',
      'unknown',
      'temperature',
      'pressure',
    ]);
  });
});
