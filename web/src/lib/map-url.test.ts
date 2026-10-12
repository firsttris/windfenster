import { describe, expect, it } from 'vitest';
import { mapHref, mapQuery, parseMapState, parseSpot } from './map-url';

describe('the map in the address', () => {
  it('reads hour, layer and model', () => {
    expect(parseMapState('?t=1791730000&layer=gusts&model=icon_d2')).toEqual({
      time: 1791730800,
      gusts: true,
      model: 'icon_d2',
    });
    expect(parseMapState('')).toEqual({});
    expect(parseMapState('?t=x&model=<script>')).toEqual({});
  });

  it('writes only what differs from the defaults, keeping other parameters', () => {
    expect(mapQuery('', { time: null, gusts: false, model: null })).toBe('');
    expect(mapQuery('?utm=a', { time: 1791730800, gusts: true, model: 'ecmwf_ifs025' })).toBe(
      '?utm=a&t=1791730800&layer=gusts&model=ecmwf_ifs025',
    );
    expect(mapQuery('?t=1&layer=gusts', { time: null, gusts: false, model: null })).toBe('');
  });

  it('opens the map on a place, once', () => {
    const href = mapHref({ lat: 48.7793, lon: 8.0805, slug: 'edsb' });
    expect(href).toBe('/map?spot=48.7793%2C8.0805&station=edsb#12/48.7793/8.0805');
    expect(parseSpot(href.slice(4, href.indexOf('#')))).toEqual({ lat: 48.7793, lon: 8.0805, slug: 'edsb' });
    expect(parseSpot('?spot=1,2&station=<x>')).toEqual({ lat: 1, lon: 2, slug: null });
    expect(parseSpot('?spot=x')).toBeNull();
    expect(parseSpot('')).toBeNull();
    expect(mapQuery('?spot=1,2&station=edsb', { time: null, gusts: false, model: null })).toBe('');
  });
});
