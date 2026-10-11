/**
 * Coordinates typed into the search: "48.78, 8.08", "48,78 8,08", "48.78;8.08". Latitude
 * first. Returns null for anything else or values out of range.
 */
export function parseCoordinates(text: string): { lat: number; lon: number } | null {
  const m = /^\s*(-?\d{1,2}(?:[.,]\d+)?)°?\s*(?:[;,]\s*|\s+)(-?\d{1,3}(?:[.,]\d+)?)°?\s*$/.exec(text);
  if (!m) return null;
  const lat = Number(m[1].replace(',', '.'));
  const lon = Number(m[2].replace(',', '.'));
  if (Math.abs(lat) > 90 || Math.abs(lon) > 180) return null;
  return { lat, lon };
}
