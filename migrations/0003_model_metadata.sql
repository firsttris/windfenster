-- Model data checked against Open-Meteo's run metadata (meta.json, 2026-10-11): the DMI
-- HARMONIE domain ends at 62.67° N, AROME France HD reaches 51 hours.
UPDATE model SET bbox_west = -25.42, bbox_south = 39.67, bbox_east = 40.07, bbox_north = 62.67
WHERE id = 'dmi_harmonie_arome_europe';
UPDATE model SET max_lead_h = 51 WHERE id = 'meteofrance_arome_france_hd';
