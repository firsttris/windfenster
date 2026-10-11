-- A run taken from the model's schedule because Open-Meteo's meta.json did not answer. The
-- next run detection that reaches meta.json puts the real initialisation time in its place.
ALTER TABLE model_run ADD COLUMN estimated INTEGER NOT NULL DEFAULT 0;
