-- The model a spot's forecast opens with: a model's id or 'mix'; NULL for automatic (the one
-- that hits best there, from the station's ranking, else the fixed order).
ALTER TABLE favorite ADD COLUMN model TEXT;
